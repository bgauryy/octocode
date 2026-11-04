use super::*;

#[tokio::test(start_paused = true)]
async fn progress_tracker_caps_active_begin_tokens() {
    // A server that emits an unbounded stream of distinct begin tokens must
    // not grow the active set without limit.
    let tracker = ProgressTracker::new();
    for index in 0..(MAX_ACTIVE_PROGRESS_TOKENS + 50) {
        tracker.on_begin(format!("token-{index}")).await;
    }
    assert_eq!(*tracker.count_rx.borrow(), MAX_ACTIVE_PROGRESS_TOKENS);
}

/// How long `wait_until_idle` waits, and what it reports, on paused time.
#[tokio::test(start_paused = true)]
async fn progress_tracker_wait_is_bounded_by_settle_and_caller_timeout() {
    let cases = [
        // Never saw progress -> only the settle window elapsed.
        (
            "settle is bounded by the caller timeout",
            None,
            150,
            150,
            Readiness::SilentServer,
        ),
        // A server that indexes WITHOUT progress events gets the conservative
        // settle window, not an aggressive 100 ms one.
        (
            "full settle when no progress and ample timeout",
            None,
            10_000,
            2_000,
            Readiness::SilentServer,
        ),
        (
            "times out when a token never ends",
            Some("stuck"),
            300,
            300,
            Readiness::Timeout,
        ),
    ];
    for (case, token, timeout_ms, elapsed_ms, expected) in cases {
        let tracker = ProgressTracker::new();
        if let Some(token) = token {
            tracker.on_begin(token.to_owned()).await;
        }
        let start = Instant::now();
        let readiness = tracker.wait_until_idle(timeout_ms, STARTUP_SETTLE_MS).await;
        assert_eq!(start.elapsed(), Duration::from_millis(elapsed_ms), "{case}");
        assert_eq!(readiness, expected, "{case}");
    }
}

#[tokio::test(start_paused = true)]
async fn progress_tracker_waits_for_active_token_then_returns_idle() {
    let tracker = ProgressTracker::new();
    let t = Arc::clone(&tracker);
    tokio::spawn(async move {
        t.on_begin("indexing".to_owned()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        t.on_end("indexing").await;
    });
    let readiness = tracker.wait_until_idle(5_000, STARTUP_SETTLE_MS).await;
    assert_eq!(*tracker.count_rx.borrow(), 0);
    assert_eq!(readiness, Readiness::ProgressIdle);
}

#[tokio::test(start_paused = true)]
async fn server_status_not_quiescent_holds_readiness_until_quiescent() {
    let tracker = ProgressTracker::new();
    let status = Arc::clone(&tracker);
    tokio::spawn(async move {
        status.on_server_status(false).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        status.on_server_status(true).await;
    });
    let started = Instant::now();
    assert_eq!(
        tracker.wait_until_idle(5_000, STARTUP_SETTLE_MS).await,
        Readiness::ProgressIdle
    );
    assert!(started.elapsed() >= Duration::from_millis(500));

    // Quiescent with nothing open is not activity, and never goes negative.
    let tracker = ProgressTracker::new();
    tracker.on_server_status(true).await;
    assert_eq!(*tracker.count_rx.borrow(), 0);
    tracker.on_server_status(false).await;
    tracker.on_server_status(false).await;
    assert_eq!(
        *tracker.count_rx.borrow(),
        1,
        "one status token, not one per report"
    );
    assert_eq!(
        tracker.wait_until_idle(300, STARTUP_SETTLE_MS).await,
        Readiness::Timeout
    );
}

#[tokio::test(start_paused = true)]
async fn progress_tracker_completed_cycle_still_requires_quiescence() {
    let tracker = ProgressTracker::new();
    tracker.on_begin("indexing".to_owned()).await;
    tracker.on_end("indexing").await;
    let started = Instant::now();
    let readiness = tracker.wait_until_idle(5_000, STARTUP_SETTLE_MS).await;
    assert_eq!(readiness, Readiness::ProgressIdle);
    assert!(started.elapsed() >= Duration::from_millis(200));
}

#[tokio::test(start_paused = true)]
async fn progress_tracker_cannot_confirm_idle_without_full_quiescence_budget() {
    let tracker = ProgressTracker::new();
    tracker.on_begin("indexing".to_owned()).await;
    tracker.on_end("indexing").await;
    assert_eq!(
        tracker.wait_until_idle(20, STARTUP_SETTLE_MS).await,
        Readiness::Timeout
    );
}

#[tokio::test(start_paused = true)]
async fn progress_tracker_quiescence_restarts_for_a_followup_wave() {
    let tracker = ProgressTracker::new();
    tracker.on_begin("first".to_owned()).await;
    tracker.on_end("first").await;
    let followup = Arc::clone(&tracker);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        followup.on_begin("second".to_owned()).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        followup.on_end("second").await;
    });
    let started = Instant::now();
    assert_eq!(
        tracker.wait_until_idle(2_000, STARTUP_SETTLE_MS).await,
        Readiness::ProgressIdle
    );
    assert!(started.elapsed() >= Duration::from_millis(450));
}

/// A project load much longer than the settle window is a progress wait,
/// not a settle: `wait_until_idle` lasts as long as the load and reports
/// `ProgressIdle`, never `SilentServer`.
#[tokio::test(start_paused = true)]
async fn a_load_that_outlasts_the_settle_window_is_not_a_silent_server() {
    let tracker = ProgressTracker::new();
    let server = Arc::clone(&tracker);
    tokio::spawn(async move {
        server.on_begin("load".to_owned()).await;
        tokio::time::sleep(Duration::from_millis(9_300)).await;
        server.on_end("load").await;
    });
    let started = Instant::now();
    assert_eq!(
        tracker.wait_until_idle(60_000, STARTUP_SETTLE_MS).await,
        Readiness::ProgressIdle
    );
    assert!(started.elapsed() >= Duration::from_millis(9_300));
}

/// A wait owed nothing (`settle_ms` 0 after a load the startup wait
/// already confirmed) returns at once as `SilentServer`: "not confirmed
/// by this wait", not "the server never reported progress". A caller
/// must not report it as the server's readiness over the startup
/// wait's `ProgressIdle`.
#[tokio::test(start_paused = true)]
async fn a_wait_owed_nothing_returns_at_once_without_confirming() {
    let tracker = ProgressTracker::new();
    tracker.on_begin("load".to_owned()).await;
    tracker.on_end("load").await;
    let observed = tracker.subscribe();
    let started = Instant::now();
    assert_eq!(
        tracker.wait_until_idle_after(observed, 0, 60_000).await,
        Readiness::SilentServer
    );
    assert_eq!(started.elapsed(), Duration::ZERO);
}
