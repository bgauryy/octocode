//! Refusals decided before queueing: the window peek, queued admissions and retries, and the
//! counters a refusal claims.
use super::*;

/// A refusal the window already decides is returned before the group
/// permit, not after queueing behind another request's permit.
#[tokio::test]
async fn a_window_refusal_is_decided_before_the_group_permit() {
    let (budget, state) = full_code_search_window();
    let semaphore = state
        .groups
        .get(Group::CodeSearch.name())
        .expect("code-search group")
        .permit
        .clone();
    let _held = semaphore.acquire_owned().await.expect("held permit");
    let started = Instant::now();
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(1),
            Instant::now() + Duration::from_secs(5),
            &CancellationToken::new(),
        )
        .await
        .err()
        .expect("window full");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "refused after {:?}",
        started.elapsed()
    );
}

/// The peek counts the admission queued ahead: with one window slot left
/// and one sibling waiting for it, the next is refused at once instead of
/// queueing behind the sibling and being refused after the wait.
#[tokio::test]
async fn a_peek_counts_the_admissions_queued_ahead_in_this_process() {
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    }));
    let state = budget.key_state(&LimiterKey::new("h", None), None);
    state.facts().code_search_ms.push_back(now_ms() - 1_000);
    let permit = state
        .groups
        .get(Group::CodeSearch.name())
        .expect("code-search group")
        .permit
        .clone();
    let held = permit.acquire_owned().await.expect("held permit");
    let token = CancellationToken::new();
    let ahead = {
        let (budget, state, token) = (Arc::clone(&budget), Arc::clone(&state), token.clone());
        tokio::spawn(async move {
            state
                .admit(
                    Some(Group::CodeSearch),
                    budget.config(),
                    true,
                    Duration::from_secs(1),
                    Instant::now() + Duration::from_secs(10),
                    &token,
                )
                .await
                .map(drop)
        })
    };
    while state.groups[Group::CodeSearch.name()]
        .queued
        .ahead()
        .windowed
        == 0
    {
        tokio::task::yield_now().await;
    }
    let started = Instant::now();
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(1),
            Instant::now() + Duration::from_secs(10),
            &token,
        )
        .await
        .err()
        .expect("the last slot is taken by the sibling ahead");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "refused after {:?}",
        started.elapsed()
    );
    drop(held);
    ahead
        .await
        .expect("join")
        .expect("the sibling gets the slot");
}

/// A retry queued ahead takes its spacing slot but no window slot: with
/// one window slot left, a new search queued behind it is still admitted.
#[tokio::test]
async fn a_retry_queued_ahead_takes_no_window_slot() {
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        code_search_per_minute: 2,
        search_spacing: Duration::from_millis(50),
        ..ExecutorConfig::relaxed()
    }));
    let state = budget.key_state(&LimiterKey::new("h", None), None);
    state.facts().code_search_ms.push_back(now_ms() - 1_000);
    let permit = state.groups[Group::CodeSearch.name()].permit.clone();
    let held = permit.acquire_owned().await.expect("held permit");
    let token = CancellationToken::new();
    let admit = |window: bool| {
        let (budget, state, token) = (Arc::clone(&budget), Arc::clone(&state), token.clone());
        tokio::spawn(async move {
            state
                .admit(
                    Some(Group::CodeSearch),
                    budget.config(),
                    window,
                    Duration::from_secs(1),
                    Instant::now() + Duration::from_secs(10),
                    &token,
                )
                .await
                .map(drop)
        })
    };
    let retry = admit(false);
    while state.groups[Group::CodeSearch.name()].queued.ahead().plain == 0 {
        tokio::task::yield_now().await;
    }
    let search = admit(true);
    while state.groups[Group::CodeSearch.name()]
        .queued
        .ahead()
        .windowed
        == 0
    {
        tokio::task::yield_now().await;
    }
    drop(held);
    retry.await.expect("join").expect("the retry is admitted");
    search
        .await
        .expect("join")
        .expect("the search takes the last window slot");
    assert_eq!(state.facts().code_search_ms.len(), 2);
}

/// A client-side refusal reports only what the client knows: the wait.
/// GitHub's counters were not consulted, so none are claimed.
#[tokio::test]
async fn a_client_refusal_claims_no_provider_counters() {
    let (budget, state) = full_code_search_window();
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(1),
            Instant::now() + Duration::from_secs(5),
            &CancellationToken::new(),
        )
        .await
        .err()
        .expect("window full");
    let rate = error.rate_limit.expect("rate-limit metadata");
    assert_eq!(rate.remaining, None);
    assert_eq!(rate.reset_epoch_seconds, None);
    assert!(rate.retry_after_seconds.is_some_and(|secs| secs > 0));
    assert_eq!(rate.resource.as_deref(), Some("code_search"));
}
