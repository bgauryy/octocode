//! A bound on how many workers open files at once.
//!
//! Opening a file is a path lookup in the kernel, and on common platforms the
//! lookups of one process do not scale with the threads that issue them: past a
//! few concurrent opens each one costs more, so the same files take more
//! kernel time and no less wall time. The search itself (the regex over what
//! was read) does scale. A worker takes a permit for the open only, so every
//! core searches while a few open.

use std::sync::{Condvar, Mutex, PoisonError};

/// The permits to open a file.
pub(super) struct OpenGate {
    free: Mutex<usize>,
    freed: Condvar,
}

/// A permit to open one file, given back when it drops.
pub(super) struct Permit<'a>(&'a OpenGate);

impl OpenGate {
    /// A gate that lets `permits` (at least one) workers open at once.
    pub(super) fn new(permits: usize) -> Self {
        Self {
            free: Mutex::new(permits.max(1)),
            freed: Condvar::new(),
        }
    }

    /// Wait for a permit.
    pub(super) fn enter(&self) -> Permit<'_> {
        let mut free = self.free.lock().unwrap_or_else(PoisonError::into_inner);
        while *free == 0 {
            free = self
                .freed
                .wait(free)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *free -= 1;
        Permit(self)
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.0.freed.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// However many workers ask, no more than the permits open at once, and
    /// every one gets its turn.
    #[test]
    fn no_more_workers_open_at_once_than_there_are_permits() {
        let gate = OpenGate::new(2);
        let (open, peak, opened) = (
            AtomicUsize::new(0),
            AtomicUsize::new(0),
            AtomicUsize::new(0),
        );
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..50 {
                        let _permit = gate.enter();
                        let now = open.fetch_add(1, Ordering::SeqCst) + 1;
                        peak.fetch_max(now, Ordering::SeqCst);
                        std::thread::yield_now();
                        open.fetch_sub(1, Ordering::SeqCst);
                        opened.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });
        assert_eq!(opened.load(Ordering::SeqCst), 400);
        assert!(peak.load(Ordering::SeqCst) <= 2);
    }
}
