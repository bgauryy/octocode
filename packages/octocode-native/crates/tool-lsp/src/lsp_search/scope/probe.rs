//! Test-only counters of one request's whole-scope work: how many walks it
//! makes, how many files they stamp, how many paths the text scan
//! canonicalizes, and what the request stats ([`Work::observed`]).
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Work done under one [`measure`].
#[derive(Debug, Default)]
pub(in super::super) struct Work {
    pub(in super::super) walks: AtomicUsize,
    pub(in super::super) stamped: AtomicUsize,
    pub(in super::super) text_scans: AtomicUsize,
    pub(in super::super) canonicalized: AtomicUsize,
    /// The request's observation of the file system: its stats and the
    /// paths they read.
    pub(in super::super) observed: Arc<octocode_engine::stamp::Observed>,
}

impl Work {
    pub(in super::super) fn get(counter: &AtomicUsize) -> usize {
        counter.load(Ordering::Relaxed)
    }
}

tokio::task_local! {
    static WORK: Arc<Work>;
}

/// Run `request`, counting its scope work into `work`.
pub(in super::super) async fn measure<F: Future>(work: &Arc<Work>, request: F) -> F::Output {
    WORK.scope(work.clone(), request).await
}

/// The counters of the request this task belongs to, if measured.
pub(in super::super) fn current() -> Option<Arc<Work>> {
    WORK.try_with(Arc::clone).ok()
}
