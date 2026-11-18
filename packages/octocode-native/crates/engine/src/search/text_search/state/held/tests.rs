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
    let wanted = |thread: usize, round: usize| (thread + 1) * (round % 7 + 1) * RESERVE_STEP / 5;
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
                            broken.push(format!("round {round}: reserved {reserved}, held {used}"));
                        }
                    }
                    phase.wait();
                    drop(kept);
                    phase.wait();
                    if thread == 0 {
                        let (used, reserved) = (held.used.load(Ordering::SeqCst), BUDGET.used());
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
