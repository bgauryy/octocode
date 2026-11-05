//! The bytes a group of memos may hold together, the oldest-first eviction
//! across them that keeps it, and the sweep that releases what expired while
//! nothing calls them.
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::Instant;

/// The least time between two sweeps, so entries coming due one by one wake
/// the process once. Only memory waits this long: a read refuses an expired
/// entry at its exact deadline.
const SWEEP_SPACING: std::time::Duration = std::time::Duration::from_secs(30);

/// One memo's entries as its budget reaches them.
pub(super) trait Held: Send + Sync {
    fn clear(&self);
    /// The use stamp of its least recently used entry.
    fn oldest(&self) -> Option<u64>;
    /// Drop its least recently used entry if that is still the one stamped
    /// `used`; whether it did.
    fn evict_oldest(&self, used: u64) -> bool;
    /// Drop every entry whose deadline has passed at `now`; the next deadline.
    fn sweep(&self, now: Instant) -> Option<Instant>;
}

/// Stores and reads, in order, across every memo: comparing two memos'
/// least recently used entries finds the older.
static USES: AtomicU64 = AtomicU64::new(0);

pub(super) fn next_use() -> u64 {
    USES.fetch_add(1, Ordering::Relaxed)
}

/// The bytes every memo built on it may hold together. `limit` is read
/// once, on first use, so a budget derived from the host is a `static`.
pub struct MemoryBudget {
    limit: fn() -> usize,
    resolved: OnceLock<usize>,
    used: AtomicUsize,
    memos: Mutex<Vec<Weak<dyn Held>>>,
    sweeper: OnceLock<Sweeper>,
}

/// What the sweeping thread sleeps on. Its lock is never held while entering
/// a memo, and a memo notes a deadline only after releasing its own.
struct Sweeper {
    /// The earliest deadline it knows of; `None` sleeps until one is noted.
    next: Mutex<Option<Instant>>,
    noted: Condvar,
}

impl Sweeper {
    fn next(&self) -> MutexGuard<'_, Option<Instant>> {
        self.next.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wake the thread only for a deadline earlier than the one it sleeps on;
    /// a later one is found when that one comes.
    fn note(&self, deadline: Instant) {
        let mut next = self.next();
        if next.is_none_or(|next| deadline < next) {
            *next = Some(deadline);
            self.noted.notify_one();
        }
    }
}

impl MemoryBudget {
    pub const fn new(limit: fn() -> usize) -> Self {
        Self {
            limit,
            resolved: OnceLock::new(),
            used: AtomicUsize::new(0),
            memos: Mutex::new(Vec::new()),
            sweeper: OnceLock::new(),
        }
    }

    /// The bytes its memos may hold together; also the largest value one
    /// of them keeps.
    pub fn limit(&self) -> usize {
        *self.resolved.get_or_init(self.limit)
    }

    /// The bytes its memos hold now.
    pub fn used(&self) -> usize {
        self.used.load(Ordering::Relaxed)
    }

    /// Drop every entry of every live memo built on it. Callers must quiesce
    /// tool execution first; values already borrowed by a caller stay valid.
    pub fn clear(&self) {
        for memo in self.live() {
            memo.clear();
        }
    }

    pub(super) fn register(&self, memo: Arc<dyn Held>) {
        let mut memos = self.memos.lock().unwrap_or_else(PoisonError::into_inner);
        memos.retain(|memo| memo.strong_count() != 0);
        memos.push(Arc::downgrade(&memo));
    }

    pub(super) fn charge(&self, bytes: usize) {
        self.used.fetch_add(bytes, Ordering::Relaxed);
    }

    pub(super) fn release(&self, bytes: usize) {
        // Saturating: a release never underflows a concurrent charge's view.
        let _ = self
            .used
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                Some(used.saturating_sub(bytes))
            });
    }

    /// Evict the least recently used entry across its memos until they fit.
    /// Each memo is locked alone, and never while its caller holds one.
    pub(super) fn reclaim(&self) {
        while self.used() > self.limit() {
            let oldest = self
                .live()
                .into_iter()
                .filter_map(|memo| memo.oldest().map(|used| (used, memo)))
                .min_by_key(|(used, _)| *used);
            let Some((used, memo)) = oldest else {
                return;
            };
            // A memo that changed meanwhile is compared again.
            memo.evict_oldest(used);
        }
    }

    /// Release expired entries from now on with no call on their memos, for a
    /// process that stays up: it spends one sleeping thread, woken only when
    /// an entry can be due, never while no entry has a deadline. Without this
    /// (a one-shot call) an entry leaves on a read of its key, the next
    /// insertion into its memo, or the budget. The thread never delays exit.
    pub fn sweep_expired_while_running(&'static self) {
        let mut first = false;
        let sweeper = self.sweeper.get_or_init(|| {
            first = true;
            Sweeper {
                next: Mutex::new(None),
                noted: Condvar::new(),
            }
        });
        if !first {
            return;
        }
        if let Some(deadline) = self.sweep(Instant::now()) {
            sweeper.note(deadline);
        }
        // A thread refused leaves memos as a one-shot call has them.
        let _ = std::thread::Builder::new()
            .name("octocode-memo-sweep".into())
            .spawn(move || self.sweep_when_due(sweeper));
    }

    fn sweep_when_due(&self, sweeper: &Sweeper) -> ! {
        let mut earliest = Instant::now();
        let mut next = sweeper.next();
        loop {
            let Some(deadline) = *next else {
                next = sweeper
                    .noted
                    .wait(next)
                    .unwrap_or_else(PoisonError::into_inner);
                continue;
            };
            let now = Instant::now();
            let at = deadline.max(earliest);
            if now < at {
                next = sweeper
                    .noted
                    .wait_timeout(next, at - now)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
                continue;
            }
            // Cleared first: a deadline noted from here on is an entry this
            // sweep may miss, and it stays noted.
            *next = None;
            drop(next);
            let due = self.sweep(now);
            earliest = now + SWEEP_SPACING;
            if let Some(due) = due {
                sweeper.note(due);
            }
            next = sweeper.next();
        }
    }

    /// Drop what expired at `now` in every memo; the next deadline among them.
    pub(super) fn sweep(&self, now: Instant) -> Option<Instant> {
        self.live()
            .into_iter()
            .filter_map(|memo| memo.sweep(now))
            .min()
    }

    /// An entry now expires at `deadline`.
    pub(super) fn note_deadline(&self, deadline: Instant) {
        if let Some(sweeper) = self.sweeper.get() {
            sweeper.note(deadline);
        }
    }

    /// The memos still alive; the list's lock is released before any is entered.
    fn live(&self) -> Vec<Arc<dyn Held>> {
        self.memos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter_map(Weak::upgrade)
            .collect()
    }
}
