use super::*;

#[test]
fn provider_retry_after_preserves_declared_delays_without_a_day_cap() {
    for seconds in [2 * 24 * 60 * 60, u64::MAX] {
        let mut headers = HeaderMap::new();
        headers.insert(
            "retry-after",
            reqwest::header::HeaderValue::from_str(&seconds.to_string()).expect("header"),
        );
        assert_eq!(
            provider_retry_after(&headers),
            Some(Duration::from_secs(seconds))
        );
    }
}

/// A secondary limit always backs off: a declared `retry-after` under the
/// attempt's backoff (zero) waits a backoff that grows with the attempt,
/// a longer one is kept whole, and no `retry-after` waits the documented
/// default.
#[test]
fn a_secondary_wait_is_never_shorter_than_the_attempts_backoff() {
    let config = ExecutorConfig::relaxed();
    let retry = RetryPolicy {
        max_attempts: 4,
        base_delay: Duration::from_millis(100),
    };
    let declaring = |seconds: &str| {
        let mut headers = HeaderMap::new();
        headers.insert(
            "retry-after",
            reqwest::header::HeaderValue::from_str(seconds).expect("header"),
        );
        headers
    };
    for attempt in 0..3_u8 {
        let wait = secondary_wait(&declaring("0"), &config, &retry, attempt);
        assert!(wait > Duration::ZERO, "attempt {attempt}: no backoff");
        assert!(
            wait <= retry.base_delay * (1 << attempt),
            "attempt {attempt}: {wait:?} is past the attempt's backoff"
        );
    }
    assert_eq!(
        secondary_wait(&declaring("120"), &config, &retry, 2),
        Duration::from_secs(120)
    );
    assert_eq!(
        secondary_wait(&HeaderMap::new(), &config, &retry, 0),
        config.secondary_default
    );
}
