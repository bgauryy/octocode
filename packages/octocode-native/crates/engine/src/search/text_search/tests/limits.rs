//! PCRE2 worker slots and deadlines, and cancellation.

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

/// Test filter that stalls inside the walk on the Nth file it sees, standing in
/// for one uninterruptible PCRE2 match.
struct StallOnNth {
    seen: AtomicU32,
    nth: u32,
    stall: std::time::Duration,
}

impl TextSearchPathFilter for StallOnNth {
    fn allows(&self, _: &Path, is_dir: bool) -> bool {
        if !is_dir && self.seen.fetch_add(1, Ordering::SeqCst) + 1 == self.nth {
            std::thread::sleep(self.stall);
        }
        true
    }
}

#[test]
fn pcre2_hard_deadline_returns_files_finished_so_far() {
    use std::time::{Duration, Instant};
    let t = TmpDir::new();
    for i in 0..6 {
        t.write(&format!("f{i}.txt"), "needle\n");
    }
    let mut o = opts(t.path(), "needle");
    o.syntax = PatternSyntax::Pcre2;
    o.sort = SearchSort::Traversal;
    let filter = Arc::new(StallOnNth {
        seen: AtomicU32::new(0),
        nth: 4,
        stall: Duration::from_millis(1500),
    });
    let started = Instant::now();
    let r = search_with_limits(
        o,
        filter,
        &|| false,
        Pcre2Limits {
            deadline: Duration::from_millis(100),
            grace: Duration::from_millis(100),
        },
    )
    .expect("ok");
    assert!(
        started.elapsed() < Duration::from_millis(1200),
        "driver must not wait for the stuck worker"
    );
    assert_eq!(r.files.len(), 3, "{:?}", r.stats);
    assert_eq!(r.stats.files_searched, Some(3));
    assert_eq!(r.stats.files_matched, Some(3));
    assert_eq!(r.stats.capped, Some(true));
    assert_eq!(r.stats.cap_reasons, [CapReason::Pcre2Deadline]);
}

#[test]
fn pcre2_driver_honours_cancellation_while_the_worker_is_stuck() {
    use std::time::{Duration, Instant};
    let t = TmpDir::new();
    for i in 0..4 {
        t.write(&format!("f{i}.txt"), "needle\n");
    }
    let mut o = opts(t.path(), "needle");
    o.syntax = PatternSyntax::Pcre2;
    o.sort = SearchSort::Traversal;
    let filter = Arc::new(StallOnNth {
        seen: AtomicU32::new(0),
        nth: 2,
        stall: Duration::from_millis(1500),
    });
    let started = Instant::now();
    let cancel = || started.elapsed() >= Duration::from_millis(100);
    let r = search_with_limits(o, filter, &cancel, PCRE2_LIMITS).expect("ok");
    assert!(started.elapsed() < Duration::from_millis(1200));
    assert_eq!(r.files.len(), 1, "{:?}", r.stats);
    assert!(r.stats.cap_reasons.contains(&CapReason::Cancelled));
}
