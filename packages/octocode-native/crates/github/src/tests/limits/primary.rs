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

/// A 403 primary rate limit on `resource` that resets at epoch second `reset`.
fn primary_limit(resource: &str, reset: u64) -> ResponseTemplate {
    ResponseTemplate::new(403)
        .insert_header("x-ratelimit-remaining", "0")
        .insert_header("x-ratelimit-reset", reset.to_string().as_str())
        .insert_header("x-ratelimit-resource", resource)
        .set_body_json(serde_json::json!({"message": "API rate limit exceeded"}))
}

/// A spent search quota (a minute window the search pacing plans around) is
/// waited out; a spent core quota is not (see
/// `a_spent_core_quota_is_disclosed_at_once_however_far_the_deadline`).
#[tokio::test]
async fn primary_limit_waits_for_a_short_reset_then_retries() {
    let server = MockServer::builder().start().await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let reset = epoch_secs() + 1;
    Mock::given(method("GET"))
        .respond_with(Sequence::new(vec![
            primary_limit("search", reset),
            ok_json(),
        ]))
        .expect(2)
        .mount(&server)
        .await;
    let page = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &with_token("t", Duration::from_secs(5), 1024),
        )
        .await
        .expect("retried after reset");
    assert_eq!(page.status, 200);
    assert!(epoch_secs() >= reset, "retried before the reset");
}

/// A rate-limit wait that would end past the request deadline fails fast as
/// `rateLimited` instead of starting: the reset is two whole seconds away and
/// the request has 0.5 s.
#[tokio::test]
async fn a_rate_limit_wait_past_the_request_deadline_fails_fast() {
    let server = MockServer::builder().start().await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let reset = epoch_secs() + 2;
    Mock::given(method("GET"))
        .respond_with(Sequence::new(vec![
            primary_limit("search", reset),
            ok_json(),
        ]))
        .mount(&server)
        .await;
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &with_token("t", Duration::from_millis(500), 1024),
        )
        .await
        .expect_err("the wait outlasts the request");
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

/// D1: a spent core quota is disclosed at once, however far the call's own
/// deadline lies (no caller budget sets it 24 h out). GitHub meters core
/// over an hour, so waiting for its reset would hold the call for up to an
/// hour; the caller gets `rateLimited` with the reset instead and decides.
/// A known spent bucket refuses the next core read the same way, unsent.
/// Should the call wait anyway, the host hears it and the listener cancels
/// it, so the old behavior fails on the error kind rather than hanging.
#[tokio::test]
async fn a_spent_core_quota_is_disclosed_at_once_however_far_the_deadline() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("GET"))
        .respond_with(primary_limit("core", reset))
        .expect(1)
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let mut context = with_token("spent-far", Duration::from_secs(5), 1024);
    context.budget.deadline = std::time::Instant::now() + Duration::from_secs(24 * 60 * 60);
    let (scope, heard) = cancel_on_announced_wait(context.budget.cancellation.clone());
    for path in [["repos", "a", "b"], ["repos", "a", "c"]] {
        let error = scope
            .run(transport.execute(
                RequestSpec::get(endpoint.rest(&path).expect("route")),
                &context,
            ))
            .await
            .expect_err("rate limited");
        assert_eq!(
            error.kind,
            ProviderErrorKind::RateLimited,
            "{path:?}: {error:?}"
        );
        let rate = error.rate_limit.expect("rate limit");
        assert_eq!(rate.reset_epoch_seconds, Some(reset), "{path:?}");
        assert_eq!(rate.resource.as_deref(), Some("core"), "{path:?}");
        assert!(
            rate.retry_after_seconds.unwrap_or_default() > 3000,
            "{path:?}: {rate:?}"
        );
    }
    assert!(
        heard.lock().expect("lines").is_empty(),
        "no wait was announced: {:?}",
        heard.lock().expect("lines")
    );
    assert!(
        context.client_waits().is_empty(),
        "{:?}",
        context.client_waits()
    );
}
