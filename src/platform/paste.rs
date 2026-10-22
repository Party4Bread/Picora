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
