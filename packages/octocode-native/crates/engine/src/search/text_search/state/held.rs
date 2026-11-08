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
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    thread_local! {
        /// Runs once on this thread where `give_back` holds its lock and has
        /// decided what to give back, before it shrinks: the window in which
        /// another search's charge can land.
        static BETWEEN: RefCell<Option<Box<dyn FnOnce()>>> = const { RefCell::new(None) };
    }

    pub(super) fn between_read_and_shrink() {
        if let Some(hook) = BETWEEN.with(|slot| slot.borrow_mut().take()) {
            hook();
        }
    }

    fn limit() -> usize {
        4 * RESERVE_STEP
    }

    /// Searches that run together share one budget: the second of two that
    /// each hold most of it is refused, where each alone would fit (as every
    /// query of a batch did when each was judged against the whole budget).
    #[test]
    fn searches_running_together_hold_no_more_than_the_budget() {
        static BUDGET: MemoryBudget = MemoryBudget::new(limit);
        let first = Arc::new(Held::sharing(&BUDGET));
        let second = Arc::new(Held::sharing(&BUDGET));
        let mut one = Charge::new(&first);
        let mut other = Charge::new(&second);
        let most = limit() * 3 / 4;
        assert!(one.add(most), "alone, it fits");
        assert!(!other.add(most), "together, they do not");
        assert!(second.exhausted() && !first.exhausted());
        assert!(BUDGET.used() <= BUDGET.limit());
    }

    /// What a finished search held is room for the next, whatever it kept.
    #[test]
    fn a_finished_search_returns_its_bytes_to_the_budget() {
        static BUDGET: MemoryBudget = MemoryBudget::new(limit);
        let first = Arc::new(Held::sharing(&BUDGET));
        let mut charge = Charge::new(&first);
        assert!(charge.add(limit() * 3 / 4));
        drop(charge);
        drop(first);
        assert_eq!(BUDGET.used(), 0);
        let second = Arc::new(Held::sharing(&BUDGET));
        assert!(Charge::new(&second).add(limit() * 3 / 4));
    }

    /// Bytes a search drops mid-scan (filtered files) are room for the others
    /// while it runs on.
    #[test]
    fn bytes_released_mid_search_are_room_for_others() {
        static BUDGET: MemoryBudget = MemoryBudget::new(limit);
        let first = Arc::new(Held::sharing(&BUDGET));
        let second = Arc::new(Held::sharing(&BUDGET));
        let mut dropped = Charge::new(&first);
        assert!(dropped.add(limit() * 3 / 4));
        drop(dropped);
        assert!(Charge::new(&second).add(limit() * 3 / 4));
        assert!(!first.exhausted());
    }

    /// An explicit bound is judged alone, whatever the shared budget holds.
    #[test]
    fn an_explicit_bound_is_the_searchs_own() {
        let held = Arc::new(Held::new(Some(10)));
        let mut charge = Charge::new(&held);
        assert!(charge.add(10));
        assert!(!charge.add(1));
    }

    /// A charge that lands after a release decided what to give back and
    /// before it shrinks is held too: the reservation never falls below it.
    #[test]
    fn a_charge_landing_while_a_release_shrinks_stays_reserved() {
        static BUDGET: MemoryBudget = MemoryBudget::new(limit);
        let held = Arc::new(Held::sharing(&BUDGET));
        let mut releasing = Charge::new(&held);
        assert!(releasing.add(3 * RESERVE_STEP));
        let landing = Rc::new(RefCell::new(Some(Charge::new(&held))));
        let at_the_window = Rc::clone(&landing);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let in_the_window = Rc::clone(&seen);
        let inner = Arc::clone(&held);
        BETWEEN.with(|slot| {
            *slot.borrow_mut() = Some(Box::new(move || {
                let mut charge = at_the_window.borrow_mut();
                let charge = charge.as_mut().expect("the landing charge");
                assert!(charge.add(2 * RESERVE_STEP));
                in_the_window
                    .borrow_mut()
                    .push((BUDGET.used(), inner.used.load(Ordering::Relaxed)));
            }));
        });
        drop(releasing);
        let hook_ran = seen.borrow().len();
        assert_eq!(hook_ran, 1, "the window opened once");
        for (reserved, held) in seen
            .borrow()
            .iter()
            .chain([&(BUDGET.used(), held.used.load(Ordering::Relaxed))])
        {
            assert!(reserved >= held, "reserved {reserved} < held {held}");
        }
        drop(landing);
        drop(held);
        assert_eq!(BUDGET.used(), 0);
    }

    /// A search without an explicit bound holds its bytes against the process
    /// budget, the one a stored scan must fit to serve its continuation pages.
    #[test]
    fn a_search_without_a_bound_holds_its_bytes_against_the_process_budget() {
        let held = Arc::new(Held::new(None));
        let shared = held.shared.as_ref().expect("a shared search");
        assert!(std::ptr::eq(shared.budget, &crate::memory::MEMORY));
        assert_eq!(held.limit, crate::memory::MEMORY.limit());
        let mut charge = Charge::new(&held);
        assert!(charge.add(10));
        // Other searches hold the process budget too: only a lower bound.
        assert!(crate::memory::MEMORY.used() >= shared.reserved.load(Ordering::SeqCst));
        assert!(shared.reserved.load(Ordering::SeqCst) >= 10);
    }

    /// Searches charging and releasing together keep the reservation at what
    /// they hold, between every phase, and return every byte when they end.
    #[test]
    fn concurrent_charges_and_releases_keep_the_reservation_at_what_is_held() {
        const THREADS: usize = 4;
        const ROUNDS: usize = 40;
        fn roomy() -> usize {
            64 * RESERVE_STEP
        }
        static BUDGET: MemoryBudget = MemoryBudget::new(roomy);
        let held = Arc::new(Held::sharing(&BUDGET));
        let phase = std::sync::Barrier::new(THREADS);
        // Broken invariants are collected, not asserted in a thread: a thread
        // that panicked would leave the others waiting at the barrier.
        let broken = Mutex::new(Vec::new());
        let wanted =
            |thread: usize, round: usize| (thread + 1) * (round % 7 + 1) * RESERVE_STEP / 5;
        std::thread::scope(|scope| {
            for thread in 0..THREADS {
                let (held, phase, broken) = (&held, &phase, &broken);
                scope.spawn(move || {
                    for round in 0..ROUNDS {
                        let mut kept = Charge::new(held);
                        let total = (0..THREADS).map(|t| wanted(t, round)).sum::<usize>();
                        // Churn the others see while they charge: bytes
                        // charged and released before the phase settles.
                        for _ in 0..round % 5 {
                            let mut passing = Charge::new(held);
                            if !passing.add(wanted(thread, round)) {
                                broken
                                    .lock()
                                    .unwrap()
                                    .push(format!("round {round}: refused"));
                            }
                        }
                        if !kept.add(wanted(thread, round)) {
                            broken
                                .lock()
                                .unwrap()
                                .push(format!("round {round}: refused"));
                        }
                        phase.wait();
                        if thread == 0 {
                            let used = held.used.load(Ordering::SeqCst);
                            let reserved = BUDGET.used();
                            let mut broken = broken.lock().unwrap();
                            if used != total {
                                broken.push(format!("round {round}: held {used} != {total}"));
                            }
                            if reserved < used || reserved >= used + 2 * RESERVE_STEP {
                                broken.push(format!(
                                    "round {round}: reserved {reserved}, held {used}"
                                ));
                            }
                        }
                        phase.wait();
                        drop(kept);
                        phase.wait();
                        if thread == 0 {
                            let (used, reserved) =
                                (held.used.load(Ordering::SeqCst), BUDGET.used());
                            if used != 0 || reserved >= 2 * RESERVE_STEP {
                                broken.lock().unwrap().push(format!(
                                    "round {round} released: held {used}, reserved {reserved}"
                                ));
                            }
                        }
                        phase.wait();
                    }
                });
            }
        });
        assert_eq!(*broken.lock().unwrap(), Vec::<String>::new());
        drop(held);
        assert_eq!(BUDGET.used(), 0, "every byte returned when the search ends");
    }
}
