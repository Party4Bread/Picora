use crate::{
    media::{Asset, Decoded, MediaCache},
    model::{LibrarySticker, Pack, PackRef, Provider},
    platform::{
        clipboard::ClipboardService,
        focus::{self, Target},
    },
    providers::{Repository, StickerProvider},
};
#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::Result;
use eframe::egui;
use std::sync::{Arc, Mutex, mpsc};
use tokio::sync::Semaphore;

pub enum Event {
    UpdateHealthFailed(String),
    Catalog(u64, Result<Vec<Pack>>),
    Preview(u64, Result<Pack>),
    Installed(Result<Pack>),
    Media(
        String,
        Result<(Asset, Decoded)>,
        tokio::sync::OwnedSemaphorePermit,
    ),
    Progress(String),
    Copied(Result<LibrarySticker>),
    Pasted(Result<()>),
    Login(Result<String>),
    Show,
    #[cfg(target_os = "linux")]
    Wayland(Result<crate::platform::wayland::Wayland>),
}
#[derive(Clone)]
pub struct Bus {
    pub tx: mpsc::Sender<Event>,
    pub ctx: egui::Context,
}
impl Bus {
    pub fn send(&self, event: Event) {
        let _ = self.tx.send(event);
        self.ctx.request_repaint();
    }
}
#[derive(Clone)]
pub struct Services {
    pub runtime: tokio::runtime::Handle,
    pub cache: MediaCache,
    pub bus: Bus,
    repos: Arc<Mutex<[Repository; 2]>>,
    semaphore: Arc<Semaphore>,
    decode_slots: Arc<Semaphore>,
    clipboard: ClipboardService,
    cancellation: crate::platform::paste::Cancellation,
    #[cfg(target_os = "linux")]
    pub wayland: Arc<Mutex<Option<crate::platform::wayland::Wayland>>>,
}
impl Services {
    pub fn new(runtime: tokio::runtime::Handle, cache: MediaCache, bus: Bus) -> Result<Self> {
        let arca = Repository::new(Provider::Arca)?;
        if let Ok(c) = crate::auth::load() {
            arca.set_cookies(&c);
        }
        Ok(Self {
            runtime,
            cache,
            bus,
            repos: Arc::new(Mutex::new([Repository::new(Provider::Dcinside)?, arca])),
            semaphore: Arc::new(Semaphore::new(4)),
            decode_slots: Arc::new(Semaphore::new(2)),
            clipboard: ClipboardService::default(),
            cancellation: Default::default(),
            #[cfg(target_os = "linux")]
            wayland: Arc::new(Mutex::new(None)),
        })
    }
    pub fn repo(&self, p: Provider) -> Repository {
        self.repos.lock().unwrap()[if p == Provider::Dcinside { 0 } else { 1 }].clone()
    }
    pub fn browse(&self, p: Provider, query: String, page: u32, id: u64) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = s.repo(p).browse(&query, page).await;
            s.bus.send(Event::Catalog(id, result));
        });
    }
    pub fn preview(&self, r: PackRef, id: u64) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = s.repo(r.provider).pack(&r).await;
            s.bus.send(Event::Preview(id, result));
        });
    }
    pub async fn asset(&self, p: Provider, url: &str) -> Result<Asset> {
        let _permit = self.semaphore.acquire().await?;
        let cache = self.cache.clone();
        let key = url.to_owned();
        if let Some(asset) = tokio::task::spawn_blocking(move || cache.cached(&key)).await?? {
            return Ok(asset);
        }
        let bytes = self.repo(p).bytes(url).await?;
        let cache = self.cache.clone();
        let url = url.to_owned();
        tokio::task::spawn_blocking(move || cache.save(&url, &bytes)).await?
    }
    pub fn media(&self, p: Provider, url: String) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let Ok(permit) = s.decode_slots.clone().acquire_owned().await else {
                return;
            };
            let result = async {
                let asset = s.asset(p, &url).await?;
                let a = asset.clone();
                let decoded = tokio::task::spawn_blocking(move || {
                    crate::media::preview(&std::fs::read(a.original)?)
                })
                .await??;
                Ok((asset, decoded))
            }
            .await;
            s.bus.send(Event::Media(url, result, permit));
        });
    }
    pub fn install(&self, pack: Pack) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = async {
                for (i, sticker) in pack.stickers.iter().enumerate() {
                    s.bus.send(Event::Progress(format!(
                        "Downloading {} · {}/{}",
                        pack.title,
                        i + 1,
                        pack.stickers.len()
                    )));
                    s.asset(pack.reference.provider, &sticker.url).await?;
                }
                Ok(pack)
            }
            .await;
            s.bus.send(Event::Installed(result));
        });
    }
    pub fn copy(&self, item: LibrarySticker, mode: crate::media::CopyMode) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = async {
                let asset = s.asset(item.provider, &item.sticker.url).await?;
                let cache = s.cache.clone();
                let (asset, original) =
                    tokio::task::spawn_blocking(move || cache.prepare_copy(asset, mode)).await??;
                #[cfg(target_os = "linux")]
                {
                    let wayland = s.wayland.lock().unwrap().clone();
                    if let Some(w) = wayland
                        && w.copy(asset.clone(), original).await.is_ok()
                    {
                        return Ok(item);
                    }
                }
                let clipboard = s.clipboard.clone();
                tokio::task::spawn_blocking(move || clipboard.copy(&asset, original)).await??;
                Ok(item)
            }
            .await;
            s.bus.send(Event::Copied(result));
        });
    }
    pub fn paste_ticket(&self) -> crate::platform::paste::Ticket {
        self.cancellation.ticket()
    }
    pub fn cancel_paste(&self) {
        self.cancellation.cancel();
    }
    pub fn paste(&self, target: Target, picker: Option<Target>) {
        let ticket = self.cancellation.ticket();
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = async {
                if crate::platform::is_wayland() {
                    #[cfg(target_os = "linux")]
                    {
                        let w = s
                            .wayland
                            .lock()
                            .unwrap()
                            .clone()
                            .context("Enable Wayland keyboard access in Settings")?;
                        anyhow::ensure!(ticket.valid(), "Paste cancelled");
                        return w.paste(ticket).await;
                    }
                }
                tokio::task::spawn_blocking(move || focus::paste(target, picker, ticket)).await?
            }
            .await;
            s.bus.send(Event::Pasted(result));
        });
    }
    pub fn login(&self) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = async {
                let cookies = crate::auth::login(crate::auth::load().unwrap_or_default()).await?;
                let repo = Repository::new(Provider::Arca)?;
                repo.set_cookies(&cookies);
                // Keep the usable session even if a catalog parser needs updating.
                s.repos.lock().unwrap()[1] = repo.clone();
                let persisted =
                    tokio::task::spawn_blocking(move || crate::auth::save(&cookies)).await?;
                let storage = match persisted {
                    Ok(()) => "Session saved securely for your OS account.".to_owned(),
                    Err(e) => format!("Session is memory-only: could not save securely ({e:#})."),
                };
                match repo.browse("", 1).await {
                    Ok(_) => Ok(format!("Arca connected. {storage}")),
                    Err(e) => Ok(format!("Session received. {storage} Catalog check: {e}")),
                }
            }
            .await;
            s.bus.send(Event::Login(result));
        });
    }
    pub fn logout(&self) -> Result<()> {
        self.repos.lock().unwrap()[1] = Repository::new(Provider::Arca)?;
        crate::auth::forget()
    }
    #[cfg(target_os = "linux")]
    pub fn enable_wayland(&self) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = crate::platform::wayland::Wayland::connect().await;
            s.bus.send(Event::Wayland(result));
        });
    }
}
