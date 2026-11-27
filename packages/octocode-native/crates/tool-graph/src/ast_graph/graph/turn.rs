//! How much of a call's budget a checkpointed build's extraction may use.
//!
//! Extraction commits each file as it goes; linking and publication do not,
//! and a call the runtime cuts at its deadline answers nothing. So a build
//! that can resume ends its extraction at the midpoint of what its call has
//! left, leaving the other half to link and publish, and otherwise answers
//! that the committed extraction resumes on the next call. Every call
//! therefore either publishes or commits more files, and a scope too large
//! for one call finishes over several.
use octocode_runtime_base::execution::cancel::CancellationCheck;
use std::time::Instant;

/// The request's own stop, plus the end of extraction's share of its budget.
pub(super) struct ExtractionTurn<'a> {
    cancel: &'a dyn CancellationCheck,
    ends: Option<Instant>,
    /// Files the scan's walk found: the turn bounds parsing, never the walk,
    /// so a paused build always knows how many files are left.
    walked: std::sync::atomic::AtomicUsize,
}

impl<'a> ExtractionTurn<'a> {
    /// Half of what `cancel`'s request has left when extraction can resume
    /// (`resumable`); a build that cannot resume, or a request with no
    /// deadline, extracts until the request itself stops.
    pub(super) fn new(cancel: &'a dyn CancellationCheck, resumable: bool) -> Self {
        let ends = octocode_runtime_base::execution::cancel::halfway(cancel).filter(|_| resumable);
        Self {
            cancel,
            ends,
            walked: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// The request's own stop, which the walk runs under.
    pub(super) fn request(&self) -> &'a dyn CancellationCheck {
        self.cancel
    }

    pub(super) fn record_walk(&self, files: usize) {
        self.walked
            .store(files, std::sync::atomic::Ordering::Relaxed);
    }

    pub(super) fn walked(&self) -> usize {
        self.walked.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether extraction's share ended while the request still runs.
    pub(super) fn ended(&self) -> bool {
        self.cancel.check().is_ok() && self.ends.is_some_and(|ends| Instant::now() >= ends)
    }
}

impl CancellationCheck for ExtractionTurn<'_> {
    fn check(&self) -> Result<(), String> {
        self.cancel.check()?;
        if self.ends.is_some_and(|ends| Instant::now() >= ends) {
            return Err("graph extraction used its share of this call's budget".into());
        }
        Ok(())
    }

    fn deadline(&self) -> Option<Instant> {
        self.ends.or_else(|| self.cancel.deadline())
    }
}

#[cfg(test)]
mod tests;
