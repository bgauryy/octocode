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
mod tests;
