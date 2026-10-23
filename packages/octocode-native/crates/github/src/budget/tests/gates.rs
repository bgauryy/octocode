//! The OAuth and git gates: permits, cancellation and a shared cooldown.
use super::*;

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
