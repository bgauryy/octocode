//! One request's observation of the file system
//! ([`octocode_engine::stamp::observing`]): the proofs of the scope's kept
//! fingerprint, listing, dependents and test-code verdicts, and the spelling
//! lookups of its text scan, read each path once per request. The first
//! check stats it; every later one, in any of them, reads that stat.
//!
//! The observation lives as long as the request's task ([`with_observation`]);
//! work the request hands to a blocking worker reads through it
//! ([`carried`]), and so does each thread a proof check or a scan fans out to.
use octocode_engine::stamp::Observed;
use std::future::Future;
use std::sync::Arc;

tokio::task_local! {
    static OBSERVED: Arc<Observed>;
}

/// Run one request under an observation of its own (a measured test's,
/// under its probe).
pub(super) async fn with_observation<F: Future>(request: F) -> F::Output {
    #[cfg(test)]
    let observed = super::scope::probe::current()
        .map(|work| Arc::clone(&work.observed))
        .unwrap_or_default();
    #[cfg(not(test))]
    let observed = Arc::default();
    OBSERVED.scope(observed, request).await
}

/// The observation of the request this task belongs to.
fn current() -> Option<Arc<Observed>> {
    OBSERVED.try_with(Arc::clone).ok()
}

/// `work`, to run on another thread under this request's observation.
pub(super) fn carried<T>(work: impl FnOnce() -> T) -> impl FnOnce() -> T {
    let observed = current();
    move || octocode_engine::stamp::observing(observed, work)
}

/// Run synchronous `work` on this task's thread under the request's
/// observation (on a worker, under the one it carries).
pub(super) fn within<T>(work: impl FnOnce() -> T) -> T {
    match current() {
        Some(observed) => octocode_engine::stamp::observing(Some(observed), work),
        None => work(),
    }
}
