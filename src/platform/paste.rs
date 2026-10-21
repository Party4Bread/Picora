use anyhow::{Result, ensure};
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Hide,
    Paste,
    Copied,
}
pub struct PasteFlow {
    auto: bool,
    target: bool,
    phase: u8,
}
impl PasteFlow {
    #[must_use]
    pub fn new(auto: bool, target: bool) -> Self {
        Self {
            auto,
            target,
            phase: 0,
        }
    }
    pub fn cancel_auto(&mut self) {
        self.target = false;
    }
    /// # Errors
    /// Rejects completion if this flow has already completed its copy phase.
    pub fn copied(&mut self) -> Result<Action> {
        ensure!(self.phase == 0, "Copy already completed");
        self.phase = 1;
        Ok(Action::Hide)
    }
    /// # Errors
    /// Rejects hiding before clipboard preparation has completed or after this flow has already advanced.
    pub fn hidden(&mut self) -> Result<Action> {
        ensure!(self.phase == 1, "Clipboard must be ready before hiding");
        self.phase = 2;
        Ok(if self.auto && self.target {
            Action::Paste
        } else {
            Action::Copied
        })
    }
}

#[derive(Clone, Default)]
pub struct Cancellation(std::sync::Arc<std::sync::atomic::AtomicU64>);
pub struct Ticket {
    epoch: Cancellation,
    generation: u64,
}
impl Cancellation {
    pub fn cancel(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    #[must_use]
    pub fn ticket(&self) -> Ticket {
        Ticket {
            epoch: self.clone(),
            generation: self.0.load(std::sync::atomic::Ordering::SeqCst),
        }
    }
}
impl Ticket {
    #[must_use]
    pub fn valid(&self) -> bool {
        self.generation == self.epoch.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}
