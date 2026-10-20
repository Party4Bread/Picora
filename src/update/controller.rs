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
    pub fn busy(&self) -> bool {
        matches!(
            self.state,
            State::Checking | State::Downloading(..) | State::HandingOff
        )
    }
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
    #[test]
    fn cancelled_and_stale_events_cannot_start_handoff() {
        let mut c = Controller::default();
        c.state = State::Downloading(1, 5);
        let id = c.generation;
        c.cancel();
        c.tx.send((id, Event::Checked(Ok(None)))).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(!c.poll(rt.handle(), &egui::Context::default()));
        assert!(matches!(c.state, State::Idle));
    }
}
