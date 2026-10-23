//! Executor budget: limiter keys, reset waits and primary buckets; each
//! admission behavior is a child module.
use super::*;

mod gates;
mod queueing;
mod shared_state;
mod spacing;
mod window;

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
