/// Account-key churn (many short-lived tokens) does not grow the
/// registry for the process lifetime: idle keys past the retention cap
/// are dropped, while a key that is still throttled keeps its state.
#[test]
fn idle_keys_are_dropped_past_the_cap_but_throttled_ones_stay() {
    let budget = super::GitHubBudget::with_config(super::ExecutorConfig::octokit());
    let throttled = budget.key_state(&super::LimiterKey::new("github.com", Some("hot")), None);
    throttled.cool_down(super::now_ms() + 60_000);
    drop(throttled);
    let held = budget.key_state(&super::LimiterKey::new("github.com", Some("held")), None);
    for n in 0..(super::KEY_RETENTION * 3) {
        budget.key_state(
            &super::LimiterKey::new("github.com", Some(&format!("token-{n}"))),
            None,
        );
    }
    let keys = budget.keys.lock().unwrap();
    assert!(
        keys.len() <= super::KEY_RETENTION + 1,
        "{} keys",
        keys.len()
    );
    assert!(keys.contains_key(&super::LimiterKey::new("github.com", Some("hot"))));
    assert!(keys.contains_key(&super::LimiterKey::new("github.com", Some("held"))));
    drop(held);
}

#[test]
fn reset_wait_is_the_reset_plus_grace_from_now() {
    let config = super::ExecutorConfig::octokit();
    let now = super::now_ms() / 1000;
    assert_eq!(config.reset_wait(now.saturating_sub(10)), Duration::ZERO);
    let wait = config.reset_wait(now + 10);
    assert!(
        wait > Duration::from_secs(9) && wait <= Duration::from_secs(11),
        "{wait:?}"
    );
    assert_eq!(config.reset_until_ms(5), 6_000);
}
use super::*;

#[test]
fn limiter_keys_fold_api_host_and_hash_tokens() {
    let api = LimiterKey::for_url(
        &Url::parse("https://api.github.com/").expect("url"),
        Some("t1"),
    );
    let web = LimiterKey::for_url(
        &Url::parse("https://github.com/a/b.git").expect("url"),
        Some("t1"),
    );
    assert_eq!(api, web);
    assert_ne!(api, LimiterKey::new("github.com", Some("t2")));
    assert_eq!(LimiterKey::new("github.com", None).token, "anon");
    assert!(!api.file_name().contains("t1"));
    let ported = LimiterKey::for_url(
        &Url::parse("http://127.0.0.1:8080/api/v3").expect("url"),
        None,
    );
    assert_eq!(ported.file_name(), "127.0.0.1_8080-anon.json");
}

#[test]
fn primary_bucket_blocks_until_reset_and_isolates_keys() {
    let budget = GitHubBudget::relaxed();
    let one = budget.key_state(&LimiterKey::new("github.com", Some("one")), None);
    let two = budget.key_state(&LimiterKey::new("github.com", Some("two")), None);
    let reset = now_ms() / 1000 + 120;
    one.exhaust("core", reset);
    let block = one
        .blocked("core", budget.config())
        .expect("exhausted bucket blocks");
    assert_eq!(block.reset, Some(reset));
    assert!(one.blocked("search", budget.config()).is_none());
    assert!(two.blocked("core", budget.config()).is_none());
    let error = block.error(now_ms());
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    let rate = error.rate_limit.expect("metadata");
    assert_eq!(rate.resource.as_deref(), Some("core"));
    assert!(rate.retry_after_seconds.unwrap_or_default() >= 119);
}

/// A budget under `config` and its state for one host key.
fn limited(config: ExecutorConfig) -> (Arc<GitHubBudget>, Arc<KeyState>) {
    let budget = Arc::new(GitHubBudget::with_config(config));
    let state = budget.key_state(&LimiterKey::new("h", None), None);
    (budget, state)
}

/// A spacing rejection names the group's own bucket, not the code-search
/// window.
#[tokio::test]
async fn spacing_rejection_names_its_own_resource() {
    let (budget, state) = limited(ExecutorConfig {
        graphql_spacing: Duration::from_secs(5),
        ..ExecutorConfig::relaxed()
    });
    let token = CancellationToken::new();
    let admit = |deadline| {
        state.admit(
            Some(Group::Graphql),
            budget.config(),
            false,
            Duration::from_secs(10),
            deadline,
            &token,
        )
    };
    drop(
        admit(Instant::now() + Duration::from_secs(30))
            .await
            .expect("first start"),
    );
    let error = admit(Instant::now() + Duration::from_millis(100))
        .await
        .err()
        .expect("spacing exceeds the deadline");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.rate_limit.and_then(|rate| rate.resource).as_deref(),
        Some("graphql")
    );
    assert!(!error.message.contains("code search"), "{}", error.message);
}

#[tokio::test]
async fn search_spacing_orders_starts() {
    let (budget, state) = limited(ExecutorConfig {
        search_spacing: Duration::from_millis(150),
        ..ExecutorConfig::relaxed()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let token = CancellationToken::new();
    let started = Instant::now();
    for _ in 0..3 {
        let _admission = state
            .admit(
                Some(Group::Search),
                budget.config(),
                false,
                Duration::from_secs(10),
                deadline,
                &token,
            )
            .await
            .expect("admitted");
    }
    assert!(started.elapsed() >= Duration::from_millis(290));
}

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

/// Two processes sharing the state directory reserve under its lock: the
/// second sees the first's window slot before it reserves, so the
/// window never admits more than its limit.
#[tokio::test]
async fn processes_reserve_window_slots_against_each_other() {
    let dir = tempfile::tempdir().expect("state directory");
    let config = ExecutorConfig {
        code_search_per_minute: 1,
        ..ExecutorConfig::relaxed()
    };
    let key = LimiterKey::new("h", None);
    let first = GitHubBudget::with_config(config.clone());
    let second = GitHubBudget::with_config(config);
    let (one, two) = (
        first.key_state(&key, Some(dir.path())),
        second.key_state(&key, Some(dir.path())),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let token = CancellationToken::new();
    let taken = one
        .admit(
            Some(Group::CodeSearch),
            first.config(),
            true,
            Duration::from_secs(1),
            deadline,
            &token,
        )
        .await;
    assert!(taken.is_ok(), "{:?}", taken.err());
    let error = two
        .admit(
            Some(Group::CodeSearch),
            second.config(),
            true,
            Duration::from_secs(1),
            deadline,
            &token,
        )
        .await
        .err()
        .expect("the other process holds the only slot");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
}

/// Seed a full 2/min window whose oldest slot frees in ~1.5 s.
fn full_code_search_window() -> (Arc<GitHubBudget>, Arc<KeyState>) {
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    }));
    let state = budget.key_state(&LimiterKey::new("h", None), None);
    {
        let now = now_ms();
        let mut facts = state.facts();
        facts.code_search_ms.push_back(now - 58_500);
        facts.code_search_ms.push_back(now - 10_000);
    }
    (budget, state)
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

/// A peer process that holds the state file's lock must not hold this
/// request past its deadline or its cancellation, and the request goes
/// through as soon as the lock is released.
#[tokio::test]
async fn a_held_state_lock_cannot_block_admission_past_its_deadline() {
    let dir = tempfile::tempdir().expect("state directory");
    let config = || ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    };
    let key = LimiterKey::new("h", None);
    let budget = GitHubBudget::with_config(config());
    let state = budget.key_state(&key, Some(dir.path()));
    let mut sidecar = dir.path().join(key.file_name()).into_os_string();
    sidecar.push(".lock");
    let peer = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&sidecar)
        .expect("lock file");
    peer.lock().expect("peer takes the lock");

    let started = Instant::now();
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_millis(300),
            &CancellationToken::new(),
        )
        .await
        .err()
        .expect("the held lock times the request out");
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "waited {:?}",
        started.elapsed()
    );

    let token = CancellationToken::new();
    let canceller = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        canceller.cancel();
    });
    let started = Instant::now();
    let error = state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_secs(60),
            &token,
        )
        .await
        .err()
        .expect("cancellation ends the lock wait");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "waited {:?}",
        started.elapsed()
    );

    peer.unlock().expect("peer releases the lock");
    state
        .admit(
            Some(Group::CodeSearch),
            budget.config(),
            true,
            Duration::from_secs(2),
            Instant::now() + Duration::from_secs(10),
            &CancellationToken::new(),
        )
        .await
        .expect("admitted once the lock is free");
}

/// The reservation is mirrored to disk before the wait, so the release
/// must reach disk too, or other processes (and this one, on its next
/// refresh) still count the phantom slot.
#[tokio::test]
async fn a_cancelled_window_wait_releases_its_persisted_slot() {
    let dir = tempfile::tempdir().expect("state directory");
    let config = || ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    };
    let key = LimiterKey::new("h", None);
    let budget = Arc::new(GitHubBudget::with_config(config()));
    let state = budget.key_state(&key, Some(dir.path()));
    {
        let now = now_ms();
        let mut facts = state.facts();
        facts.code_search_ms.push_back(now - 58_500);
        facts.code_search_ms.push_back(now - 10_000);
    }
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
    let other_process = GitHubBudget::with_config(config());
    let seen = other_process.key_state(&key, Some(dir.path()));
    let facts = seen.facts();
    assert_eq!(facts.code_search_ms.len(), 2, "no phantom slot on disk");
    assert!(
        facts
            .last_start_ms
            .get(Group::CodeSearch.name())
            .is_none_or(|start| *start <= now_ms()),
        "no phantom spacing reservation on disk"
    );
}

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

/// A batch refused by spacing alone names the spacing, not the window.
#[test]
fn a_batch_refused_by_spacing_names_the_spacing() {
    let (budget, state) = limited(ExecutorConfig {
        search_spacing: Duration::from_secs(2),
        ..ExecutorConfig::relaxed()
    });
    let refusal = state
        .code_search_batch_refusal(
            3,
            budget.config(),
            Duration::from_secs(1),
            Instant::now() + Duration::from_secs(30),
        )
        .expect("three starts 2 s apart exceed the cap");
    assert!(
        refusal
            .message
            .contains("spacing cannot fit this row's 3 searches"),
        "{}",
        refusal.message
    );
    assert!(!refusal.message.contains("window"), "{}", refusal.message);
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

#[tokio::test]
async fn oauth_admission_holds_permits_and_honors_cancellation_and_shared_cooldown() {
    let budget = GitHubBudget::with_config(ExecutorConfig {
        global_concurrency: 1,
        ..ExecutorConfig::relaxed()
    });
    let origin = Url::parse("https://api.github.com").expect("origin");
    let cap = Duration::from_secs(1);
    let deadline = Instant::now() + Duration::from_secs(5);
    let cancellation = CancellationToken::new();
    let permit = budget
        .admit_auth(&origin, cap, deadline, &cancellation)
        .await
        .expect("first admission");
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let error = budget
        .admit_auth(&origin, cap, deadline, &cancelled)
        .await
        .err()
        .expect("cancelled while waiting for a permit");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
    let error = budget
        .admit_auth(
            &origin,
            cap,
            Instant::now() + Duration::from_millis(20),
            &cancellation,
        )
        .await
        .err()
        .expect("permit stays held");
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    drop(permit);
    drop(
        budget
            .admit_auth(&origin, cap, deadline, &cancellation)
            .await
            .expect("permit released"),
    );
    budget
        .key_state(&LimiterKey::new("github.com", None), None)
        .cool_down(now_ms() + 60_000);
    let error = budget
        .admit_auth(&origin, cap, deadline, &cancellation)
        .await
        .err()
        .expect("shared anonymous cooldown");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
}

#[test]
fn git_gate_fails_fast_on_long_cooldown() {
    let budget = GitHubBudget::relaxed();
    let dir = tempfile::tempdir().expect("state directory");
    let key = LimiterKey::new("github.com", None);
    let state = budget.key_state(&key, Some(dir.path()));
    let deadline = Instant::now() + Duration::from_secs(5);
    let permit = budget
        .acquire_git_blocking(
            &key,
            Some(dir.path()),
            Duration::from_secs(1),
            deadline,
            &|| false,
        )
        .expect("free git slot");
    drop(permit);
    state.cool_down(now_ms() + 60_000);
    let error = GitHubBudget::relaxed()
        .acquire_git_blocking(
            &key,
            Some(dir.path()),
            Duration::from_secs(1),
            deadline,
            &|| false,
        )
        .expect_err("cooldown");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
}
