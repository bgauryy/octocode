//! The circuit breaker: what opens it, what it reports, and what resets it.
use super::*;

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
