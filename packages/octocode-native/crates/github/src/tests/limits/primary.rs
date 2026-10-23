//! Primary rate limits: waits for a short reset, fail-fast past the cap, and limit metadata.
use super::*;

#[tokio::test]
async fn retries_secondary_rate_limit_without_remaining_zero() {
    let server = MockServer::builder().start().await;
    let limited = Sequence::new(vec![secondary_limit(Some("0.1")), ok_json()]);
    let hits = limited.arrivals.clone();
    Mock::given(method("GET"))
        .respond_with(limited)
        .expect(2)
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let transport = GitHubTransport::new(
        endpoint.clone(),
        RetryPolicy {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_retry_after: Duration::from_secs(5),
            max_rate_limit_wait: Duration::from_secs(5),
        },
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    let page = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(5), 1024),
        )
        .await
        .expect("retried");
    assert_eq!(page.status, 200);
    assert_eq!(hits.count(), 2);
}

#[tokio::test]
async fn projects_rate_limit_metadata_without_retrying_long_delays() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "60")
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1900000000")
                .set_body_json(serde_json::json!({"message":"rate limited"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let transport = anonymous_transport(&endpoint);
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["rate"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(2), 1024),
        )
        .await
        .expect_err("rate limit");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.rate_limit,
        Some(Box::new(RateLimit {
            remaining: Some(0),
            reset_epoch_seconds: Some(1_900_000_000),
            retry_after_seconds: Some(60),
            resource: Some("core".into()),
        }))
    );
}

/// A 403 primary (core) rate limit that resets at epoch second `reset`.
fn primary_limit(reset: u64) -> ResponseTemplate {
    ResponseTemplate::new(403)
        .insert_header("x-ratelimit-remaining", "0")
        .insert_header("x-ratelimit-reset", reset.to_string().as_str())
        .insert_header("x-ratelimit-resource", "core")
        .set_body_json(serde_json::json!({"message": "API rate limit exceeded"}))
}

#[tokio::test]
async fn primary_limit_waits_for_a_short_reset_then_retries() {
    let server = MockServer::builder().start().await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let reset = epoch_secs() + 1;
    Mock::given(method("GET"))
        .respond_with(Sequence::new(vec![primary_limit(reset), ok_json()]))
        .expect(2)
        .mount(&server)
        .await;
    let page = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &with_token("t", Duration::from_secs(5), 1024),
        )
        .await
        .expect("retried after reset");
    assert_eq!(page.status, 200);
    assert!(epoch_secs() >= reset, "retried before the reset");
}

/// A rate-limit wait runs on the call's time, not the request's: a 0.5 s
/// request budget survives a 1–2 s reset wait, and the retried request still
/// gets its own time. Without a call deadline past the request's, the same
/// wait fails fast. The reset is two whole seconds away, so the wait (at
/// least 1 s less the mount time) outlasts the 0.5 s budget whenever the 403
/// arrives inside it; the reset is set once the transport is built, which
/// alone can take seconds on a loaded machine (the HTTP client loads the
/// system's root certificates).
#[tokio::test]
async fn rate_limit_waits_do_not_spend_the_request_timeout() {
    let server = MockServer::builder().start().await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let reset = epoch_secs() + 2;
    Mock::given(method("GET"))
        .respond_with(Sequence::new(vec![primary_limit(reset), ok_json()]))
        .mount(&server)
        .await;
    let mut context = with_token("t", Duration::from_millis(500), 1024);
    context.budget.wait_until = std::time::Instant::now() + Duration::from_secs(10);
    let page = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &context,
        )
        .await
        .expect("waited for the reset on the call's time");
    assert_eq!(page.status, 200);
    assert!(context.budget.effective_deadline() > context.budget.deadline);

    let other = MockServer::builder().start().await;
    let (transport, endpoint) = executor_transport(&other, GitHubBudget::relaxed(), short_retry());
    let reset = epoch_secs() + 2;
    Mock::given(method("GET"))
        .respond_with(Sequence::new(vec![primary_limit(reset), ok_json()]))
        .mount(&other)
        .await;
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &with_token("t", Duration::from_millis(500), 1024),
        )
        .await
        .expect_err("no call time past the request's");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
}

#[tokio::test]
async fn primary_limit_fails_fast_and_blocks_later_sends_until_reset() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(serde_json::json!({"message": "API rate limit exceeded"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = with_token("t", Duration::from_secs(5), 1024);
    let first = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &context,
        )
        .await
        .expect_err("primary limit");
    assert_eq!(first.kind, ProviderErrorKind::RateLimited);
    let rate = first.rate_limit.expect("metadata");
    assert_eq!(rate.remaining, Some(0));
    assert_eq!(rate.reset_epoch_seconds, Some(reset));
    assert_eq!(rate.resource.as_deref(), Some("core"));
    assert!(rate.retry_after_seconds.unwrap_or_default() > 3000);
    // The exhausted bucket is known: the next core call is not sent at all.
    let second = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "c"]).expect("route")),
            &context,
        )
        .await
        .expect_err("blocked before send");
    assert_eq!(second.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        second.rate_limit.and_then(|r| r.reset_epoch_seconds),
        Some(reset)
    );
}

/// Rate-limit bodies without `x-ratelimit-*` headers (proxies, GHES).
#[tokio::test]
async fn a_headerless_rate_limit_body_is_a_primary_limit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403).set_body_json(
            serde_json::json!({"message": "API rate limit exceeded for user ID 1."}),
        ))
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("limited");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert!(error.retryable);
    // No headers: a primary core limit with GitHub's one-minute fallback wait.
    let rate = error.rate_limit.expect("rate limit");
    assert_eq!(rate.remaining, Some(0));
    assert_eq!(rate.resource.as_deref(), Some("core"));
    assert_eq!(rate.retry_after_seconds, Some(60));
    let reset = rate.reset_epoch_seconds.expect("reset");
    assert!(
        (epoch_secs() + 55..=epoch_secs() + 61).contains(&reset),
        "reset {reset} is not about a minute out"
    );
}
