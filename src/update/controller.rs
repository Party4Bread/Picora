use super::{
    archive::{StagedUpdate, stage_archive},
    helper::{ReadyHelper, launch_helper},
    release::{AvailableUpdate, ReleaseClient, Target},
    transaction::InstallLayout,
};
use anyhow::{Context, Result};
use eframe::egui;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

pub enum State {
    Idle,
    Checking,
    Current,
    Available(AvailableUpdate),
    Downloading(u64, u64),
    Staged(AvailableUpdate, tempfile::TempDir, StagedUpdate),
    HandingOff,
    Error(String),
}
enum Event {
    Checked(Result<Option<AvailableUpdate>>),
    Progress(u64, u64),
    Staged(Result<(AvailableUpdate, tempfile::TempDir, StagedUpdate)>),
    Ready(Result<ReadyHelper>),
}
/// Update commands start background work; call [`Self::poll`] on UI updates to
/// receive completion through [`Self::state`], including [`State::Error`]. Polling
/// automatically hands a staged archive to the updater helper. Once handoff starts,
/// cancellation is disabled so the installation cannot be interrupted halfway.
pub struct Controller {
    pub state: State,
    pub previous_failure: Option<String>,
    generation: u64,
    tx: mpsc::Sender<(u64, Event)>,
    rx: mpsc::Receiver<(u64, Event)>,
    task: Option<tokio::task::JoinHandle<()>>,
    helper: Option<ReadyHelper>,
}
impl Default for Controller {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            state: State::Idle,
            previous_failure: super::helper::last_failure(),
            generation: 0,
            tx,
            rx,
            task: None,
            helper: None,
        }
    }
}
impl Controller {
    #[must_use]
    pub fn busy(&self) -> bool {
        matches!(
            self.state,
            State::Checking | State::Downloading(..) | State::HandingOff
        )
    }
    /// Starts a check unless another operation is busy; results arrive through `poll`.
    pub fn check(&mut self, runtime: &tokio::runtime::Handle, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        self.generation += 1;
        let id = self.generation;
        self.state = State::Checking;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.task = Some(runtime.spawn(async move {
            let result = async {
                ReleaseClient::new()?
                    .check(
                        env!("CARGO_PKG_VERSION"),
                        Target::current().context("No update build for this platform")?,
                    )
                    .await
            }
            .await;
            let _ = tx.send((id, Event::Checked(result)));
            ctx.request_repaint();
        }));
    }
    /// Starts a download only from `Available`; all other states ignore the request.
    pub fn download(&mut self, runtime: &tokio::runtime::Handle, ctx: &egui::Context) {
        let State::Available(update) = &self.state else {
            return;
        };
        let update = update.clone();
        self.generation += 1;
        let id = self.generation;
        self.state = State::Downloading(0, update.asset.size);
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.task = Some(runtime.spawn(async move {
            let result = async {
                let dir = tempfile::Builder::new()
                    .prefix("picora-update-")
                    .tempdir()?;
                let archive = dir.path().join("download");
                let mut last = Instant::now();
                ReleaseClient::new()?
                    .download(&update, &archive, |done, total| {
                        if last.elapsed() > Duration::from_millis(100) || done == total {
                            let _ = tx.send((id, Event::Progress(done, total)));
                            ctx.request_repaint();
                            last = Instant::now();
                        }
                    })
                    .await?;
                tokio::task::spawn_blocking(move || {
                    let staged =
                        stage_archive(&archive, update.asset.target, &dir.path().join("staged"))?;
                    Ok((update, dir, staged))
                })
                .await?
            }
            .await;
            let _ = tx.send((id, Event::Staged(result)));
            ctx.request_repaint();
        }));
    }
    /// Aborts work and invalidates queued events, returning to `Idle`. Ignored during handoff.
    pub fn cancel(&mut self) {
        if matches!(self.state, State::HandingOff) {
            return;
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
        self.generation += 1;
        self.state = State::Idle;
    }
    fn handoff(&mut self, runtime: &tokio::runtime::Handle, ctx: &egui::Context) {
        let State::Staged(_, dir, staged) = std::mem::replace(&mut self.state, State::HandingOff)
        else {
            return;
        };
        let id = self.generation;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.task = Some(runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let layout = InstallLayout::detect(&std::env::current_exe()?)?;
                let helper = launch_helper(layout, staged, dir.path())?;
                let _ = dir.keep();
                Ok(helper)
            })
            .await
            .unwrap_or_else(|e| Err(e.into()));
            let _ = tx.send((id, Event::Ready(result)));
            ctx.request_repaint();
        }));
    }
    /// Returns true only after a helper is ready for this process to exit.
    /// Applies current-generation events and starts staged handoff. Returns true when
    /// the helper is ready and the caller must exit to release the running installation.
    pub fn poll(&mut self, runtime: &tokio::runtime::Handle, ctx: &egui::Context) -> bool {
        while let Ok((id, event)) = self.rx.try_recv() {
            if id != self.generation {
                continue;
            }
            self.state = match event {
                Event::Checked(Ok(Some(u))) => State::Available(u),
                Event::Checked(Ok(None)) => State::Current,
                Event::Checked(Err(e)) | Event::Staged(Err(e)) | Event::Ready(Err(e)) => {
                    State::Error(format!("{e:#}"))
                }
                Event::Progress(done, total) => State::Downloading(done, total),
                Event::Staged(Ok((update, dir, staged))) => State::Staged(update, dir, staged),
                Event::Ready(Ok(helper)) => {
                    self.helper = Some(helper);
                    return true;
                }
            };
        }
        if matches!(self.state, State::Staged(..)) {
            self.handoff(runtime, ctx);
        }
        false
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn update() -> AvailableUpdate {
        AvailableUpdate {
            version: "99.0.0".into(),
            notes: String::new(),
            asset: super::super::release::ReleaseAsset {
                target: Target::current().unwrap(),
                name: "offline-test.zip".into(),
                size: 100,
                sha256: "0".repeat(64),
            },
            url: "https://invalid.example/offline-test.zip".into(),
        }
    }

    fn staged() -> Event {
        let dir = tempfile::tempdir().unwrap();
        let staged = StagedUpdate {
            root: dir.path().join("staged"),
            files: vec!["picora".into()],
            target: Target::current().unwrap(),
        };
        Event::Staged(Ok((update(), dir, staged)))
    }

    #[test]
    fn commands_and_completions_follow_update_lifecycle() {
        // Spawned network/helper tasks cannot execute unless this runtime is driven.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let ctx = egui::Context::default();
        let mut c = Controller::default();
        c.download(rt.handle(), &ctx);
        assert!(matches!(c.state, State::Idle));
        assert!(c.task.is_none());
        c.check(rt.handle(), &ctx);
        let id = c.generation;
        assert!(c.busy());
        c.check(rt.handle(), &ctx);
        c.download(rt.handle(), &ctx);
        assert_eq!(c.generation, id);
        c.tx.send((id, Event::Checked(Ok(None)))).unwrap();
        assert!(!c.poll(rt.handle(), &ctx));
        assert!(matches!(c.state, State::Current));
        c.check(rt.handle(), &ctx);
        c.tx.send((c.generation, Event::Checked(Ok(Some(update())))))
            .unwrap();
        assert!(!c.poll(rt.handle(), &ctx));
        assert!(matches!(c.state, State::Available(_)));
        c.download(rt.handle(), &ctx);
        assert!(matches!(c.state, State::Downloading(0, 100)));
        c.tx.send((c.generation, Event::Progress(50, 100))).unwrap();
        assert!(!c.poll(rt.handle(), &ctx));
        assert!(matches!(c.state, State::Downloading(50, 100)));
        c.tx.send((c.generation, staged())).unwrap();
        assert!(!c.poll(rt.handle(), &ctx));
        assert!(matches!(c.state, State::HandingOff));
        let id = c.generation;
        c.cancel();
        c.check(rt.handle(), &ctx);
        c.download(rt.handle(), &ctx);
        assert_eq!(c.generation, id);
        assert!(matches!(c.state, State::HandingOff));
        c.tx.send((id, Event::Ready(Err(anyhow::anyhow!("helper rejected")))))
            .unwrap();
        assert!(!c.poll(rt.handle(), &ctx));
        assert!(matches!(&c.state, State::Error(message) if message == "helper rejected"));
        c.cancel();
        assert!(matches!(c.state, State::Idle));
    }

    #[test]
    fn cancelled_download_ignores_late_progress_and_staging() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let ctx = egui::Context::default();
        let mut c = Controller::default();
        c.state = State::Available(update());
        c.download(rt.handle(), &ctx);
        let id = c.generation;
        c.cancel();
        c.tx.send((id, Event::Progress(100, 100))).unwrap();
        c.tx.send((id, staged())).unwrap();
        assert!(!c.poll(rt.handle(), &ctx));
        assert!(matches!(c.state, State::Idle));
        assert!(c.task.is_none());
    }
}
