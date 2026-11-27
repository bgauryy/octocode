//! The OAuth, API and git gates: permits, cancellation and a shared cooldown.
use super::*;

/// The secondary-limit waits `waits` recorded, in milliseconds.
fn secondary_ms(waits: &ClientWaits) -> i64 {
    waits
        .summed()
        .iter()
        .filter(|wait| wait.kind == octocode_contract_base::ClientWaitKind::SecondaryLimit)
        .map(|wait| wait.ms)
        .sum()
}

/// A secondary cooldown that outlasts the wait bound (an MCP host's reply
/// budget) but ends before the deadline is slept up to the bound, then
/// refused unsent with only the rest as `retryAfter`: the retry that refusal
/// hands back waits out the rest within its own bound and is admitted. An
/// agent cannot sleep, so a refusal that slept nothing hands back a retry
/// that is refused the same way. A cooldown past the deadline itself is
/// refused at once, sleeping nothing: the caller's deadline says it does not
/// listen that long.
#[tokio::test]
async fn a_cooldown_past_the_wait_bound_is_slept_to_the_bound_and_its_retry_admitted() {
    let (budget, state) = limited(ExecutorConfig::relaxed());
    let cooldown = Duration::from_millis(1500);
    let bound = cooldown / 3;
    state.cool_down(now_ms() + millis(cooldown));

    let first = ClientWaits::default();
    let started = Instant::now();
    let limit = WaitLimit {
        deadline: started + cooldown * 10,
        sleep_by: started + bound,
    };
    let error = state
        .wait_unblocked("core", budget.config(), limit, &Cancellation::new(), &first)
        .await
        .expect_err("the cooldown outlasts the bound");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{error:?}");
    let slept = secondary_ms(&first);
    assert!(
        slept.unsigned_abs() >= millis(bound) * 9 / 10,
        "slept {slept} ms"
    );
    let rest = error
        .rate_limit
        .and_then(|rate| rate.retry_after_seconds)
        .expect("the rest of the cooldown");
    assert!(rest < ceil_secs(cooldown), "retryAfter {rest}s");

    let retry = ClientWaits::default();
    let now = Instant::now();
    let limit = WaitLimit {
        deadline: now + cooldown * 10,
        sleep_by: now + cooldown,
    };
    state
        .wait_unblocked("core", budget.config(), limit, &Cancellation::new(), &retry)
        .await
        .expect("the rest fits the retry's bound");
    assert!(secondary_ms(&retry) > 0, "{:?}", retry.summed());

    state.cool_down(now_ms() + millis(cooldown));
    let refused = ClientWaits::default();
    let started = Instant::now();
    let error = state
        .wait_unblocked(
            "core",
            budget.config(),
            started + bound,
            &Cancellation::new(),
            &refused,
        )
        .await
        .expect_err("the cooldown ends past the deadline");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{error:?}");
    assert_eq!(secondary_ms(&refused), 0, "{:?}", refused.summed());
    assert!(started.elapsed() < bound, "{:?}", started.elapsed());
}

#[tokio::test]
async fn oauth_admission_holds_permits_and_honors_cancellation_and_shared_cooldown() {
    let budget = GitHubBudget::with_config(ExecutorConfig {
        global_concurrency: 1,
        ..ExecutorConfig::relaxed()
    });
    let origin = Url::parse("https://api.github.com").expect("origin");
    let deadline = Instant::now() + Duration::from_secs(5);
    let cancellation = Cancellation::new();
    let permit = budget
        .admit_auth(&origin, deadline, &cancellation)
        .await
        .expect("first admission");
    let cancelled = Cancellation::new();
    cancelled.cancel();
    let error = budget
        .admit_auth(&origin, deadline, &cancelled)
        .await
        .err()
        .expect("cancelled while waiting for a permit");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
    let error = budget
        .admit_auth(
            &origin,
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
            .admit_auth(&origin, deadline, &cancellation)
            .await
            .expect("permit released"),
    );
    budget
        .key_state(&LimiterKey::new("github.com", None), None)
        .cool_down(now_ms() + 60_000);
    let error = budget
        .admit_auth(&origin, deadline, &cancellation)
        .await
        .err()
        .expect("shared anonymous cooldown");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
}

/// B91 r2: a clone's wait out a secondary cooldown is `secondaryLimit`, its
/// queue behind the other clones' `git` permits is `spacing`, and a wait that
/// ends in a refusal is recorded too. Only the cooldown is disclosed: a
/// queue is routine spacing.
#[test]
fn git_admission_waits_land_on_the_row_by_cause() {
    use octocode_contract_base::ClientWaitKind;
    let ms = |context: &crate::RequestContext, kind: ClientWaitKind| {
        context
            .waits
            .summed()
            .iter()
            .filter(|wait| wait.kind == kind)
            .map(|wait| wait.ms)
            .sum::<i64>()
    };
    let row = || {
        crate::RequestContext::new(
            octocode_io::budget::RequestBudget::with_timeout(Duration::from_secs(5), 1024),
            None,
        )
    };
    let budget = GitHubBudget::relaxed();
    let key = LimiterKey::new("github.com", None);
    let state = budget.key_state(&key, None);
    let deadline = || Instant::now() + Duration::from_secs(5);

    state.cool_down(now_ms() + 300);
    let cooled = row();
    drop(
        GitLimiter::new(Arc::clone(&budget), None)
            .for_row(&cooled)
            .acquire_blocking(&key, deadline(), &|| false)
            .expect("after the cooldown"),
    );
    assert!(
        ms(&cooled, ClientWaitKind::SecondaryLimit) >= 200,
        "{:?}",
        cooled.client_waits()
    );

    let held = (0..budget.config().git_concurrency.max(1))
        .map(|_| {
            GitLimiter::new(Arc::clone(&budget), None)
                .acquire_blocking(&key, deadline(), &|| false)
                .expect("a free permit")
        })
        .collect::<Vec<_>>();
    let refused = row();
    let error = GitLimiter::new(Arc::clone(&budget), None)
        .for_row(&refused)
        .acquire_blocking(&key, Instant::now() + Duration::from_millis(250), &|| false)
        .expect_err("every permit stays held past the deadline");
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    assert!(
        ms(&refused, ClientWaitKind::Spacing) >= 200,
        "{:?}",
        refused.waits.summed()
    );
    assert_eq!(
        refused.client_waits(),
        Vec::new(),
        "spacing is not disclosed"
    );
    drop(held);
}

#[test]
fn git_gate_fails_fast_on_long_cooldown() {
    let budget = GitHubBudget::relaxed();
    let dir = tempfile::tempdir().expect("state directory");
    let key = LimiterKey::new("github.com", None);
    let state = budget.key_state(&key, Some(dir.path()));
    let deadline = Instant::now() + Duration::from_secs(5);
    let permit = GitLimiter::new(Arc::clone(&budget), Some(dir.path().to_owned()))
        .acquire_blocking(&key, deadline, &|| false)
        .expect("free git slot");
    drop(permit);
    state.cool_down(now_ms() + 60_000);
    let error = GitLimiter::new(GitHubBudget::relaxed(), Some(dir.path().to_owned()))
        .acquire_blocking(&key, deadline, &|| false)
        .expect_err("cooldown");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
}

/// A clone waits out a secondary cooldown of any length while it ends before
/// the clone's deadline (here until the caller cancels), as an API request
/// does; a cooldown that ends past the deadline fails at once with its
/// rate-limit facts, waiting nothing.
#[test]
fn git_admission_waits_out_a_long_cooldown_that_ends_before_the_deadline() {
    use octocode_contract_base::ClientWaitKind;
    let secondary = |context: &crate::RequestContext| {
        context
            .waits
            .summed()
            .iter()
            .filter(|wait| wait.kind == ClientWaitKind::SecondaryLimit)
            .map(|wait| wait.ms)
            .sum::<i64>()
    };
    let row = || {
        crate::RequestContext::new(
            octocode_io::budget::RequestBudget::with_timeout(Duration::from_secs(60), 1024),
            None,
        )
    };
    let budget = GitHubBudget::relaxed();
    let key = LimiterKey::new("github.com", None);
    budget.key_state(&key, None).cool_down(now_ms() + 30_000);

    let waiting = row();
    let started = Instant::now();
    let error = GitLimiter::new(Arc::clone(&budget), None)
        .for_row(&waiting)
        .acquire_blocking(&key, started + Duration::from_secs(60), &|| {
            started.elapsed() >= Duration::from_millis(300)
        })
        .expect_err("cancelled while it waits out the cooldown");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled, "{error:?}");
    assert!(secondary(&waiting) >= 200, "{:?}", waiting.waits.summed());

    let refused = row();
    let error = GitLimiter::new(Arc::clone(&budget), None)
        .for_row(&refused)
        .acquire_blocking(&key, Instant::now() + Duration::from_secs(20), &|| false)
        .expect_err("the cooldown ends past the deadline");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{error:?}");
    let rate = error.rate_limit.expect("the cooldown's facts");
    assert!(
        rate.retry_after_seconds.is_some_and(|seconds| seconds > 20),
        "{rate:?}"
    );
    assert_eq!(secondary(&refused), 0, "{:?}", refused.waits.summed());
}
