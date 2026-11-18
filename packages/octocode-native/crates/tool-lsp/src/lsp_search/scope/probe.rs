//! Test-only counters of one request's whole-scope work: how many walks it
//! makes, how many files they stamp, and how many paths the text scan
//! canonicalizes.
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
pub(super) fn current() -> Option<Arc<Work>> {
    WORK.try_with(Arc::clone).ok()
}
