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
use tokio::{
    io::AsyncWriteExt,
    sync::{mpsc, oneshot},
};

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
    pub async fn connect(restore_token: Option<&str>) -> Result<(Self, Option<String>)> {
        let remote = RemoteDesktop::new()
            .await
            .context("Desktop does not offer the Remote Desktop portal")?;
        let session = remote.create_session().await?;
        let setup = async {
            remote
                .select_devices(
                    &session,
                    DeviceType::Keyboard.into(),
                    restore_token,
                    PersistMode::ExplicitlyRevoked,
                )
                .await?
                .response()?;
            let clipboard = Clipboard::new().await.ok();
            let clipboard = if let Some(c) = clipboard {
                if c.request(&session).await.is_ok() {
                    Some(c)
                } else {
                    None
                }
            } else {
                None
            };
            let selected = remote.start(&session, None).await?.response()?;
            ensure!(
                selected.devices().contains(DeviceType::Keyboard),
                "Keyboard permission was not granted"
            );
            Ok::<_, anyhow::Error>((clipboard, selected))
        }
        .await;
        let (clipboard, selected) = match setup {
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
            let mut transfers = if let Some(c) = &clipboard {
                c.receive_selection_transfer().await.ok().map(Box::pin)
            } else {
                None
            };
            let Ok(closed) = session.receive_closed().await else {
                let _ = session.close().await;
                return;
            };
            tokio::pin!(closed);
            loop {
                tokio::select! {
                    _=closed.next()=>break,
                    cmd=rx.recv()=>match cmd {
                        Some(Command::Copy{asset,original,result})=>{
                            let response=async {
                                let c=clipboard.as_ref().context("Clipboard portal unavailable; trying the desktop clipboard")?;
                                let (mime,data)=if original {let url=url::Url::from_file_path(&asset.original).map_err(|_|anyhow::anyhow!("Invalid sticker file path"))?;("text/uri-list",format!("{url}\r\n").into_bytes())}else{("image/png",tokio::fs::read(&asset.still).await?)};
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
                        if let (Some((_,mime,serial)),Some(c))=(transfer,&clipboard) {
                            let response=async {let (_,data)=contents.as_ref().filter(|(m,_)|*m==mime).context("Clipboard format not offered")?;
                                let fd=c.selection_write(&session,serial).await?;
                                let fd:std::os::fd::OwnedFd=fd.into();let file:std::fs::File=fd.into();let mut file=tokio::fs::File::from_std(file);file.write_all(data).await?;file.flush().await?;Ok::<_,anyhow::Error>(())}.await;
                            let _=c.selection_write_done(&session,serial,response.is_ok()).await;
                        }else{break;}
                    }
                }
            }
            let _ = session.close().await;
        });
        Ok((Self { tx }, restore_token))
    }
    pub async fn copy(&self, asset: Asset, original: bool) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Copy {
                asset,
                original,
                result: tx,
            })
            .await
            .context("Wayland session closed; reconnect in Settings")?;
        rx.await?
    }
    pub async fn paste(&self, ticket: crate::platform::paste::Ticket) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(Command::Paste(ticket, tx))
            .await
            .context("Wayland session closed; reconnect in Settings")?;
        rx.await?
    }
}
pub async fn shortcuts(trigger: String, activate: impl Fn() + Send + 'static) -> Result<()> {
    let proxy = GlobalShortcuts::new()
        .await
        .context("No shortcut portal; bind `picora --show` in desktop settings")?;
    let session = proxy.create_session().await?;
    let mut events = proxy.receive_activated().await?;
    let shortcut = NewShortcut::new("show-picker", "Open Picora sticker picker")
        .preferred_trigger(trigger.as_str());
    let bound = proxy
        .bind_shortcuts(&session, &[shortcut], None)
        .await?
        .response()?;
    ensure!(
        !bound.shortcuts().is_empty(),
        "No shortcut was granted; use picora --show"
    );
    while let Some(event) = events.next().await {
        if event.shortcut_id() == "show-picker" {
            activate();
        }
    }
    Ok(())
}
