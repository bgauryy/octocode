//! The memory the files of one scan share while they are in flight.
//!
//! A scan maps files in parallel, and each file's source correspondence is
//! built before its caller can charge what it keeps. Each file therefore
//! draws every byte it allocates from one [`ScanMemory`]: the files in flight
//! together never hold more than the scan's budget, however many workers run
//! them. A file refused only because others hold the memory is run again
//! alone, once they release it, so what a file may use never depends on
//! which files happen to run beside it.
use crate::graph::GraphFactsDocument;
use crate::text::analysis::{Draw, SourceError};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// One scan's memory budget, shared by every file in flight.
pub struct ScanMemory<'a> {
    budget: usize,
    /// What a file holds before any source correspondence, as its caller
    /// prices it from the facts: the first draw of each file, and the bound
    /// a file must fit beside what is already kept before its source is
    /// bound.
    base: &'a (dyn Fn(&GraphFactsDocument) -> usize + Sync),
    state: Mutex<State>,
    turn: Condvar,
}

#[derive(Default)]
struct State {
    /// Bytes the caller kept ([`ScanMemory::commit`]).
    committed: usize,
    /// Bytes the files in flight hold.
    held: usize,
    /// The most `committed + held` reached.
    peak: usize,
    /// Files waiting to run alone.
    waiting: usize,
    /// A file runs alone.
    solo: bool,
    refused: Option<Refusal>,
}

/// The first file the scan could not keep beside what it already kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub relative_path: String,
    /// The bytes keeping it needed: what was kept plus what it would hold.
    pub needed: usize,
}

impl<'a> ScanMemory<'a> {
    #[must_use]
    pub fn new(budget: usize, base: &'a (dyn Fn(&GraphFactsDocument) -> usize + Sync)) -> Self {
        Self {
            budget,
            base,
            state: Mutex::new(State::default()),
            turn: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn budget(&self) -> usize {
        self.budget
    }

    /// The most a file reserves past what it asked for: a 64th of the
    /// budget, at most a mebibyte, so a draw rarely takes the scan's lock
    /// and the files in flight leave little reserved but unused.
    fn chunk(&self) -> usize {
        (self.budget / 64).clamp(1, 1 << 20)
    }

    /// Bytes the caller has kept.
    #[must_use]
    pub fn committed(&self) -> usize {
        self.lock().committed
    }

    /// The most the scan held at once: kept bytes plus files in flight.
    #[must_use]
    pub fn peak(&self) -> usize {
        self.lock().peak
    }

    /// Records `bytes` the caller keeps from here on (its charge of a kept
    /// file); its admission decides whether they fit.
    pub fn commit(&self, bytes: usize) {
        let mut state = self.lock();
        state.committed = state.committed.saturating_add(bytes);
        state.peak = state.peak.max(state.committed.saturating_add(state.held));
    }

    /// Records the first file the scan cannot keep; the scan stops.
    pub fn refuse(&self, relative_path: &str, needed: usize) {
        self.lock().refused.get_or_insert_with(|| Refusal {
            relative_path: relative_path.to_owned(),
            needed,
        });
    }

    #[must_use]
    pub fn refused(&self) -> Option<Refusal> {
        self.lock().refused.clone()
    }

    /// What a file with `facts` holds before any source correspondence, as
    /// the caller prices it.
    #[must_use]
    pub fn base(&self, facts: &GraphFactsDocument) -> usize {
        (self.base)(facts)
    }

    /// Runs `work` for `hold` with no other file holding memory: other files
    /// in flight finish first, and none starts while it runs.
    pub(super) fn alone<R>(&self, hold: &FileHold<'_>, work: impl FnOnce() -> R) -> R {
        hold.release_all();
        let mut state = self.lock();
        state.waiting += 1;
        while state.solo || state.held > 0 {
            state = self
                .turn
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.waiting -= 1;
        state.solo = true;
        drop(state);
        hold.solo.store(true, Ordering::Relaxed);
        /// Ends the solo run, even when `work` unwinds.
        struct Alone<'x> {
            state: &'x Mutex<State>,
            turn: &'x Condvar,
            solo: &'x AtomicBool,
        }
        impl Drop for Alone<'_> {
            fn drop(&mut self) {
                self.solo.store(false, Ordering::Relaxed);
                self.state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .solo = false;
                self.turn.notify_all();
            }
        }
        let _alone = Alone {
            state: &self.state,
            turn: &self.turn,
            solo: &hold.solo,
        };
        work()
    }
}

/// What one file holds of a [`ScanMemory`], returned when it is dropped.
///
/// A file reserves the scan's memory in chunks ([`ScanMemory::chunk`]) and
/// serves its allocations from that credit without the scan's lock; what
/// it reserved counts as held, so the scan's bound covers the credit too.
pub struct FileHold<'m> {
    memory: &'m ScanMemory<'m>,
    /// Bytes reserved from the scan: `used` plus unused credit.
    held: AtomicUsize,
    /// Bytes this file's budgets hold now.
    used: AtomicUsize,
    solo: AtomicBool,
    contended: AtomicBool,
    needed: AtomicUsize,
    /// [`ScanMemory::base`] of this file's facts, priced once.
    base: usize,
}

impl<'m> FileHold<'m> {
    #[cfg(test)]
    pub(super) fn new(memory: &'m ScanMemory<'m>) -> Self {
        Self::priced(memory, 0)
    }

    /// A hold for a file whose facts cost `base` ([`ScanMemory::base`]).
    pub(super) fn priced(memory: &'m ScanMemory<'m>, base: usize) -> Self {
        Self {
            memory,
            base,
            held: AtomicUsize::new(0),
            used: AtomicUsize::new(0),
            solo: AtomicBool::new(false),
            contended: AtomicBool::new(false),
            needed: AtomicUsize::new(0),
        }
    }

    /// Whether a draw was refused only because other files held memory
    /// (the file runs again alone).
    #[must_use]
    pub fn contended(&self) -> bool {
        self.contended.load(Ordering::Relaxed)
    }

    pub(super) fn take_contended(&self) -> bool {
        self.contended.swap(false, Ordering::Relaxed)
    }

    /// What keeping this file needed at its refused draw: what the scan
    /// kept plus what this file would hold.
    #[must_use]
    pub fn needed(&self) -> usize {
        self.needed.load(Ordering::Relaxed)
    }

    /// What this file holds before its correspondence
    /// ([`ScanMemory::base`]), priced once when it entered the scan's
    /// memory.
    #[must_use]
    pub fn base(&self) -> usize {
        self.base
    }

    /// The scan's memory this file draws from.
    #[must_use]
    pub fn memory(&self) -> &ScanMemory<'m> {
        self.memory
    }

    fn release_all(&self) {
        self.used.store(0, Ordering::Relaxed);
        let held = self.held.swap(0, Ordering::Relaxed);
        if held > 0 {
            let mut state = self.memory.lock();
            state.held = state.held.saturating_sub(held);
            drop(state);
            self.memory.turn.notify_all();
        }
    }
}

impl Draw for FileHold<'_> {
    fn draw(&self, bytes: usize) -> Result<(), SourceError> {
        let used = self.used.load(Ordering::Relaxed);
        let held = self.held.load(Ordering::Relaxed);
        // Served from this file's credit: no lock.
        if held - used >= bytes {
            self.used.store(used + bytes, Ordering::Relaxed);
            return Ok(());
        }
        let memory = self.memory;
        let solo = self.solo.load(Ordering::Relaxed);
        let mut state = memory.lock();
        // A file holding nothing waits while another runs alone or waits to:
        // files already holding memory go on, so theirs is released.
        while !solo && held == 0 && (state.solo || state.waiting > 0) {
            state = memory
                .turn
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        let need = bytes - (held - used);
        let free = memory
            .budget
            .saturating_sub(state.committed.saturating_add(state.held));
        if need <= free {
            let take = need.max(memory.chunk()).min(free);
            state.held += take;
            state.peak = state.peak.max(state.committed + state.held);
            self.held.store(held + take, Ordering::Relaxed);
            self.used.store(used + bytes, Ordering::Relaxed);
            return Ok(());
        }
        if !solo && state.held > held {
            self.contended.store(true, Ordering::Relaxed);
        }
        self.needed.store(
            state.committed.saturating_add(used).saturating_add(bytes),
            Ordering::Relaxed,
        );
        Err(SourceError::LimitExceeded("bytes"))
    }

    /// Freed bytes become this file's credit again; the scan gets them back
    /// when the file ends or reruns alone.
    fn release(&self, bytes: usize) {
        let used = self.used.load(Ordering::Relaxed);
        self.used.store(used - bytes.min(used), Ordering::Relaxed);
    }
}

impl Drop for FileHold<'_> {
    fn drop(&mut self) {
        self.release_all();
    }
}

#[cfg(test)]
mod tests;
