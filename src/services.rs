//! Commands submit background work and report completion through `Bus`; callers
//! must keep draining events even after hiding the picker. Errors are carried in
//! each completion's `Result`, not returned by the submission method.
//!
//! | Command | Completion | Correlation |
//! | --- | --- | --- |
//! | `browse`, `preview` | `Catalog`, `Preview` | caller-supplied request ID |
//! | `request_media_preview` | `Media` | asset URL and animation request |
//! | `download_pack` | `PackDownloaded` | returned pack; `Progress` is informational |
//! | `copy`, `paste` | `Copied`, `Pasted` | one in-flight UI copy/paste operation |
//! | `login`, `check_arca` | `Login` | shared account status updated before completion |
//! | `enable_wayland` | `Wayland` | session installed before notification; no event if no saved grant |
//!
//! `load_asset` is awaited work: its caller receives the asset or error directly.
//! Construction and `logout` perform synchronous credential-store I/O; `logout`
//! resets local account state even if deleting the persisted credentials fails.
//! On macOS, `NativePaste` transfers keyboard execution to the UI thread before
//! the final `Pasted` event. `cancel_paste` invalidates tickets without an event.

pub mod account;
use crate::{
    media::{Asset, Decoded, MediaCache},
    model::{LibrarySticker, Pack, PackRef, Provider},
    platform::{
        clipboard::ClipboardService,
        focus::{self, Target},
    },
    providers::Repository,
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
    PackDownloaded(Result<Pack>),
    Media(
        String,
        Result<(Asset, Decoded)>,
        bool,
        tokio::sync::OwnedSemaphorePermit,
    ),
    Progress(String),
    Copied(Result<LibrarySticker>),
    Pasted(Result<()>),
    #[cfg(target_os = "macos")]
    NativePaste(Target, crate::platform::paste::Ticket),
    Login(Result<String>),
    #[cfg(target_os = "linux")]
    Show,
    #[cfg(target_os = "linux")]
    ShortcutRegistered(u64, String, bool, Result<()>),
    #[cfg(target_os = "linux")]
    Wayland(Result<Option<String>>),
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
    pub account_status: Arc<Mutex<account::AccountStatus>>,
    cancellation: crate::platform::paste::Cancellation,
    #[cfg(target_os = "linux")]
    wayland: Arc<Mutex<Option<crate::platform::wayland::Wayland>>>,
    #[cfg(target_os = "linux")]
    wayland_connecting: Arc<tokio::sync::Mutex<()>>,
}
impl Services {
    pub fn new(runtime: tokio::runtime::Handle, cache: MediaCache, bus: Bus) -> Result<Self> {
        Self::with_session(runtime, cache, bus, crate::auth::load())
    }
    pub(crate) fn with_session(
        runtime: tokio::runtime::Handle,
        cache: MediaCache,
        bus: Bus,
        session: Result<Vec<String>>,
    ) -> Result<Self> {
        let arca = Repository::new(Provider::Arca)?;
        let mut account_status = account::AccountStatus::default();
        match session {
            Ok(c) if !c.is_empty() => {
                arca.set_cookies(&c);
                account_status.has_session = true;
                account_status.saved = true;
            }
            Ok(_) => (),
            Err(e)
                if matches!(
                    e.downcast_ref::<keyring::Error>(),
                    Some(keyring::Error::NoEntry)
                ) => {}
            Err(e) => {
                account_status.check =
                    account::CheckState::Failed(format!("Could not load the saved session: {e:#}"));
            }
        }
        Ok(Self {
            runtime,
            cache,
            bus,
            repos: Arc::new(Mutex::new([Repository::new(Provider::Dcinside)?, arca])),
            semaphore: Arc::new(Semaphore::new(4)),
            decode_slots: Arc::new(Semaphore::new(2)),
            clipboard: ClipboardService::default(),
            account_status: Arc::new(Mutex::new(account_status)),
            cancellation: crate::platform::paste::Cancellation::default(),
            #[cfg(target_os = "linux")]
            wayland: Arc::new(Mutex::new(None)),
            #[cfg(target_os = "linux")]
            wayland_connecting: Arc::default(),
        })
    }
    pub fn repo(&self, p: Provider) -> Result<Repository> {
        let repos = self
            .repos
            .lock()
            .map_err(|_| anyhow::anyhow!("Repository state unavailable; restart Picora"))?;
        Ok(repos[usize::from(p != Provider::Dcinside)].clone())
    }
    pub fn account_status(&self) -> Result<std::sync::MutexGuard<'_, account::AccountStatus>> {
        self.account_status
            .lock()
            .map_err(|_| anyhow::anyhow!("Account state unavailable; restart Picora"))
    }
    pub fn browse(&self, p: Provider, query: String, page: u32, id: u64) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = async { s.repo(p)?.browse(&query, page).await }.await;
            s.bus.send(Event::Catalog(id, result));
        });
    }
    pub fn preview(&self, r: PackRef, id: u64) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = async { s.repo(r.provider)?.pack(&r).await }.await;
            s.bus.send(Event::Preview(id, result));
        });
    }
    pub async fn load_asset(&self, p: Provider, url: &str) -> Result<Asset> {
        let _permit = self.semaphore.acquire().await?;
        let cache = self.cache.clone();
        let key = url.to_owned();
        if let Some(asset) = tokio::task::spawn_blocking(move || cache.cached(&key)).await?? {
            return Ok(asset);
        }
        let bytes = self.repo(p)?.bytes(url).await?;
        let cache = self.cache.clone();
        let url = url.to_owned();
        tokio::task::spawn_blocking(move || cache.save(&url, &bytes)).await?
    }
    pub fn request_media_preview(&self, p: Provider, url: String, animate: bool) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let Ok(permit) = s.decode_slots.clone().acquire_owned().await else {
                return;
            };
            let result = async {
                let asset = s.load_asset(p, &url).await?;
                let a = asset.clone();
                let decoded =
                    tokio::task::spawn_blocking(move || crate::media::preview_asset(&a, animate))
                        .await??;
                Ok((asset, decoded))
            }
            .await;
            s.bus.send(Event::Media(url, result, animate, permit));
        });
    }
    pub fn download_pack(&self, pack: Pack) {
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
                    s.load_asset(pack.reference.provider, &sticker.url).await?;
                }
                Ok(pack)
            }
            .await;
            s.bus.send(Event::PackDownloaded(result));
        });
    }
    pub fn copy(&self, item: LibrarySticker, mode: crate::media::CopyMode) {
        let s = self.clone();
        self.runtime.spawn(async move {
            s.copy_with(item, mode, |asset, original| {
                s.copy_to_clipboard(asset, original)
            })
            .await;
        });
    }
    async fn copy_with<F>(
        &self,
        item: LibrarySticker,
        mode: crate::media::CopyMode,
        clipboard: impl FnOnce(Asset, bool) -> F,
    ) where
        F: std::future::Future<Output = Result<()>>,
    {
        let result = async {
            let asset = self.load_asset(item.provider, &item.sticker.url).await?;
            let cache = self.cache.clone();
            let (asset, original) =
                tokio::task::spawn_blocking(move || cache.prepare_copy(asset, mode)).await??;
            clipboard(asset, original).await?;
            Ok(item)
        }
        .await;
        self.bus.send(Event::Copied(result));
    }
    async fn copy_to_clipboard(&self, asset: Asset, original: bool) -> Result<()> {
        #[cfg(target_os = "linux")]
        let portal_error = {
            let wayland = self
                .wayland
                .lock()
                .map_err(|_| anyhow::anyhow!("Wayland session unavailable; restart Picora"))?
                .clone();
            if let Some(w) = wayland {
                match w.copy(asset.clone(), original).await {
                    Ok(()) => return Ok(()),
                    Err(error) => Some(error),
                }
            } else {
                None
            }
        };
        let clipboard = self.clipboard.clone();
        let fallback = tokio::task::spawn_blocking(move || clipboard.copy(&asset, original))
            .await
            .unwrap_or_else(|error| Err(error.into()));
        #[cfg(target_os = "linux")]
        let fallback = match portal_error {
            Some(error) => {
                fallback.with_context(|| format!("Portal clipboard also failed: {error:#}"))
            }
            None => fallback,
        };
        fallback
    }
    pub fn paste_ticket(&self) -> crate::platform::paste::Ticket {
        self.cancellation.ticket()
    }
    pub fn cancel_paste(&self) {
        self.cancellation.cancel();
    }
    pub fn paste(&self, target: Target, picker: Option<Target>) {
        let ticket = self.cancellation.ticket();
        #[cfg(target_os = "macos")]
        {
            let bus = self.bus.clone();
            if !crate::platform::macos_permissions::accessibility_allowed() {
                bus.send(Event::Pasted(Err(
                    crate::platform::macos_permissions::AccessibilityRequired.into(),
                )));
                return;
            }
            // Keep the UI runloop free to process the focus change. Carbon's
            // keyboard-layout APIs used by Enigo must then run on the UI thread.
            self.runtime.spawn_blocking(move || {
                match focus::prepare_paste(target, picker, &ticket) {
                    Ok(()) => bus.send(Event::NativePaste(target, ticket)),
                    Err(error) => bus.send(Event::Pasted(Err(error))),
                }
            });
        }
        #[cfg(not(target_os = "macos"))]
        {
            let s = self.clone();
            self.runtime.spawn(async move {
                let result = async {
                    if crate::platform::is_wayland() {
                        #[cfg(target_os = "linux")]
                        {
                            let w = s
                                .wayland
                                .lock()
                                .map_err(|_| {
                                    anyhow::anyhow!("Wayland session unavailable; restart Picora")
                                })?
                                .clone()
                                .context("Enable Wayland keyboard access in Settings")?;
                            anyhow::ensure!(ticket.valid(), "Paste cancelled");
                            return w.paste(ticket).await;
                        }
                    }
                    tokio::task::spawn_blocking(move || focus::paste(target, picker, ticket))
                        .await?
                }
                .await;
                s.bus.send(Event::Pasted(result));
            });
        }
    }
    pub fn login(&self, reuse_session: bool) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let cookies = if reuse_session {
                crate::auth::load().unwrap_or_default()
            } else {
                Vec::new()
            };
            s.login_with(
                crate::auth::login(cookies),
                |cookies| async move {
                    tokio::task::spawn_blocking(move || crate::auth::save(&cookies)).await?
                },
                |repo| async move { repo.browse("", 1).await.map(|_| ()) },
            )
            .await;
        });
    }
    async fn login_with<P, V>(
        &self,
        sign_in: impl std::future::Future<Output = Result<Vec<String>>>,
        persist: impl FnOnce(Vec<String>) -> P,
        verify: impl FnOnce(Repository) -> V,
    ) where
        P: std::future::Future<Output = Result<()>>,
        V: std::future::Future<Output = Result<()>>,
    {
        let result = async {
            let cookies = sign_in.await?;
            let repo = Repository::new(Provider::Arca)?;
            repo.set_cookies(&cookies);
            let preserve_previous = {
                let mut status = self.account_status()?;
                let previous = status.has_session || status.saved;
                status.check = account::CheckState::Checking;
                previous
            };
            self.bus.ctx.request_repaint();
            let check = verify(repo.clone()).await.map_err(|e| format!("{e:#}"));
            if preserve_previous && let Err(error) = &check {
                anyhow::bail!(
                    "Reconnect could not be verified. Your previous session was kept. {error}"
                );
            }
            // A first sign-in remains usable in memory if saving fails.
            self.repos
                .lock()
                .map_err(|_| anyhow::anyhow!("Repository state unavailable; restart Picora"))?[1] =
                repo.clone();
            let persisted = persist(cookies).await;
            let saved = persisted.is_ok();
            let storage_error = persisted
                .err()
                .map(|e| format!("Could not save the session: {e:#}"));
            *self.account_status()? = account::AccountStatus {
                has_session: true,
                saved,
                check: account::CheckState::Checking,
                storage_error,
            };
            self.bus.ctx.request_repaint();
            self.account_status()?.finish_check(check);
            Ok(self.account_status()?.label().to_owned())
        }
        .await;
        if let Err(e) = &result
            && let Ok(mut status) = self.account_status()
        {
            status.finish_check(Err(format!("{e:#}")));
        }
        self.bus.send(Event::Login(result));
    }
    pub fn check_arca(&self) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let result = async {
                s.account_status()?.check = account::CheckState::Checking;
                let check = async { s.repo(Provider::Arca)?.browse("", 1).await }
                    .await
                    .map(|_| ())
                    .map_err(|e| format!("{e:#}"));
                let mut status = s.account_status()?;
                status.finish_check(check);
                Ok(status.label().to_owned())
            }
            .await;
            s.bus.send(Event::Login(result));
        });
    }
    pub fn logout(&self) -> Result<()> {
        self.logout_with(crate::auth::forget)
    }
    fn logout_with(&self, forget: impl FnOnce() -> Result<()>) -> Result<()> {
        self.repos
            .lock()
            .map_err(|_| anyhow::anyhow!("Repository state unavailable; restart Picora"))?[1] =
            Repository::new(Provider::Arca)?;
        let result = forget();
        let mut status = self.account_status()?;
        *status = account::AccountStatus::default();
        if let Err(e) = &result {
            status.saved = true;
            status.check = account::CheckState::Failed(format!(
                "Disconnected locally, but the saved session could not be removed: {e:#}. Try Sign out again."
            ));
        }
        result
    }
    #[cfg(target_os = "linux")]
    pub fn enable_wayland(&self, automatic: bool) {
        let s = self.clone();
        self.runtime.spawn(async move {
            let Ok(_guard) = s.wayland_connecting.try_lock() else {
                return;
            };
            match crate::platform::wayland::permission::connect(automatic).await {
                Ok(Some((session, warning))) => {
                    let result = s
                        .wayland
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Wayland session unavailable; restart Picora"))
                        .map(|mut slot| {
                            *slot = Some(session);
                            warning
                        });
                    s.bus.send(Event::Wayland(result));
                }
                Ok(None) => (),
                Err(e) => s.bus.send(Event::Wayland(Err(e))),
            }
        });
    }
}

#[cfg(test)]
mod tests;
