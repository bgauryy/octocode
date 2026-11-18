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

/// However far a cancel lets a search get, its result is either whole or says
/// it was cancelled: a cancel first seen inside a file (its read, its matched
/// lines) is reported like one seen between walk entries. The root is one
/// file, searched on the calling thread, so the cancel is first seen at the
/// same point on every run, and every point the search looks at it is tried.
#[test]
fn a_cancel_seen_anywhere_in_the_search_is_reported() {
    let t = TmpDir::new();
    let file = t.write("hits.txt", "needle\n".repeat(300));
    let root = file.to_string_lossy().into_owned();
    let search = |cancel_at: u32| {
        let mut o = opts(root.clone(), "needle");
        o.sort = SearchSort::Traversal;
        let polls = AtomicU32::new(0);
        let cancel = || polls.fetch_add(1, Ordering::SeqCst) >= cancel_at;
        let result = search_cancellable(o, Arc::new(AllowAll), &cancel).expect("search");
        (result.stats, polls.load(Ordering::SeqCst))
    };
    let (whole, polls) = search(u32::MAX);
    assert!(!whole.cap_reasons.contains(&CapReason::Cancelled));
    assert!(
        polls > 1,
        "the search looks at the cancel inside the file too"
    );
    for cancel_at in 0..polls {
        let (cut, _) = search(cancel_at);
        let reported = cut.cap_reasons.contains(&CapReason::Cancelled);
        assert!(
            reported
                || (cut.match_count, cut.matched_lines, cut.files_searched)
                    == (whole.match_count, whole.matched_lines, whole.files_searched),
            "cancelled from poll {cancel_at} of {polls}: neither whole nor reported cancelled: {cut:?}"
        );
        assert!(!reported || cut.capped == Some(true), "{cut:?}");
    }
}
