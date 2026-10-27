pub(crate) mod permission;
use crate::media::Asset;
use anyhow::{Context, Result, ensure};
use ashpd::desktop::{
    PersistMode,
    clipboard::Clipboard,
    global_shortcuts::{GlobalShortcuts, NewShortcut},
    remote_desktop::{DeviceType, KeyState, RemoteDesktop},
};
use futures_util::StreamExt;
use rust_i18n::t;
use tokio::{
    io::AsyncWriteExt,
    sync::{mpsc, oneshot},
};

async fn register_application() -> Result<()> {
    static REGISTERED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
    REGISTERED
        .get_or_try_init(|| async {
            match ashpd::register_host_app("app.picora.desktop".parse()?).await {
                Ok(()) => Ok(()),
                // Older portals identify host applications through their desktop launcher.
                Err(ashpd::Error::PortalNotFound(_)) => Ok(()),
                Err(error) => Err(anyhow::Error::from(error)
                    .context("Could not identify Picora to the desktop portal")),
            }
        })
        .await?;
    Ok(())
}

enum Command {
    Copy {
        asset: Asset,
        original: bool,
        result: oneshot::Sender<Result<()>>,
    },
    Paste(crate::platform::paste::Ticket, oneshot::Sender<Result<()>>),
}
#[derive(Clone)]
pub struct Wayland {
    tx: mpsc::Sender<Command>,
}
impl Wayland {
    /// # Errors
    /// Returns an error if the portal cannot create or start an authorized remote-desktop session.
    pub async fn connect(restore_token: Option<&str>) -> Result<(Self, Option<String>)> {
        register_application().await?;
        let remote = RemoteDesktop::new()
            .await
            .with_context(|| t!("Desktop does not offer the Remote Desktop portal").into_owned())?;
        let session = remote.create_session().await?;
        let setup = async {
            let closed = session
                .receive_closed()
                .await
                .context("Cannot monitor Wayland session closure")?;
            remote
                .select_devices(
                    &session,
                    DeviceType::Keyboard.into(),
                    restore_token,
                    PersistMode::ExplicitlyRevoked,
                )
                .await?
                .response()?;
            let clipboard = async {
                let clipboard = Clipboard::new()
                    .await
                    .context("Cannot initialize clipboard portal")?;
                clipboard
                    .request(&session)
                    .await
                    .with_context(|| t!("Clipboard portal access was not granted").into_owned())?;
                Ok::<_, anyhow::Error>(clipboard)
            }
            .await;
            let selected = remote.start(&session, None).await?.response()?;
            ensure!(
                selected.devices().contains(DeviceType::Keyboard),
                "Keyboard permission was not granted"
            );
            Ok::<_, anyhow::Error>((clipboard, selected, closed))
        }
        .await;
        let (clipboard, selected, closed) = match setup {
            Ok(result) => result,
            Err(e) => {
                let _ = session.close().await;
                return Err(e);
            }
        };
        let restore_token = selected.restore_token().map(str::to_owned);
        let (tx, mut rx) = mpsc::channel::<Command>(8);
        tokio::spawn(async move {
            let mut contents: Option<(String, Vec<u8>)> = None;
            let (mut transfers, transfer_error) = match &clipboard {
                Ok(c) => match c.receive_selection_transfer().await {
                    Ok(stream) => (Some(Box::pin(stream)), None),
                    Err(error) => (
                        None,
                        Some(format!("Clipboard transfer listener unavailable: {error}")),
                    ),
                },
                Err(error) => (None, Some(format!("{error:#}"))),
            };
            tokio::pin!(closed);
            loop {
                tokio::select! {
                    _=closed.next()=>break,
                    cmd=rx.recv()=>match cmd {
                        Some(Command::Copy{asset,original,result})=>{
                            let response=async {
                                ensure_clipboard_ready(transfer_error.as_deref())?;
                                let c=clipboard.as_ref().map_err(|error|anyhow::anyhow!("{error:#}"))?;
                                let (mime,data)=if original {let url=url::Url::from_file_path(&asset.original).map_err(|()|anyhow::anyhow!("Invalid sticker file path"))?;("text/uri-list",format!("{url}\r\n").into_bytes())}else{("image/png",tokio::fs::read(&asset.still).await?)};
                                c.set_selection(&session,&[mime]).await?;contents=Some((mime.into(),data));Ok(())
                            }.await;let _=result.send(response);
                        }
                        Some(Command::Paste(ticket, result))=>{
                            // Linux evdev keycodes: left Ctrl 29, V 47. Always release Ctrl.
                            let response=async {anyhow::ensure!(ticket.valid(), "Paste cancelled");remote.notify_keyboard_keycode(&session,29,KeyState::Pressed).await?;
                                let v=remote.notify_keyboard_keycode(&session,47,KeyState::Pressed).await;
                                let release_v=remote.notify_keyboard_keycode(&session,47,KeyState::Released).await;
                                let release_ctrl=remote.notify_keyboard_keycode(&session,29,KeyState::Released).await;
                                v?;release_v?;release_ctrl?;Ok(())}.await;
                            let _=result.send(response);
                        }
                        None=>break,
                    },
                    transfer=async {match transfers.as_mut(){Some(s)=>s.next().await,None=>std::future::pending().await}}=>{
                        if let (Some((_,mime,serial)),Ok(c))=(transfer,&clipboard) {
                            let response=async {let (_,data)=contents.as_ref().filter(|(m,_)|*m==mime).context("Clipboard format not offered")?;
                                let fd=c.selection_write(&session,serial).await?;
                                let fd:std::os::fd::OwnedFd=fd.into();let file:std::fs::File=fd.into();let mut file=tokio::fs::File::from_std(file);file.write_all(data).await?;file.flush().await?;Ok::<_,anyhow::Error>(())}.await;
                            let delivered = response.is_ok();
                            if let Err(error) = response {
                                eprintln!("Picora: Wayland clipboard transfer {serial} ({mime}) failed: {error:#}");
                            }
                            if let Err(error) = c.selection_write_done(&session, serial, delivered).await {
                                eprintln!("Picora: Cannot acknowledge Wayland clipboard transfer {serial} ({mime}): {error:#}");
                            }
                        }else{break;}
                    }
                }
            }
            let _ = session.close().await;
        });
        Ok((Self { tx }, restore_token))
    }
    /// # Errors
    /// Returns an error if clipboard transfers are unavailable, the asset cannot be read, or the portal worker cannot claim selection.
    pub async fn copy(&self, asset: Asset, original: bool) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Copy {
                asset,
                original,
                result: tx,
            })
            .await
            .with_context(|| t!("Wayland session closed; reconnect in Settings").into_owned())?;
        rx.await?
    }
    /// # Errors
    /// Returns an error if the worker has stopped, the ticket was cancelled, or the portal rejects keyboard input.
    pub async fn paste(&self, ticket: crate::platform::paste::Ticket) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Paste(ticket, tx))
            .await
            .with_context(|| t!("Wayland session closed; reconnect in Settings").into_owned())?;
        rx.await?
    }
}
/// # Errors
/// Returns an error if the portal cannot create, bind, or monitor the global-shortcut session.
pub async fn shortcuts(
    trigger: String,
    ready: impl FnOnce() + Send + 'static,
    activate: impl Fn() + Send + 'static,
) -> Result<()> {
    register_application().await?;
    let proxy = GlobalShortcuts::new().await.with_context(|| {
        t!("No shortcut portal; bind `picora --show` in desktop settings").into_owned()
    })?;
    let session = proxy.create_session().await?;
    let mut events = proxy.receive_activated().await?;
    let shortcut = NewShortcut::new("show-picker", t!("Open Picora sticker picker").as_ref())
        .preferred_trigger(trigger.as_str());
    let bound = proxy
        .bind_shortcuts(&session, &[shortcut], None)
        .await?
        .response()?;
    ensure!(
        !bound.shortcuts().is_empty(),
        "No shortcut was granted; use picora --show"
    );
    ready();
    while let Some(event) = events.next().await {
        if event.shortcut_id() == "show-picker" {
            activate();
        }
    }
    Ok(())
}

fn ensure_clipboard_ready(error: Option<&str>) -> Result<()> {
    match error {
        Some(error) => anyhow::bail!("{error}; trying the desktop clipboard"),
        None => Ok(()),
    }
}
