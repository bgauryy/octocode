//! Primary and secondary rate limits, spacing, the circuit breaker and the code-search window.
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

#[tokio::test]
async fn secondary_limit_without_retry_after_waits_sixty_seconds_or_fails_fast() {
    let server = MockServer::builder().start().await;
    let limited = Sequence::new(vec![secondary_limit(None), ok_json()]);
    let hits = limited.arrivals.clone();
    Mock::given(method("GET"))
        .respond_with(limited)
        .expect(1)
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1024);
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &context,
        )
        .await
        .expect_err("60s fallback exceeds the 5s cap");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert!(error.retryable);
    assert_eq!(error.status, Some(403));
    // A 403 secondary limit keeps its rate-limit metadata.
    let rate = error.rate_limit.expect("secondary metadata");
    assert_eq!(rate.retry_after_seconds, Some(60));
    assert_eq!(rate.remaining, Some(21));
    // The cooldown is shared by the key: a core call is not sent either.
    let blocked = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &context,
        )
        .await
        .expect_err("cooldown");
    assert_eq!(blocked.kind, ProviderErrorKind::RateLimited);
    assert_eq!(hits.count(), 1);
}

#[tokio::test]
async fn permits_are_released_while_backing_off() {
    let server = MockServer::builder().start().await;
    let flaky = Sequence::new(vec![
        ResponseTemplate::new(503).insert_header("retry-after", "1"),
        ResponseTemplate::new(200).set_body_bytes(b"ok"),
    ]);
    let hits = flaky.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/flaky"))
        .respond_with(flaky)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/fast"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"fast"))
        .mount(&server)
        .await;
    // One global slot per key: a request sleeping on retry-after must not
    // hold it.
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        global_concurrency: 1,
        ..ExecutorConfig::relaxed()
    }));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let slow = {
        let transport = transport.clone();
        let url = endpoint.rest(&["flaky"]).expect("route");
        tokio::spawn(async move {
            transport
                .execute(
                    RequestSpec::get(url),
                    &with_token("t", Duration::from_secs(5), 1024),
                )
                .await
        })
    };
    while hits.count() == 0 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    let started = std::time::Instant::now();
    let fast = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["fast"]).expect("route")),
            &with_token("t", Duration::from_secs(5), 1024),
        )
        .await
        .expect("fast");
    assert_eq!(fast.body, b"fast".as_slice());
    assert!(
        started.elapsed() < Duration::from_millis(700),
        "fast request waited for the backing-off request's permit"
    );
    let slow = slow.await.expect("join").expect("slow retried");
    assert_eq!(slow.body, b"ok".as_slice());
    assert_eq!(hits.count(), 2);
}

#[tokio::test]
async fn search_requests_are_spaced_per_key() {
    let server = MockServer::builder().start().await;
    let search = Sequence::new(vec![
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": []})),
    ]);
    let arrivals = search.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(search)
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        search_spacing: Duration::from_millis(150),
        ..ExecutorConfig::relaxed()
    }));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let calls = (0..3).map(|_| {
        let transport = transport.clone();
        let url = endpoint.rest(&["search", "issues"]).expect("route");
        async move {
            transport
                .execute(
                    RequestSpec::get(url),
                    &with_token("t", Duration::from_secs(5), 1024),
                )
                .await
                .expect("search")
        }
    });
    futures_util::future::join_all(calls).await;
    let arrivals = arrivals.times();
    assert_eq!(arrivals.len(), 3);
    for pair in arrivals.windows(2) {
        assert!(
            pair[1].duration_since(pair[0]) >= Duration::from_millis(120),
            "search starts not spaced: {arrivals:?}"
        );
    }
}

#[tokio::test]
async fn limiter_keys_isolate_tokens() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("GET"))
        .and(header("authorization", "Bearer exhausted"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .set_body_json(serde_json::json!({"message": "API rate limit exceeded"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"ok"))
        .expect(2)
        .mount(&server)
        .await;
    let budget = GitHubBudget::relaxed();
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let exhausted = || with_token("exhausted", Duration::from_secs(5), 1024);
    let fresh = || with_token("fresh", Duration::from_secs(5), 1024);
    let url = endpoint.rest(&["repos", "a", "b"]).expect("route");
    transport
        .execute(RequestSpec::get(url.clone()), &exhausted())
        .await
        .expect_err("exhausted");
    transport
        .execute(RequestSpec::get(url.clone()), &exhausted())
        .await
        .expect_err("still exhausted, not sent");
    for _ in 0..2 {
        transport
            .execute(RequestSpec::get(url.clone()), &fresh())
            .await
            .expect("other token unaffected");
    }
}

// ── Throttling fixes (docs: .octocode/octocode-roast/2026-10-10-github-throttling.md) ──

fn secondary_limit_response(retry_after: Option<&str>) -> ResponseTemplate {
    let mut response = ResponseTemplate::new(403)
        .insert_header("x-ratelimit-remaining", "4000")
        .insert_header("x-ratelimit-reset", (epoch_secs() + 3600).to_string().as_str())
        .insert_header("x-ratelimit-resource", "search")
        .set_body_json(serde_json::json!({
            "message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."
        }));
    if let Some(retry_after) = retry_after {
        response = response.insert_header("retry-after", retry_after);
    }
    response
}

/// A row queued behind the search spacing must re-check the block
/// that landed while it slept, not send into a 120 s secondary cooldown.
#[tokio::test]
async fn a_queued_request_is_not_sent_after_a_cooldown_lands_while_it_waits() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(secondary_limit_response(Some("120")))
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        search_spacing: Duration::from_millis(300),
        ..ExecutorConfig::relaxed()
    }));
    let (transport, endpoint) = executor_transport(&server, budget, short_retry());
    let rows = futures_util::future::join_all((0..2).map(|_| {
        let transport = transport.clone();
        let endpoint = endpoint.clone();
        async move { get_status(&transport, &endpoint, &["search", "issues"]).await }
    }))
    .await;
    for row in &rows {
        let error = row.as_ref().expect_err("cooldown");
        assert_eq!(error.kind, ProviderErrorKind::RateLimited);
        assert_eq!(
            error
                .rate_limit
                .as_ref()
                .and_then(|r| r.retry_after_seconds)
                .map(|s| s >= 100),
            Some(true)
        );
    }
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        1,
        "the queued row was sent into the cooldown"
    );
}

/// Answers every GET with 502; the returned log counts the requests.
async fn mount_bad_gateway(server: &MockServer) -> Arrivals {
    let bad_gateway = Sequence::new(vec![
        ResponseTemplate::new(502).set_body_json(serde_json::json!({"message": "Server Error"})),
    ]);
    let hits = bad_gateway.arrivals.clone();
    Mock::given(method("GET"))
        .respond_with(bad_gateway)
        .mount(server)
        .await;
    hits
}

/// An outage trips the breaker, and the open breaker names the real
/// cause (a server failure) rather than a rate limit.
#[tokio::test]
async fn an_open_circuit_reports_a_server_outage_not_a_rate_limit() {
    let server = MockServer::builder().start().await;
    let hits = mount_bad_gateway(&server).await;
    let (transport, endpoint) = executor_transport(&server, breaker_budget(2), short_retry());
    for _ in 0..2 {
        let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
            .await
            .expect_err("502");
        assert_eq!(error.kind, ProviderErrorKind::Server);
    }
    let sent = hits.count();
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("circuit open");
    assert_eq!(hits.count(), sent, "an open circuit must not send");
    assert_eq!(error.kind, ProviderErrorKind::Server, "{error:?}");
    assert!(error.retryable);
    assert!(error.message.contains("unavailable"), "{}", error.message);
    assert!(
        error
            .rate_limit
            .as_ref()
            .and_then(|r| r.retry_after_seconds)
            .is_some(),
        "the wait is still reported"
    );
}

/// Retries of one logical request are one failure, not one each.
#[tokio::test]
async fn the_circuit_counts_one_failure_per_logical_request() {
    let server = MockServer::builder().start().await;
    let hits = mount_bad_gateway(&server).await;
    // 3 attempts per request: per-attempt counting opens at the first
    // request's third attempt; per-request counting needs 3 requests.
    let (transport, endpoint) = executor_transport(&server, breaker_budget(3), short_retry());
    for _ in 0..2 {
        let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
            .await
            .expect_err("502");
        assert_eq!(error.kind, ProviderErrorKind::Server, "{error:?}");
    }
    assert_eq!(hits.count(), 6, "both requests ran all their attempts");
}

/// A success closes the breaker's count.
#[tokio::test]
async fn a_success_resets_the_circuit_count() {
    let server = MockServer::builder().start().await;
    // 502 for requests 1 and 3 (single attempts), 200 for the rest.
    let ok = || ResponseTemplate::new(200).set_body_bytes(b"ok");
    Mock::given(method("GET"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(502),
            ok(),
            ResponseTemplate::new(502),
            ok(),
        ]))
        .mount(&server)
        .await;
    let single = RetryPolicy {
        max_attempts: 1,
        ..short_retry()
    };
    let (transport, endpoint) = executor_transport(&server, breaker_budget(2), single);
    for expected in [false, true, false, true, true] {
        let result = get_status(&transport, &endpoint, &["repos", "a", "b"]).await;
        assert_eq!(result.is_ok(), expected, "{result:?}");
    }
}

/// Transport outages carry their own cause too.
#[tokio::test]
async fn an_open_circuit_after_connection_failures_reports_transport() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    let endpoint = GitHubEndpoint::new(
        url::Url::parse(&format!("http://127.0.0.1:{port}/api/v3")).expect("URL"),
    )
    .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        RetryPolicy {
            max_attempts: 1,
            ..short_retry()
        },
        breaker_budget(1),
    )
    .expect("transport");
    let first = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("refused");
    assert_eq!(first.kind, ProviderErrorKind::Transport);
    let second = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("circuit open");
    assert_eq!(second.kind, ProviderErrorKind::Transport, "{second:?}");
    assert!(second.retryable);
    assert!(second.message.contains("unavailable"), "{}", second.message);
}

/// Repeated secondary limits still report a rate limit.
#[tokio::test]
async fn an_open_circuit_after_secondary_limits_reports_a_rate_limit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(secondary_limit_response(Some("1")))
        .mount(&server)
        .await;
    let single = RetryPolicy {
        max_attempts: 1,
        ..short_retry()
    };
    let (transport, endpoint) = executor_transport(&server, breaker_budget(1), single);
    let first = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("secondary");
    assert_eq!(first.kind, ProviderErrorKind::RateLimited);
    let second = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("blocked");
    assert_eq!(second.kind, ProviderErrorKind::RateLimited);
}

/// A secondary limit's `resetEpochSeconds` is the cooldown end, not
/// the unrelated primary window reset an hour away.
#[tokio::test]
async fn secondary_limit_reset_is_the_cooldown_end() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(secondary_limit_response(Some("120")))
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let before = epoch_secs();
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("secondary");
    let rate = error.rate_limit.expect("rate limit");
    assert_eq!(rate.retry_after_seconds, Some(120));
    let reset = rate.reset_epoch_seconds.expect("reset");
    assert!(
        (before + 119..=before + 123).contains(&reset),
        "reset {reset} is not the cooldown end near {}",
        before + 120
    );
}

/// A 5xx `Retry-After` beyond the retry cap is surfaced, not dropped.
#[tokio::test]
async fn a_long_server_retry_after_is_surfaced() {
    let server = MockServer::builder().start().await;
    let unavailable = Sequence::new(vec![
        ResponseTemplate::new(503).insert_header("retry-after", "30"),
    ]);
    let hits = unavailable.arrivals.clone();
    Mock::given(method("GET"))
        .respond_with(unavailable)
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("503");
    assert_eq!(error.kind, ProviderErrorKind::Server);
    assert_eq!(hits.count(), 1, "a long retry-after fails fast");
    assert_eq!(
        error.rate_limit.and_then(|rate| rate.retry_after_seconds),
        Some(30)
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

/// The legacy abuse-detection 403 is a secondary limit.
#[tokio::test]
async fn an_abuse_detection_403_is_a_secondary_limit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "message": "You have triggered an abuse detection mechanism. Please wait a few minutes before you try again."
        })))
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("limited");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.message.as_ref(),
        "GitHub secondary rate limit exceeded"
    );
}

/// An HTTP-date `Retry-After` is honored instead of the 60 s default. The
/// date is whole seconds from now, so it is far enough ahead that a loaded
/// machine delaying the response by seconds still reads it, not 0 or 60;
/// `octocode_io::retry::tests` pins the exact arithmetic.
#[tokio::test]
async fn an_http_date_retry_after_is_honored() {
    let server = MockServer::builder().start().await;
    let date = httpdate_after(30);
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", date.as_str())
                .set_body_json(serde_json::json!({"message": "Too Many Requests"})),
        )
        .mount(&server)
        .await;
    let single = RetryPolicy {
        max_attempts: 1,
        ..short_retry()
    };
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), single);
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("limited");
    let seconds = error
        .rate_limit
        .and_then(|rate| rate.retry_after_seconds)
        .expect("retry after");
    assert!((20..=31).contains(&seconds), "{seconds} is not ~30 s");
}

/// An RFC 9110 IMF-fixdate `seconds` from now.
fn httpdate_after(seconds: u64) -> String {
    let at = epoch_secs() + seconds;
    let days = (at / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let rem = at % 86_400;
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(days % 7) as usize];
    let month_name = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(month - 1) as usize];
    format!(
        "{weekday}, {day:02} {month_name} {year} {:02}:{:02}:{:02} GMT",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[tokio::test]
async fn code_search_room_counts_the_searches_the_window_has_left() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"total_count":0,"incomplete_results":false,"items":[]}),
        ))
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        code_search_per_minute: 3,
        ..ExecutorConfig::relaxed()
    }));
    let (transport, _) = executor_transport(&server, budget, short_retry());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    assert_eq!(transport.code_search_room(&context), 3);
    for keyword in ["one", "two"] {
        transport
            .search_code(
                &CodeSearchRequest {
                    query: keyword.into(),
                    page: 1,
                    per_page: 5,
                    include_fragments: false,
                },
                &context,
            )
            .await
            .expect("search");
    }
    assert_eq!(transport.code_search_room(&context), 1);
    assert_eq!(
        transport.code_search_room(&with_token("fresh", Duration::from_secs(5), 1 << 20)),
        3,
        "another credential has its own window"
    );
}

/// Four transports with their own budgets share one state directory,
/// as four processes on one `OCTOCODE_HOME` do. Eight concurrent code
/// searches against a 2/min window send at most 2, and every refused search
/// returns well before its deadline instead of queueing toward it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn processes_sharing_a_state_dir_never_exceed_the_window_and_refuse_promptly() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(
                    serde_json::json!({"total_count":0,"incomplete_results":false,"items":[]}),
                )
                .set_delay(Duration::from_millis(200)),
        )
        .mount(&server)
        .await;
    let state = tempfile::tempdir().expect("state directory");
    let transports = (0..4)
        .map(|_| {
            let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
                code_search_per_minute: 2,
                ..ExecutorConfig::relaxed()
            }));
            let (mut transport, _) = executor_transport(
                &server,
                budget,
                RetryPolicy {
                    max_rate_limit_wait: Duration::from_secs(1),
                    ..short_retry()
                },
            );
            transport.set_rate_limit_state_dir(Some(state.path().to_path_buf()));
            Arc::new(transport)
        })
        .collect::<Vec<_>>();
    let mut searches = tokio::task::JoinSet::new();
    for (index, transport) in transports.iter().cycle().take(8).enumerate() {
        let transport = Arc::clone(transport);
        searches.spawn(async move {
            let started = std::time::Instant::now();
            let result = transport
                .search_code(
                    &CodeSearchRequest {
                        query: format!("q{index}"),
                        page: 1,
                        per_page: 5,
                        include_fragments: false,
                    },
                    &with_token("t", Duration::from_secs(10), 1 << 20),
                )
                .await;
            (result.map(|_| ()), started.elapsed())
        });
    }
    let mut sent = 0;
    while let Some(joined) = searches.join_next().await {
        let (result, elapsed) = joined.expect("search task");
        match result {
            Ok(()) => sent += 1,
            Err(error) => {
                assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{error:?}");
                assert!(
                    elapsed < Duration::from_secs(3),
                    "refused after {elapsed:?}: a refusal must not queue toward the deadline"
                );
            }
        }
    }
    let received = server.received_requests().await.expect("recorded").len();
    assert_eq!(received, sent);
    assert!(received <= 2, "{received} searches reached a 2/min window");
    assert!(sent >= 1);
}

/// A row of several code searches is refused before the first is
/// sent when they cannot all start within the cap; the refusal reserves no
/// slot and claims no provider counters. A search the cache answers needs no
/// slot, so callers can count only the searches they will send.
#[tokio::test]
async fn a_code_search_batch_that_cannot_fit_is_refused_before_any_send() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"total_count":0,"incomplete_results":false,"items":[]}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        code_search_per_minute: 2,
        ..ExecutorConfig::relaxed()
    }));
    let (mut transport, _) = executor_transport(&server, budget, short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    let request = |query: &str| CodeSearchRequest {
        query: query.into(),
        page: 1,
        per_page: 5,
        include_fragments: false,
    };
    transport
        .search_code(&request("one"), &context)
        .await
        .expect("search");
    assert_eq!(transport.code_search_room(&context), 1);

    assert!(transport.code_search_batch_refusal(1, &context).is_none());
    let refusal = transport
        .code_search_batch_refusal(2, &context)
        .expect("two searches cannot fit one slot within the cap");
    assert_eq!(refusal.kind, ProviderErrorKind::RateLimited);
    assert!(
        refusal.message.contains("2 searches"),
        "{}",
        refusal.message
    );
    let rate = refusal.rate_limit.expect("rate limit");
    assert_eq!(rate.remaining, None);
    assert_eq!(rate.resource.as_deref(), Some("code_search"));
    // The wait is what is left of the one-minute code-search window.
    assert!(
        matches!(rate.retry_after_seconds, Some(1..=60)),
        "{:?}",
        rate.retry_after_seconds
    );
    assert_eq!(
        transport.code_search_room(&context),
        1,
        "a refusal reserves nothing"
    );

    assert!(
        transport
            .cached_code_search(&request("one"), &context)
            .await
            .expect("lookup")
            .is_some(),
        "a cached search is visible without a send"
    );
    assert!(
        transport
            .cached_code_search(&request("two"), &context)
            .await
            .expect("lookup")
            .is_none()
    );
}
