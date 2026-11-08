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
    /// The signal that raises this cancellation, for work that awaits it
    /// instead of re-running [`Self::check`]; `None` when only `check` says.
    fn signal(&self) -> Option<&octocode_io::cancellation::Cancellation> {
        None
    }
}

/// The instant one wait under `cancel` must end: its request's deadline,
/// else the longest single wait from now (a request without a caller budget
/// has no deadline; a cancel ends its waits).
pub(crate) fn deadline_of(cancel: &dyn CancellationCheck) -> std::time::Instant {
    cancel
        .deadline()
        .unwrap_or_else(|| std::time::Instant::now() + super::lifecycle::MAX_TIMEOUT)
}
/// The midpoint of what `cancel`'s request has left: where work that can
/// resume ends, leaving the other half to answer; `None` for work no request
/// bounds.
pub(crate) fn halfway(cancel: &dyn CancellationCheck) -> Option<std::time::Instant> {
    let now = std::time::Instant::now();
    cancel
        .deadline()
        .map(|deadline| now + deadline.saturating_duration_since(now) / 2)
}

#[cfg(test)]
pub(crate) struct NeverCancel;
#[cfg(test)]
impl CancellationCheck for NeverCancel {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
}
