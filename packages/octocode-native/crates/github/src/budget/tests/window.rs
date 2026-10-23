//! The code-search window: waits inside the request, the cap, and slots released by a cancelled or
//! dropped wait.
use super::*;

#[tokio::test]
async fn a_short_code_search_window_wait_is_taken_inside_the_request() {
    let (budget, state) = limited(ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    });
    // The window is full, and its oldest slot frees in ~300 ms: inside
    // the 2 s rate-limit cap.
    {
        let now = now_ms();
        let mut facts = state.facts();
        facts.code_search_ms.push_back(now - 59_700);
        facts.code_search_ms.push_back(now - 10_000);
    }
    let started = Instant::now();
    let admitted = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_secs(10),
            &CancellationToken::new(),
        )
        .await;
    assert!(admitted.is_ok(), "{:?}", admitted.err());
    assert!(started.elapsed() >= Duration::from_millis(200));
}

/// One cap bounds every wait: a window slot that frees after the cap
/// fails fast with its retry-after, as a rate-limit block would.
#[tokio::test]
async fn a_window_wait_past_the_cap_fails_fast() {
    let (budget, state) = full_code_search_window();
    let started = Instant::now();
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(1),
            Instant::now() + Duration::from_secs(10),
            &CancellationToken::new(),
        )
        .await
        .err()
        .expect("window wait past the cap");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.rate_limit.and_then(|limit| limit.retry_after_seconds),
        Some(2)
    );
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[tokio::test]
async fn a_code_search_window_wait_does_not_block_other_searches() {
    let (budget, state) = full_code_search_window();
    let deadline = Instant::now() + Duration::from_secs(10);
    let token = CancellationToken::new();
    let waiting = {
        let (budget, state, token) = (budget.clone(), state.clone(), token.clone());
        tokio::spawn(async move {
            state
                .admit(
                    Some(Group::CodeSearch),
                    budget.config(),
                    true,
                    Duration::from_secs(2),
                    deadline,
                    &token,
                )
                .await
                .map(drop)
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    let started = Instant::now();
    let other = state
        .admit(
            Some(Group::Search),
            budget.config(),
            false,
            Duration::from_secs(1),
            deadline,
            &token,
        )
        .await;
    assert!(other.is_ok(), "{:?}", other.err());
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "repo/issue search waited {:?} behind the code-search window",
        started.elapsed()
    );
    drop(other);
    waiting.await.expect("join").expect("code search admitted");
}

#[tokio::test]
async fn a_cancelled_window_wait_releases_its_reserved_slot() {
    let (budget, state) = full_code_search_window();
    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        canceller.cancel();
    });
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_secs(10),
            &token,
        )
        .await
        .err()
        .expect("cancelled");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
    let facts = state.facts();
    assert_eq!(facts.code_search_ms.len(), 2, "no phantom window slot");
    assert!(
        facts
            .last_start_ms
            .get(Group::CodeSearch.name())
            .is_none_or(|start| *start <= now_ms()),
        "no phantom spacing reservation"
    );
}

/// A caller that drops the admission future mid-wait (a timeout wrapper, a
/// `select!` that loses) leaves no phantom slot either.
#[tokio::test]
async fn a_dropped_window_wait_releases_its_reserved_slot() {
    let (budget, state) = full_code_search_window();
    let token = CancellationToken::new();
    let dropped = tokio::time::timeout(
        Duration::from_millis(100),
        state.admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_secs(10),
            &token,
        ),
    )
    .await;
    assert!(dropped.is_err(), "the wait was dropped, not completed");
    let facts = state.facts();
    assert_eq!(facts.code_search_ms.len(), 2, "no phantom window slot");
    assert!(
        facts
            .last_start_ms
            .get(Group::CodeSearch.name())
            .is_none_or(|start| *start <= now_ms()),
        "no phantom spacing reservation"
    );
}

/// An admission dropped while it waits for its reserved spacing slot
/// hands the slot back through `Drop`: the next admission plans its start
/// after the last admitted request, not after the dropped one.
#[tokio::test]
async fn dropped_admission_releases_its_slot() {
    let spacing = Duration::from_millis(400);
    let (budget, state) = limited(ExecutorConfig {
        search_spacing: spacing,
        ..ExecutorConfig::relaxed()
    });
    let token = CancellationToken::new();
    let admit = || {
        state.admit(
            Some(Group::Search),
            budget.config(),
            false,
            spacing,
            Instant::now() + Duration::from_secs(10),
            &token,
        )
    };
    drop(admit().await.expect("first start"));
    let dropped = tokio::time::timeout(Duration::from_millis(100), admit()).await;
    assert!(
        dropped.is_err(),
        "the spacing wait was dropped, not completed"
    );
    // A leaked slot would push this start a second spacing out, past the
    // one-spacing cap, and refuse it.
    let next = admit().await;
    assert!(next.is_ok(), "{:?}", next.err());
}

#[tokio::test]
async fn code_search_window_fails_fast_when_full() {
    let (budget, state) = limited(ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let token = CancellationToken::new();
    for _ in 0..2 {
        drop(
            state
                .admit(
                    Some(Group::CodeSearch),
                    budget.config(),
                    true,
                    Duration::from_secs(1),
                    deadline,
                    &token,
                )
                .await
                .expect("admitted"),
        );
    }
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(1),
            deadline,
            &token,
        )
        .await
        .err()
        .expect("window full");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.rate_limit.and_then(|r| r.resource).as_deref(),
        Some("code_search")
    );
}
