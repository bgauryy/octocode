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
