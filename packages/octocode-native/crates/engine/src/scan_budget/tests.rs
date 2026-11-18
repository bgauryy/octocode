use super::*;
use crate::test_support::clock::ManualWalkClock;
use std::time::Duration;

/// The budget is read on the pacer's clock: standing still, no real time
/// spends it; moved past it, the pacer stops after the first chunk ran.
#[test]
fn the_pacer_spends_its_budget_on_its_clock() {
    let clock = ManualWalkClock::new();
    let mut pacer = ScanPacer::new(Some(SCAN_TIME_BUDGET), clock.walk_clock());
    assert!(!pacer.spent(), "never before the first chunk ran");
    pacer.record(pacer.next_chunk());
    assert!(!pacer.spent(), "a clock standing still spends nothing");
    clock.advance(SCAN_TIME_BUDGET);
    assert!(pacer.spent());
}

#[test]
fn chunks_follow_the_measured_throughput() {
    let budget = Some(Duration::from_secs(10));
    assert_eq!(
        chunk_files(None, 1_000, Duration::from_secs(1)),
        MAX_CHUNK_FILES
    );
    assert_eq!(chunk_files(budget, 0, Duration::ZERO), FIRST_CHUNK_FILES);
    // 10 ms a file: a one-second chunk is 100 files.
    assert_eq!(chunk_files(budget, 128, Duration::from_millis(1_280)), 100);
    // Near the end only what is left of the budget is filled.
    assert_eq!(chunk_files(budget, 950, Duration::from_millis(9_500)), 50);
    // Very slow storage still keeps every worker busy.
    assert_eq!(
        chunk_files(budget, 128, Duration::from_secs(9)),
        MIN_CHUNK_FILES
    );
    // Fast storage runs maximal chunks.
    assert_eq!(
        chunk_files(budget, 128, Duration::from_micros(10)),
        MAX_CHUNK_FILES
    );
}
