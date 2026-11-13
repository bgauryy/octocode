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
    /// The part of `used` that live [`Reservation`]s hold: what no eviction
    /// can free, so a grow fits only while this stays within the limit.
    reserved: AtomicUsize,
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
            reserved: AtomicUsize::new(0),
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

    /// Admit bytes only when they fit without eviction. The check and charge
    /// are one atomic update, including charges made by different memos.
    pub(super) fn try_charge(&self, bytes: usize) -> bool {
        let limit = self.limit();
        self.used
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes).filter(|total| *total <= limit)
            })
            .is_ok()
    }

    pub(super) fn release(&self, bytes: usize) {
        // Saturating: a release never underflows a concurrent charge's view.
        let _ = self
            .used
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                Some(used.saturating_sub(bytes))
            });
    }

    /// Hold `bytes` of transient work (a scan's result vector) against the
    /// budget until the returned [`Reservation`] drops, evicting memo entries
    /// oldest first to make room. `None` when the bytes cannot fit even with
    /// every memo emptied: the caller stops its work instead of allocating
    /// past the budget.
    pub fn reserve(&self, bytes: usize) -> Option<Reservation<'_>> {
        let mut held = Reservation {
            budget: self,
            bytes: 0,
        };
        held.grow(bytes).then_some(held)
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

/// Bytes of transient work held against a [`MemoryBudget`], returned when it
/// drops. Memos give way to it; it never gives way to a memo.
pub struct Reservation<'a> {
    budget: &'a MemoryBudget,
    bytes: usize,
}

impl std::fmt::Debug for Reservation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reservation")
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

impl Reservation<'_> {
    /// Hold `more` bytes further; `false` (holding nothing more) when they
    /// cannot fit within the budget's limit even with every memo emptied.
    pub fn grow(&mut self, more: usize) -> bool {
        let limit = self.budget.limit();
        // Checked and counted in one step, before any eviction: concurrent
        // grows are admitted against each other's bytes, a grow that cannot
        // fit evicts nothing, and one that can always fits once memos give way.
        let admitted = self
            .budget
            .reserved
            .try_update(Ordering::AcqRel, Ordering::Acquire, |reserved| {
                reserved.checked_add(more).filter(|total| *total <= limit)
            })
            .is_ok();
        if !admitted {
            return false;
        }
        self.budget.charge(more);
        // Reservations alone fit the limit (checked above), so once the memos
        // give way this grow fits. Bytes a concurrent insert charges after
        // the reclaim are that insert's own to reclaim: never a reason to
        // refuse a grow that fits.
        self.budget.reclaim();
        self.bytes += more;
        true
    }

    /// Give back `bytes` of what it holds (at most all of it), for memory the
    /// work no longer holds.
    pub fn shrink(&mut self, bytes: usize) {
        let bytes = bytes.min(self.bytes);
        self.bytes -= bytes;
        self.budget.release(bytes);
        self.budget.reserved.fetch_sub(bytes, Ordering::AcqRel);
    }

    /// The bytes it holds now.
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        self.shrink(self.bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One read's last byte fits while another asks for more than can ever
    /// fit: charging before checking let the refused ask's charge push the
    /// fitting read past the limit, and both were refused.
    #[test]
    fn a_grow_that_fits_is_not_refused_by_a_concurrent_one_that_cannot() {
        fn limit() -> usize {
            2
        }
        for _ in 0..2000 {
            let budget = MemoryBudget::new(limit);
            let gate = std::sync::Barrier::new(2);
            let ((fits, _fits), (oversized, _oversized)) = std::thread::scope(|scope| {
                let fits = scope.spawn(|| {
                    let mut held = budget.reserve(1).expect("fits");
                    gate.wait();
                    (held.grow(1), held)
                });
                let oversized = scope.spawn(|| {
                    let mut held = budget.reserve(0).expect("empty");
                    gate.wait();
                    (held.grow(2), held)
                });
                (
                    fits.join().expect("thread"),
                    oversized.join().expect("thread"),
                )
            });
            assert!(fits, "the last byte fits whatever the other asks");
            assert!(!oversized, "2 more bytes cannot fit beside 1 held");
            drop((_fits, _oversized));
            assert_eq!(budget.used(), 0);
        }
    }

    #[test]
    fn a_grow_that_cannot_fit_evicts_nothing() {
        use super::super::Memo;
        fn limit() -> usize {
            10
        }
        static BUDGET: MemoryBudget = MemoryBudget::new(limit);
        let memo = Memo::<&str, u8>::new(&BUDGET);
        memo.insert("kept", 1, 4, None);
        let mut held = BUDGET.reserve(8).expect("evicts the memo to fit");
        memo.insert("again", 1, 2, None);
        assert!(!held.grow(3), "8 + 3 cannot fit whatever is evicted");
        assert!(memo.get("again").is_some(), "a refusal evicts nothing");
        assert!(held.grow(2));
        assert!(memo.get("again").is_none(), "a grow that fits evicts");
    }

    /// Reservations alone fit the limit, so a grow is never refused for the
    /// memo bytes a concurrent insert charged after this grow's reclaim: that
    /// insert reclaims for itself.
    #[test]
    fn a_grow_is_not_refused_for_what_a_concurrent_insert_charged() {
        use super::super::Memo;
        use std::sync::atomic::{AtomicBool, Ordering};
        fn limit() -> usize {
            100
        }
        static BUDGET: MemoryBudget = MemoryBudget::new(limit);
        let memo = Memo::<u64, u8>::new(&BUDGET);
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let mut key = 0;
                while !done.load(Ordering::Relaxed) {
                    key = (key + 1) % 4;
                    memo.insert(key, 1, 60, None);
                }
            });
            for round in 0..50_000 {
                let mut held = BUDGET.reserve(0).expect("empty");
                let fits = held.grow(limit() / 2);
                if !fits {
                    done.store(true, Ordering::Relaxed);
                }
                assert!(fits, "round {round}: 50 of 100 bytes always fit alone");
            }
            done.store(true, Ordering::Relaxed);
        });
    }

    #[test]
    fn shrink_returns_bytes_to_the_budget() {
        fn limit() -> usize {
            10
        }
        let budget = MemoryBudget::new(limit);
        let mut held = budget.reserve(10).expect("fits");
        held.shrink(6);
        assert_eq!((held.bytes(), budget.used()), (4, 4));
        assert!(held.grow(6));
    }
}
