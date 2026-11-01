//! Cooperative cancellation shared by every tool that walks, parses, or reads.
//! The runtime's execution context implements it; tools poll it between units
//! of work and stop with the returned reason.
pub trait CancellationCheck: Sync {
    fn check(&self) -> Result<(), String>;
    /// When the request this work serves runs out of time; `None` for work
    /// no request bounds (a CLI build its user stops).
    fn deadline(&self) -> Option<std::time::Instant> {
        None
    }
}

/// The instant work under `cancel` must stop: its request's deadline, else
/// the longest any request may run.
pub(crate) fn deadline_of(cancel: &dyn CancellationCheck) -> std::time::Instant {
    cancel
        .deadline()
        .unwrap_or_else(|| std::time::Instant::now() + super::lifecycle::MAX_TIMEOUT)
}
pub(crate) struct NeverCancel;
impl CancellationCheck for NeverCancel {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}
