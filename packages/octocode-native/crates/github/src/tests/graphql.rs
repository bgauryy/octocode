//! GraphQL partial data, semantic errors and their retries.
use super::*;

#[tokio::test]
async fn preserves_graphql_partial_data_and_errors() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":{"repository":{"name":"repo"}},
            "errors":[{"message":"field denied","path":["repository","secret"],"extensions":{"type":"FORBIDDEN"}}]
        })))
        .mount(&server)
        .await;
    let transport = anonymous_transport(&mock_endpoint(&server));
    let page = transport
        .execute_graphql(
            "query Q { repository { name } }",
            serde_json::json!({}),
            &RequestContext::with_timeout(Duration::from_secs(2), 1024),
        )
        .await
        .expect("graphql");
    assert_eq!(page.data.expect("data")["repository"]["name"], "repo");
    assert_eq!(page.errors[0].message, "field denied");
    assert_eq!(page.errors[0].extensions["type"], "FORBIDDEN");
}

#[tokio::test]
async fn graphql_top_level_rate_limited_type_is_a_primary_limit() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .set_body_json(serde_json::json!({
                    "data": null,
                    "errors": [{"type": "RATE_LIMITED", "message": "API rate limit exceeded for user ID 1."}]
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = with_token("t", Duration::from_secs(5), 1024);
    assert!(transport.graphql_available(&context));
    let error = transport
        .execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &context,
        )
        .await
        .expect_err("rate limited");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    let rate = error.rate_limit.expect("metadata");
    assert_eq!(rate.resource.as_deref(), Some("graphql"));
    assert_eq!(rate.reset_epoch_seconds, Some(reset));
    // Cooldown-until-reset for this key only (not a permanent host skip).
    assert!(!transport.graphql_available(&context));
    assert!(transport.graphql_available(&with_token("other", Duration::from_secs(5), 1024)));
}

/// D1 for GraphQL (RV-GH L1): a spent GraphQL quota (an hourly window, like
/// core) is disclosed at once however far the call's deadline lies: one
/// request, no wait announced, the reset carried.
#[tokio::test]
async fn a_spent_graphql_quota_is_disclosed_at_once_however_far_the_deadline() {
    let server = MockServer::builder().start().await;
    let reset = epoch_secs() + 3600;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .set_body_json(serde_json::json!({
                    "data": null,
                    "errors": [{"type": "RATE_LIMITED", "message": "API rate limit exceeded for user ID 1."}]
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let mut context = with_token("spent-graphql", Duration::from_secs(5), 1024);
    context.budget.wait_until = std::time::Instant::now() + Duration::from_secs(24 * 60 * 60);
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
        "no wait was announced: {:?}",
        heard.lock().expect("lines")
    );
    assert!(
        context.client_waits().is_empty(),
        "{:?}",
        context.client_waits()
    );
}

#[tokio::test]
async fn graphql_something_went_wrong_is_retried_like_a_500() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": null,
                "errors": [{"message": "Something went wrong while executing your query. This may be the result of a timeout."}]
            })),
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data": {"viewer": {"login": "me"}}})),
        ]))
        .expect(2)
        .mount(&server)
        .await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let page = transport
        .execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &with_token("t", Duration::from_secs(5), 1024),
        )
        .await
        .expect("retried");
    assert!(page.errors.is_empty());
    assert_eq!(page.data.expect("data")["viewer"]["login"], "me");
}

/// One logical GraphQL operation spends one physical-attempt budget: HTTP
/// retries and GraphQL semantic retries draw from the same count, so mixed
/// failures never send more than `max_attempts` requests.
#[tokio::test]
async fn graphql_semantic_and_http_retries_share_one_attempt_budget() {
    let transient = || {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": null,
            "errors": [{"message": "Something went wrong while executing your query. This may be the result of a timeout."}]
        }))
    };
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(Sequence::new(vec![
            ResponseTemplate::new(503),
            transient(),
            ResponseTemplate::new(503),
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data": {"viewer": {"login": "me"}}})),
        ]))
        .mount(&server)
        .await;
    let retry = RetryPolicy {
        max_attempts: 2,
        ..short_retry()
    };
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), retry);
    let page = transport
        .execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &with_token("t", Duration::from_secs(5), 1024),
        )
        .await
        .expect("the last answer is returned");
    assert!(
        !page.errors.is_empty(),
        "the budget ran out on the transient error"
    );
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        2,
        "two physical sends for max_attempts = 2"
    );
}

/// A cancel during a GraphQL semantic-retry backoff stops the operation
/// without another send.
#[tokio::test]
async fn graphql_semantic_retry_backoff_honors_cancellation() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": null,
            "errors": [{"message": "Something went wrong while executing your query. This may be the result of a timeout."}]
        })))
        .mount(&server)
        .await;
    let retry = RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_secs(30),
        ..short_retry()
    };
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), retry);
    let context = with_token("t", Duration::from_secs(60), 1024);
    let cancel = context.budget.cancellation.clone();
    let canceller = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        cancel.cancel();
    });
    let started = std::time::Instant::now();
    let result = transport
        .execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &context,
        )
        .await;
    canceller.await.expect("canceller");
    // A zero full-jitter draw may retry at once; every later wait is cancelled.
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(server.received_requests().await.unwrap_or_default().len() <= 3);
    assert_eq!(
        result.err().map(|error| error.kind),
        Some(ProviderErrorKind::Cancelled)
    );
}

/// GitHub may answer a GraphQL secondary limit with HTTP 200 and the limit in
/// `errors[]`, and warns that continuing can ban the integration. That starts
/// the key's cooldown, honors `retry-after`, and blocks REST too; REST is not a
/// way around it.
#[tokio::test]
async fn a_graphql_secondary_limit_on_http_200_starts_the_cooldown() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("retry-after", "3600")
                .set_body_json(serde_json::json!({
                    "data": null,
                    "errors": [{"message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."}]
                })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(0)
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = || with_token("t", Duration::from_secs(5), 1024);
    let error = transport
        .execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &context(),
        )
        .await
        .expect_err("limited");
    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
    assert_eq!(
        error.rate_limit.and_then(|rate| rate.retry_after_seconds),
        Some(3600)
    );
    assert!(!transport.graphql_available(&context()));
    let rest = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &context(),
        )
        .await
        .expect_err("REST waits out the same cooldown");
    assert_eq!(rest.kind, ProviderErrorKind::RateLimited);
    assert!(transport.graphql_available(&with_token("other", Duration::from_secs(5), 1024)));
}

/// A limit whose `type` says RATE_LIMITED but whose message names a secondary
/// limit is the key-wide cooldown, not one exhausted bucket; with no
/// `retry-after` it waits the secondary default.
#[tokio::test]
async fn a_graphql_secondary_limit_is_not_mistaken_for_the_primary_one() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "errors": [{"type": "RATE_LIMITED", "message": "You have exceeded a secondary rate limit."}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(0)
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = || with_token("t", Duration::from_secs(5), 1024);
    let error = transport
        .execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &context(),
        )
        .await
        .expect_err("limited");
    let waited = error
        .rate_limit
        .and_then(|rate| rate.retry_after_seconds)
        .expect("the wait");
    assert_eq!(
        waited,
        ExecutorConfig::octokit().secondary_default.as_secs()
    );
    transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &context(),
        )
        .await
        .expect_err("REST waits out the same cooldown");
}

/// A short `retry-after` is waited out and the read retried: the answer arrives
/// no sooner than GitHub allowed.
#[tokio::test]
async fn a_short_graphql_secondary_limit_is_waited_out() {
    let server = MockServer::builder().start().await;
    let limited = Sequence::new(vec![
        ResponseTemplate::new(200)
            .insert_header("retry-after", "1")
            .set_body_json(serde_json::json!({
                "errors": [{"message": "You have exceeded a secondary rate limit."}]
            })),
        ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({"data": {"viewer": {"login": "me"}}})),
    ]);
    let arrivals = limited.arrivals.clone();
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(limited)
        .mount(&server)
        .await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let page = transport
        .execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &with_token("t", Duration::from_secs(5), 1024),
        )
        .await
        .expect("retried");
    assert_eq!(page.data.expect("data")["viewer"]["login"], "me");
    let times = arrivals.times();
    assert_eq!(times.len(), 2);
    assert!(
        times[1].duration_since(times[0]) >= Duration::from_millis(900),
        "{times:?}"
    );
}

/// GraphQL reads go unspaced because they are reads: a document that mutates is
/// refused before anything is sent, wherever its operation sits.
#[tokio::test]
async fn a_mutating_graphql_document_is_never_sent() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {}})))
        .expect(0)
        .mount(&server)
        .await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    for document in [
        "mutation { addStar(input: {}) { clientMutationId } }",
        "  # a comment\n mutation Named($a: ID!) { x(a: $a) }",
        "query A { viewer { login } }\nmutation B { x }",
    ] {
        let error = transport
            .execute_graphql(
                document,
                serde_json::json!({}),
                &with_token("t", Duration::from_secs(5), 1024),
            )
            .await
            .expect_err(document);
        assert_eq!(error.kind, ProviderErrorKind::Validation, "{document}");
    }
}

/// A field, a string or a comment that says "mutation" is still a read.
#[tokio::test]
async fn a_read_that_mentions_mutation_is_sent() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {}})))
        .expect(3)
        .mount(&server)
        .await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    for document in [
        "query { viewer { mutation: login } }",
        "query { search(query: \"mutation x\") { codeCount } }",
        "# no mutation here\nquery { viewer { login } }",
    ] {
        transport
            .execute_graphql(
                document,
                serde_json::json!({}),
                &with_token("t", Duration::from_secs(5), 1024),
            )
            .await
            .expect(document);
    }
}
