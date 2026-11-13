//! The OAuth and git gates: permits, cancellation and a shared cooldown.
use super::*;

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
    use octocode_contract_types::ClientWaitKind;
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
            .acquire_blocking(&key, Duration::from_secs(1), deadline(), &|| false)
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
                .acquire_blocking(&key, Duration::from_secs(1), deadline(), &|| false)
                .expect("a free permit")
        })
        .collect::<Vec<_>>();
    let refused = row();
    let error = GitLimiter::new(Arc::clone(&budget), None)
        .for_row(&refused)
        .acquire_blocking(
            &key,
            Duration::from_secs(1),
            Instant::now() + Duration::from_millis(250),
            &|| false,
        )
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
        .acquire_blocking(&key, Duration::from_secs(1), deadline, &|| false)
        .expect("free git slot");
    drop(permit);
    state.cool_down(now_ms() + 60_000);
    let error = GitLimiter::new(GitHubBudget::relaxed(), Some(dir.path().to_owned()))
        .acquire_blocking(&key, Duration::from_secs(1), deadline, &|| false)
        .expect_err("cooldown");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
}
