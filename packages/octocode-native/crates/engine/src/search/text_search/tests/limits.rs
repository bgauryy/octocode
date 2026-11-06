//! PCRE2 worker slots, and cancellation.

use super::*;

#[test]
fn pcre2_worker_slots_are_bounded_and_released() {
    // The worker-slot bound rejects new acquisitions once saturated and
    // frees a slot on release — tested directly on the counter logic.
    use std::sync::atomic::AtomicUsize;
    let counter = AtomicUsize::new(0);
    let max = 3;
    assert!(try_acquire_worker_slot(&counter, max));
    assert!(try_acquire_worker_slot(&counter, max));
    assert!(try_acquire_worker_slot(&counter, max));
    // Saturated: the 4th acquisition is rejected and the counter is unchanged.
    assert!(!try_acquire_worker_slot(&counter, max));
    assert_eq!(counter.load(Ordering::Relaxed), 3);
    // Releasing frees exactly one slot.
    release_worker_slot(&counter);
    assert_eq!(counter.load(Ordering::Relaxed), 2);
    assert!(try_acquire_worker_slot(&counter, max));
    assert!(!try_acquire_worker_slot(&counter, max));
}

#[test]
fn cancellation_stops_the_walk_mid_tree() {
    let t = TmpDir::new();
    for i in 0..60 {
        t.write(&format!("f{i:02}.txt"), "needle\n");
    }
    let mut o = opts(t.path(), "needle");
    o.sort = SearchSort::Traversal;
    let polls = AtomicU32::new(0);
    let cancel = || polls.fetch_add(1, Ordering::SeqCst) >= 6;
    let r = search_cancellable(o, Arc::new(AllowAll), &cancel).expect("ok");
    let searched = r.stats.files_searched.unwrap_or(0);
    assert!(searched <= 6, "walk kept going after cancel: {searched}");
    assert!(
        r.stats.cap_reasons.contains(&CapReason::Cancelled),
        "{:?}",
        r.stats
    );
    assert_eq!(r.stats.capped, Some(true));
}
