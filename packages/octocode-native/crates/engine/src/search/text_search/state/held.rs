//! Memory accounting for a search and its retained file outcomes.
use super::FileRec;
use crate::search::text_parser::RawMatch;
use crate::types::TextSearchMatch;
use octocode_io::memo::{MemoryBudget, Reservation};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

/// The bytes one search's evidence may hold while it is collected. Every
/// order ranks the whole scope and every page is cut from the whole
/// collected result, so what a search keeps grows with its matches, not
/// with the page it answers. The bound is the process memory budget
/// ([`crate::memory::MEMORY`]), the one a stored scan must fit to serve its
/// continuation pages: a scan past it could not be kept for them anyway. A
/// search that reaches it stops and is refused
/// ([`crate::error::ErrorKind::ResourceExhausted`]), never cut short.
///
/// The budget is shared: searches that run together (the queries of one
/// batch, or of concurrent calls) hold their bytes as reservations against
/// the same limit ([`Reservation`]), so together they never hold more than
/// it, however many there are. A search with an explicit bound (a caller's
/// or a test's) is judged against that alone.
pub(in super::super) struct Held {
    used: AtomicUsize,
    limit: usize,
    exhausted: AtomicBool,
    /// What this search holds of the shared budget; `None` for an explicit bound.
    shared: Option<Shared>,
}

struct Shared {
    budget: &'static MemoryBudget,
    /// The reservation, made on the first charge and grown in
    /// [`RESERVE_STEP`]s so the many small charges of a scan do not each touch
    /// the budget.
    reservation: Mutex<Option<Reservation<'static>>>,
    /// What the reservation holds, readable without its lock.
    reserved: AtomicUsize,
}

/// How far ahead a search reserves: the granularity of the accounting, not a
/// bound. A reservation that cannot grow by a step grows by what is needed.
const RESERVE_STEP: usize = 1024 * 1024;

impl Held {
    pub(in super::super) fn new(limit: Option<usize>) -> Self {
        match limit {
            Some(limit) => Self::bounded(limit),
            None => Self::sharing(&crate::memory::MEMORY),
        }
    }

    fn bounded(limit: usize) -> Self {
        Self {
            used: AtomicUsize::new(0),
            limit,
            exhausted: AtomicBool::new(false),
            shared: None,
        }
    }

    /// A search holding its bytes against `budget`, shared with every other.
    pub(in super::super) fn sharing(budget: &'static MemoryBudget) -> Self {
        Self {
            shared: Some(Shared {
                budget,
                reservation: Mutex::new(None),
                reserved: AtomicUsize::new(0),
            }),
            ..Self::bounded(budget.limit())
        }
    }

    /// Hold `bytes` more; whether the search still fits its bound (once it
    /// does not, it never does again).
    pub(in super::super) fn charge(&self, bytes: usize) -> bool {
        // Sequentially consistent: a charge counts its bytes before it reads
        // what is reserved, and `give_back` lowers what is reserved before it
        // reads the bytes, so one of the two sees the other.
        let used = self
            .used
            .fetch_add(bytes, Ordering::SeqCst)
            .saturating_add(bytes);
        let fits = match &self.shared {
            Some(shared) => used <= shared.reserved.load(Ordering::SeqCst) || shared.grow(used),
            None => used <= self.limit,
        };
        if !fits {
            self.exhausted.store(true, Ordering::Relaxed);
        }
        !self.exhausted()
    }

    fn release(&self, bytes: usize) {
        self.used.fetch_sub(bytes, Ordering::SeqCst);
        if let Some(shared) = &self.shared {
            shared.give_back(&self.used);
        }
    }

    /// The search held more than its bound at some point.
    pub(in super::super) fn exhausted(&self) -> bool {
        self.exhausted.load(Ordering::Relaxed)
    }

    /// The refusal of a search that passed its bound.
    pub(in super::super) fn refusal(&self) -> crate::error::Error {
        crate::error::Error::resource_exhausted(format!(
            "the search's matches pass the process memory budget ({} bytes) before the search ends",
            self.limit
        ))
    }
}

impl Shared {
    /// Reserve for `used` bytes, a step ahead when the budget has room for it;
    /// whether the budget holds them.
    fn grow(&self, used: usize) -> bool {
        let mut slot = self
            .reservation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let held = slot.as_ref().map_or(0, Reservation::bytes);
        if used <= held {
            return true;
        }
        let need = used - held;
        let grown = match slot.as_mut() {
            Some(reservation) => reservation.grow(need.max(RESERVE_STEP)) || reservation.grow(need),
            None => {
                *slot = self
                    .budget
                    .reserve(need.max(RESERVE_STEP))
                    .or_else(|| self.budget.reserve(need));
                slot.is_some()
            }
        };
        self.reserved.store(
            slot.as_ref().map_or(0, Reservation::bytes),
            Ordering::SeqCst,
        );
        grown
    }

    /// Return the reservation's bytes past what the search holds (`used`)
    /// and a step: what a search released is room for the others.
    ///
    /// `used` is read under the lock, after the lower bound is published, so
    /// a charge that passed the lock-free check against the old reservation
    /// is counted before the shrink, and one that comes later sees the lower
    /// bound and waits for the lock in [`Self::grow`]: the reservation never
    /// falls below the bytes held.
    fn give_back(&self, used: &AtomicUsize) {
        let spare_ahead = used.load(Ordering::SeqCst).saturating_add(2 * RESERVE_STEP);
        if self.reserved.load(Ordering::SeqCst) < spare_ahead {
            return;
        }
        let mut slot = self
            .reservation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(reservation) = slot.as_mut() else {
            return;
        };
        let keep = used.load(Ordering::SeqCst).saturating_add(RESERVE_STEP);
        #[cfg(test)]
        tests::between_read_and_shrink();
        self.reserved
            .store(keep.min(reservation.bytes()), Ordering::SeqCst);
        let keep = keep.max(used.load(Ordering::SeqCst));
        reservation.shrink(reservation.bytes().saturating_sub(keep));
        self.reserved.store(reservation.bytes(), Ordering::SeqCst);
    }
}

/// What one file's search holds so far, released when its outcome is
/// dropped unkept (a re-searched prefix, a filtered file) and kept with
/// its record ([`Self::keep`]).
pub(in super::super) struct Charge {
    held: std::sync::Arc<Held>,
    bytes: usize,
}

impl Charge {
    pub(in super::super) fn new(held: &std::sync::Arc<Held>) -> Self {
        Self {
            held: std::sync::Arc::clone(held),
            bytes: 0,
        }
    }

    /// Hold `bytes` more; whether the search still fits its bound.
    pub(in super::super) fn add(&mut self, bytes: usize) -> bool {
        self.bytes = self.bytes.saturating_add(bytes);
        self.held.charge(bytes)
    }

    /// The file's record is kept: its bytes stay held for the search.
    pub(in super::super) fn keep(mut self) {
        self.bytes = 0;
    }
}

impl Drop for Charge {
    fn drop(&mut self) {
        self.held.release(self.bytes);
    }
}

/// Heap bytes one kept span or prepared row holds.
pub(in super::super) fn match_weight(matched: &TextSearchMatch) -> usize {
    std::mem::size_of::<TextSearchMatch>() + matched.value.capacity()
}

/// Heap bytes one kept matched line holds.
pub(in super::super) fn raw_weight(raw: &RawMatch) -> usize {
    std::mem::size_of::<RawMatch>() + raw.line_text.capacity()
}

/// Heap bytes one kept context line holds.
pub(in super::super) fn context_weight(line: &str) -> usize {
    std::mem::size_of::<(u32, String)>() + line.len()
}

/// Heap bytes of a kept file's record past its rows.
pub(in super::super) fn record_weight(path: &str, term_counts: &[u32]) -> usize {
    std::mem::size_of::<FileRec>() + path.len() + std::mem::size_of_val(term_counts)
}

#[cfg(test)]
mod tests;
