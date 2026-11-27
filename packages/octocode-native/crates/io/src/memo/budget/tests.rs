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
