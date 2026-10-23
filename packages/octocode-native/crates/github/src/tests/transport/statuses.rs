//! Failed responses: typed statuses, bounded error bodies, retries and dropped connections.
use super::*;

#[tokio::test]
async fn distinguishes_permission_from_rate_limit_and_bounds_body() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-github-request-id", "R1")
                .insert_header("x-ratelimit-remaining", "1")
                .set_body_json(serde_json::json!({"message":"forbidden"})),
        )
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let error = get_x(&anonymous_transport(&endpoint), &endpoint, 1024)
        .await
        .expect_err("permission");
    assert_eq!(error.kind, ProviderErrorKind::Permission);
    assert_eq!(error.request_id.as_deref(), Some("R1"));

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0; 17]))
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let error = get_x(&anonymous_transport(&endpoint), &endpoint, 16)
        .await
        .expect_err("limit");
    assert_eq!(error.kind, ProviderErrorKind::ResponseTooLarge);
}

#[tokio::test]
async fn retries_transient_server_failure_once() {
    let server = MockServer::builder().start().await;
    let flaky = Sequence::new(vec![
        ResponseTemplate::new(500),
        ResponseTemplate::new(200).set_body_bytes(b"ok"),
    ]);
    let attempts = flaky.arrivals.clone();
    Mock::given(method("GET"))
        .respond_with(flaky)
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let transport = GitHubTransport::new(
        endpoint.clone(),
        RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_retry_after: Duration::from_secs(1),
            max_rate_limit_wait: Duration::from_secs(1),
        },
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    let page = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["flaky"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(2), 16),
        )
        .await
        .expect("retry");
    assert_eq!(page.body, b"ok".as_slice());
    assert_eq!(attempts.count(), 2);
}

#[tokio::test]
async fn do_not_retry_statuses_are_sent_once() {
    for status in [400_u16, 401, 403, 404, 410, 422, 451] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("x-ratelimit-remaining", "10")
                    .set_body_json(serde_json::json!({"message": "nope"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let (transport, endpoint) =
            executor_transport(&server, GitHubBudget::relaxed(), short_retry());
        let error = get_x(&transport, &endpoint, 1024)
            .await
            .expect_err("final status");
        assert_ne!(error.kind, ProviderErrorKind::RateLimited, "{status}");
        assert_eq!(error.status, Some(status));
        server.verify().await;
    }
}

#[tokio::test]
async fn legal_block_and_unmapped_statuses_are_not_network_failures() {
    for (status, kind, needle) in [
        (
            451_u16,
            ProviderErrorKind::Unavailable,
            "legal reasons (HTTP 451)",
        ),
        (418, ProviderErrorKind::HttpStatus, "HTTP 418"),
    ] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(serde_json::json!({"message": "blocked"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let (transport, endpoint) =
            executor_transport(&server, GitHubBudget::relaxed(), short_retry());
        let error = get_x(&transport, &endpoint, 1024)
            .await
            .expect_err("status error");
        assert_eq!(error.kind, kind, "{status}");
        assert!(!error.retryable, "{status}");
        assert_eq!(error.status, Some(status));
        assert!(error.message.contains(needle), "{}", error.message);
        server.verify().await;
    }
}

#[tokio::test]
async fn oversized_error_body_keeps_status_and_says_the_body_was_not_read() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404).set_body_bytes(vec![b'x'; 4096]))
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let error = get_x(&transport, &endpoint, 64)
        .await
        .expect_err("not found");
    assert_eq!(error.kind, ProviderErrorKind::NotFound);
    assert!(
        error.message.contains("error body exceeded limit"),
        "{}",
        error.message
    );
}

#[tokio::test]
async fn cancel_while_reading_an_error_body_is_cancelled() {
    use std::io::{Read, Write};
    // Headers and part of a 404 body arrive, then the server stalls.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("address");
    let (sent, partial_sent) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let Ok((mut socket, _)) = listener.accept() else {
            return;
        };
        let mut request = [0_u8; 4096];
        let _ = socket.read(&mut request);
        let _ = socket.write_all(
            b"HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\ncontent-length: 100000\r\n\r\n{\"message\":\"par",
        );
        let _ = socket.flush();
        let _ = sent.send(());
        std::thread::sleep(Duration::from_secs(3));
    });
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("http://{address}/api/v3")).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(endpoint.clone(), short_retry(), GitHubBudget::relaxed())
        .expect("transport");
    let context = RequestContext::with_timeout(Duration::from_secs(4), 1 << 20);
    let cancellation = context.budget.cancellation.clone();
    // Cancel once the headers and partial body are on the wire.
    tokio::spawn(async move {
        let _ = partial_sent.await;
        cancellation.cancel();
    });
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["x"]).expect("route")),
            &context,
        )
        .await
        .expect_err("cancelled");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled, "{error:?}");
}

/// Dropped connections are retried by the transport's own budgeted policy and
/// by nothing underneath it: exactly `max_attempts` connections, no more.
#[tokio::test]
async fn a_dropped_connection_takes_exactly_the_policy_attempts() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let accepts = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&accepts);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            seen.fetch_add(1, Ordering::SeqCst);
            drop(stream);
        }
    });
    let endpoint = GitHubEndpoint::new(
        url::Url::parse(&format!("http://127.0.0.1:{port}/api/v3")).expect("URL"),
    )
    .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        RetryPolicy {
            max_attempts: 3,
            ..short_retry()
        },
        breaker_budget(10),
    )
    .expect("transport");
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("every attempt is dropped");
    assert_eq!(error.kind, ProviderErrorKind::Transport);
    assert_eq!(accepts.load(Ordering::SeqCst), 3);
}

/// The SAML SSO authorization URL travels with the failure.
#[tokio::test]
async fn a_saml_sso_403_carries_the_authorization_url() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header(
                    "x-github-sso",
                    "required; url=https://github.com/orgs/acme/sso?authorization_request=ABC",
                )
                .set_body_json(serde_json::json!({
                    "message": "Resource protected by organization SAML enforcement."
                })),
        )
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let error = get_status(&transport, &endpoint, &["repos", "a", "b"])
        .await
        .expect_err("sso");
    assert_eq!(error.kind, ProviderErrorKind::Permission);
    assert_eq!(
        error.sso_url.as_deref(),
        Some("https://github.com/orgs/acme/sso?authorization_request=ABC")
    );
}
