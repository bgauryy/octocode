//! The bytes a group of memos may hold together, and the oldest-first
//! eviction across them that keeps it.
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

/// One memo's entries as its budget reaches them.
pub(super) trait Held: Send + Sync {
    fn clear(&self);
    /// The use stamp of its least recently used entry.
    fn oldest(&self) -> Option<u64>;
    /// Drop its least recently used entry if that is still the one stamped
    /// `used`; whether it did.
    fn evict_oldest(&self, used: u64) -> bool;
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
}

impl MemoryBudget {
    pub const fn new(limit: fn() -> usize) -> Self {
        Self {
            limit,
            resolved: OnceLock::new(),
            used: AtomicUsize::new(0),
            memos: Mutex::new(Vec::new()),
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
