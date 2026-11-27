//! A small limit for [`super::WHOLE_FILES`], so an admission test is sized in
//! kilobytes. Admission runs on worker threads, so the limit is one per
//! process: a test holds [`Limit`], which lets one such test run at a time
//! and restores the default when it drops, also when an assertion fails
//! first. A lowered limit only clamps what other tests admit at once; it
//! never refuses their work, and the structural parse budget does not read it
//! ([`crate::structural::parse_budget`]).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

static LIMIT: AtomicUsize = AtomicUsize::new(0);
static TURN: Mutex<()> = Mutex::new(());

pub(crate) fn get() -> Option<usize> {
    Some(LIMIT.load(Ordering::Relaxed)).filter(|limit| *limit != 0)
}

/// A limit a test holds: the only writer of the limit while it lives; the
/// default returns when it drops.
#[must_use = "the limit lasts only while the guard lives"]
pub(crate) struct Limit {
    limit: &'static AtomicUsize,
    _turn: MutexGuard<'static, ()>,
}

impl Limit {
    /// Admit at most `bytes` at once from now on.
    pub(crate) fn set(&self, bytes: usize) {
        self.limit.store(bytes, Ordering::Relaxed);
    }
}

impl Drop for Limit {
    fn drop(&mut self) {
        self.limit.store(0, Ordering::Relaxed);
    }
}

/// Admit at most `bytes` at once until the returned guard drops.
pub(crate) fn hold(bytes: usize) -> Limit {
    let turn = TURN.lock().unwrap_or_else(PoisonError::into_inner);
    let limit = Limit {
        limit: &LIMIT,
        _turn: turn,
    };
    limit.set(bytes);
    limit
}
