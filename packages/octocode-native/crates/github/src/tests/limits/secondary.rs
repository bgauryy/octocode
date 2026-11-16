//! Secondary limits and provider retry-after: cooldown waits, their reset, and queued requests.
use super::*;

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
        // The 403 reports 4000 searches left for an hour: 0.9 s apart.
        quota_pacing: true,
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

/// A 403 whose message names a secondary limit but whose headers
/// say the core quota is spent (`remaining: 0`, a reset, no `retry-after`)
/// is that quota's limit: GitHub's own rule is to wait until the reset, an
/// hour window here, so the spent quota is disclosed at once with the
/// reset. A secondary limit with `retry-after` or quota left stays a
/// cooldown (the tests above).
#[tokio::test]
async fn a_secondary_message_on_a_spent_core_quota_is_disclosed_at_once() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(serde_json::json!({
                    "message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let mut context = with_token("secondary-spent", Duration::from_secs(5), 1024);
    context.budget.deadline = std::time::Instant::now() + Duration::from_secs(24 * 60 * 60);
    let (scope, heard) = cancel_on_announced_wait(context.budget.cancellation.clone());
    let error = scope
        .run(transport.execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &context,
        ))
        .await
        .expect_err("rate limited");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{error:?}");
    let rate = error.rate_limit.expect("rate limit");
    assert_eq!(rate.reset_epoch_seconds, Some(reset), "{rate:?}");
    assert_eq!(rate.resource.as_deref(), Some("core"), "{rate:?}");
    assert!(
        heard.lock().expect("lines").is_empty(),
        "no wait was announced: {:?}",
        heard.lock().expect("lines")
    );
}

/// A search on `transport`'s key after a limit: `(sent, error)`. Its 2 s
/// budget is shorter than the documented 60 s secondary cooldown, so a
/// cooled key refuses it at once, unsent.
async fn search_after(
    server: &MockServer,
    transport: &GitHubTransport,
    endpoint: &GitHubEndpoint,
    token: &str,
) -> (usize, Option<ProviderError>) {
    let searched = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["search", "issues"]).expect("route")),
            &with_token(token, Duration::from_secs(2), 1024),
        )
        .await
        .err();
    let sent = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path().starts_with("/api/v3/search"))
        .count();
    (sent, searched)
}

/// A message naming a secondary limit
/// still starts the documented key-wide cooldown beside the spent bucket,
/// because requests sent during a secondary limit can get the integration
/// banned. The spent core quota is still disclosed at once; a search
/// on the same key is then refused unsent while the key cools down.
#[tokio::test]
async fn a_secondary_message_on_a_spent_bucket_still_cools_the_key() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(serde_json::json!({
                    "message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ok_json())
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let mut context = with_token("cooled-rest", Duration::from_secs(5), 1024);
    context.budget.deadline = std::time::Instant::now() + Duration::from_secs(24 * 60 * 60);
    let (scope, heard) = cancel_on_announced_wait(context.budget.cancellation.clone());
    let core = scope
        .run(transport.execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &context,
        ))
        .await
        .expect_err("rate limited");
    assert_eq!(core.kind, ProviderErrorKind::RateLimited, "{core:?}");
    assert_eq!(
        core.rate_limit.and_then(|limit| limit.reset_epoch_seconds),
        Some(reset)
    );
    assert!(
        heard.lock().expect("lines").is_empty(),
        "core is not waited out"
    );
    let (sent, searched) = search_after(&server, &transport, &endpoint, "cooled-rest").await;
    assert_eq!(sent, 0, "the key cools down: the search is not sent");
    let searched = searched.expect("refused");
    assert_eq!(
        searched.kind,
        ProviderErrorKind::RateLimited,
        "{searched:?}"
    );
    assert!(
        searched.message.contains("secondary"),
        "{}",
        searched.message
    );
}

/// For GraphQL, a secondary message on a spent GraphQL quota is
/// disclosed at once with its reset and cools the key as on REST.
#[tokio::test]
async fn a_graphql_secondary_message_on_a_spent_bucket_still_cools_the_key() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .insert_header("x-ratelimit-resource", "graphql")
                .set_body_json(serde_json::json!({
                    "data": null,
                    "errors": [{"message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."}]
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ok_json())
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let mut context = with_token("cooled-graphql", Duration::from_secs(5), 1024);
    context.budget.deadline = std::time::Instant::now() + Duration::from_secs(24 * 60 * 60);
    let (scope, heard) = cancel_on_announced_wait(context.budget.cancellation.clone());
    let error = scope
        .run(transport.execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &context,
        ))
        .await
        .expect_err("rate limited");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited, "{error:?}");
    let rate = error.rate_limit.expect("metadata");
    assert_eq!(rate.resource.as_deref(), Some("graphql"));
    assert_eq!(rate.reset_epoch_seconds, Some(reset));
    assert!(
        heard.lock().expect("lines").is_empty(),
        "GraphQL is not waited out"
    );
    let (sent, searched) = search_after(&server, &transport, &endpoint, "cooled-graphql").await;
    assert_eq!(sent, 0, "the key cools down: the search is not sent");
    assert_eq!(
        searched.map(|error| error.kind),
        Some(ProviderErrorKind::RateLimited)
    );
}
