//! The hedge between the syntax scan and the server answer.
use super::*;

/// The syntax scan runs while the server request is pending, so a failed
/// request costs `max(request, scan)`, not their sum; an answer in time
/// drops the scan; a failure with no fallback never waits for it.
#[tokio::test]
async fn the_hedge_overlaps_the_primary_and_yields_to_its_answer() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    let sleep = |ms| tokio::time::sleep(Duration::from_millis(ms));

    let started = Instant::now();
    let outcome = hedged(
        async {
            sleep(300).await;
            Err::<u8, _>("timeout")
        },
        async {
            sleep(250).await;
            "scan"
        },
        |_| true,
        |_| None,
        |_| "overdue",
        false,
    )
    .await;
    assert_eq!(outcome, Err(("timeout", Some("scan"))));
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "sequential would take 550 ms, took {:?}",
        started.elapsed()
    );

    // A hedge slower than the failure is awaited to its end.
    let outcome = hedged(
        async { Err::<u8, _>("crash") },
        async {
            sleep(50).await;
            "scan"
        },
        |_| true,
        |_| None,
        |_| "overdue",
        false,
    )
    .await;
    assert_eq!(outcome, Err(("crash", Some("scan"))));

    // A failure with no fallback does not wait for the hedge.
    let started = Instant::now();
    let outcome = hedged(
        async { Err::<u8, _>("cancelled") },
        async {
            sleep(2_000).await;
            "scan"
        },
        |failure| *failure != "cancelled",
        |_| None,
        |_| "overdue",
        false,
    )
    .await;
    assert_eq!(outcome, Err(("cancelled", None)));
    assert!(started.elapsed() < Duration::from_millis(1_000));

    // An answer drops the hedge before it finishes.
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let guard = Dropped(Arc::clone(&dropped));
    let outcome = hedged(
        async {
            sleep(20).await;
            Ok::<u8, &str>(7)
        },
        async move {
            let _guard = guard;
            sleep(5_000).await;
            "scan"
        },
        |_| true,
        |_| None,
        |_| "overdue",
        false,
    )
    .await;
    assert_eq!(outcome, Ok((7, None)));
    assert!(dropped.load(Ordering::SeqCst), "the answer drops the scan");

    // `keep` waits for the hedge and returns it beside the answer.
    let outcome = hedged(
        async { Ok::<u8, &str>(5) },
        async {
            sleep(50).await;
            "scan"
        },
        |_| true,
        |_| None,
        |_| "overdue",
        true,
    )
    .await;
    assert_eq!(outcome, Ok((5, Some("scan"))));
}

/// Once the scan has finished the server has a bounded grace: a primary
/// answering inside it wins, one still silent after it yields the scan
/// without waiting for the primary's own timeout.
#[tokio::test]
async fn a_server_silent_after_the_scan_loses_only_after_its_grace() {
    use std::time::{Duration, Instant};
    let sleep = |ms| tokio::time::sleep(Duration::from_millis(ms));
    let grace = |_: &&str| Some(Duration::from_millis(200));

    let started = Instant::now();
    let outcome = hedged(
        async {
            sleep(5_000).await;
            Ok::<u8, &str>(1)
        },
        async {
            sleep(50).await;
            "scan"
        },
        |_| true,
        grace,
        |_| "overdue",
        false,
    )
    .await;
    assert_eq!(outcome, Err(("overdue", Some("scan"))));
    assert!(
        started.elapsed() < Duration::from_millis(1_500),
        "took {:?}",
        started.elapsed()
    );

    // An answer inside the grace wins over the finished scan.
    let outcome = hedged(
        async {
            sleep(150).await;
            Ok::<u8, &str>(9)
        },
        async {
            sleep(50).await;
            "scan"
        },
        |_| true,
        grace,
        |_| "overdue",
        false,
    )
    .await;
    assert_eq!(outcome, Ok((9, None)));

    // A scan with nothing to offer grants no deadline.
    let outcome = hedged(
        async {
            sleep(300).await;
            Ok::<u8, &str>(3)
        },
        async {
            sleep(10).await;
            "empty"
        },
        |_| true,
        |_| None,
        |_| "overdue",
        false,
    )
    .await;
    assert_eq!(outcome, Ok((3, None)));
}
