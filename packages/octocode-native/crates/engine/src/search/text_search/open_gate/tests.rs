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
