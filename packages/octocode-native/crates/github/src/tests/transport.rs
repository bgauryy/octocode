//! Statuses, redirects, pagination links, credentials and routes.
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
async fn pagination_respects_last_page_only_for_matching_numbered_links() {
    for (next_query, last_route, has_next) in [
        ("page=4", "items?page=3", false),
        ("page=3", "items?page=3", true),
        ("after=opaque", "items?page=3", true),
        ("page=4&per_page=1", "items?page=3&per_page=2", true),
        ("page=4", "other?page=3", true),
    ] {
        let server = MockServer::builder().start().await;
        let next = format!("{}/api/v3/items?{next_query}", server.uri());
        let last = format!("{}/api/v3/{last_route}", server.uri());
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        "link",
                        format!("<{next}>; rel=\"next\", <{last}>; rel=\"last\""),
                    )
                    .set_body_bytes(b"[]"),
            )
            .mount(&server)
            .await;
        let endpoint = mock_endpoint(&server);
        let transport = anonymous_transport(&endpoint);
        let response = transport
            .execute(
                RequestSpec::get(endpoint.rest(&["items"]).expect("route")),
                &RequestContext::with_timeout(Duration::from_secs(5), 16),
            )
            .await
            .expect("page");
        assert_eq!(
            response.next.is_some(),
            has_next,
            "{next_query}, {last_route}"
        );
        if let Some(actual) = response.next {
            assert_eq!(actual.as_str(), next);
        }
    }
}

#[tokio::test]
async fn returns_same_origin_next_page_and_rejects_redirects() {
    let server = MockServer::builder().start().await;
    let next = format!("{}/api/v3/items?page=2", server.uri());
    Mock::given(method("GET"))
        .and(path("/api/v3/items"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", format!("<{next}>; rel=\"next\""))
                .set_body_bytes(b"[]"),
        )
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let transport = anonymous_transport(&endpoint);
    let page = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["items"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(2), 16),
        )
        .await
        .expect("page");
    assert_eq!(page.next.expect("next").as_str(), next);

    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "https://example.com"))
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let error = get_x(&anonymous_transport(&endpoint), &endpoint, 16)
        .await
        .expect_err("redirect");
    assert_eq!(error.kind, ProviderErrorKind::RedirectDenied);
}

#[tokio::test]
async fn cancellation_interrupts_an_inflight_response() {
    let server = MockServer::builder().start().await;
    // The server signals once the request arrives, so the cancel lands while
    // the delayed response is in flight rather than after a guessed sleep.
    let arrived = Arc::new(tokio::sync::Notify::new());
    Mock::given(method("GET"))
        .respond_with(Delayed {
            arrived: Some(arrived.clone()),
            ..Delayed::new(Duration::from_secs(2), ResponseTemplate::new(200))
        })
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let transport = anonymous_transport(&endpoint);
    let context = RequestContext::with_timeout(Duration::from_secs(3), 16);
    let cancellation = context.budget.cancellation.clone();
    tokio::spawn(async move {
        arrived.notified().await;
        cancellation.cancel();
    });
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["slow"]).expect("route")),
            &context,
        )
        .await
        .expect_err("cancelled");
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
}

#[tokio::test]
async fn rejects_cross_origin_pagination() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", "<https://evil.example/items?page=2>; rel=\"next\"")
                .set_body_bytes(b"[]"),
        )
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let transport = anonymous_transport(&endpoint);
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["items"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(2), 16),
        )
        .await
        .expect_err("origin");
    assert_eq!(error.kind, ProviderErrorKind::RedirectDenied);
}

#[tokio::test]
async fn cache_partition_covers_endpoint_credential_and_session() {
    let server = MockServer::builder().start().await;
    let endpoint = mock_endpoint(&server);
    let transport = GitHubTransport::new(endpoint, RetryPolicy::default(), GitHubBudget::relaxed())
        .expect("transport");
    let context = with_token("one", Duration::from_secs(2), 16);
    let a = transport
        .cache_partition(&context, Some("a"))
        .expect("partition");
    let b = transport
        .cache_partition(&context, Some("b"))
        .expect("partition");
    assert_ne!(a, b);
    let other = transport
        .cache_partition(&with_token("two", Duration::from_secs(2), 16), Some("a"))
        .expect("partition");
    assert_ne!(a, other);
    let anonymous = transport
        .cache_partition(
            &RequestContext::with_timeout(Duration::from_secs(2), 16),
            Some("a"),
        )
        .expect("partition");
    assert_ne!(a, anonymous);
}

/// One transport serves many requests; each carries its own context's
/// credential (a credential rotated between requests is used at once).
#[tokio::test]
async fn each_request_carries_its_own_context_credential() {
    let server = MockServer::builder().start().await;
    for token in ["one", "two"] {
        Mock::given(method("GET"))
            .and(header("authorization", format!("Bearer {token}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(token.as_bytes()))
            .expect(1)
            .mount(&server)
            .await;
    }
    let endpoint = mock_endpoint(&server);
    let transport = GitHubTransport::new(
        endpoint.clone(),
        RetryPolicy::default(),
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    for token in ["one", "two"] {
        let page = transport
            .execute(
                RequestSpec::get(endpoint.rest(&["x"]).expect("route")),
                &with_token(token, Duration::from_secs(2), 16),
            )
            .await
            .expect("request");
        assert_eq!(page.body.as_ref(), token.as_bytes());
    }
}

#[test]
fn canonicalizes_github_dot_com_credential_host() {
    assert_eq!(api_github_com().credential_host(), "github.com");
    let ghes = GitHubEndpoint::new(url::Url::parse("https://ghe.example/api/v3").expect("URL"))
        .expect("endpoint");
    assert_eq!(ghes.credential_host(), "ghe.example");
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

#[test]
fn routes_graphql_and_escapes_content_path_as_one_segment() {
    let endpoint = GitHubEndpoint::new(url::Url::parse("https://ghe.example/api/v3").expect("URL"))
        .expect("endpoint");
    assert_eq!(
        endpoint.graphql().as_str(),
        "https://ghe.example/api/graphql"
    );
    assert_eq!(
        endpoint
            .rest(&["repos", "a", "b", "contents", "dir/a b.rs"])
            .expect("route")
            .as_str(),
        "https://ghe.example/api/v3/repos/a/b/contents/dir%2Fa%20b.rs"
    );
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

#[test]
fn rest_routes_reject_dot_segments() {
    let endpoint = api_github_com();
    for bad in [".", ".."] {
        let error = endpoint
            .rest(&["repos", bad, "r", "pulls"])
            .expect_err("dot segment must not rewrite the route");
        assert_eq!(error.kind, ProviderErrorKind::Validation);
    }
    assert_eq!(
        endpoint
            .rest(&["repos", "o", "r", "contents", "a/../b"])
            .expect("encoded path segment")
            .path(),
        "/repos/o/r/contents/a%2F..%2Fb"
    );
}

#[test]
fn plain_http_api_base_is_limited_to_loopback() {
    let parse = |raw: &str| GitHubEndpoint::new(url::Url::parse(raw).expect("URL"));
    for allowed in [
        "https://ghe.example.com/api/v3",
        "http://127.0.0.1:8080/api/v3",
        "http://localhost:3000/",
        "http://[::1]:9000/",
    ] {
        assert!(parse(allowed).is_ok(), "{allowed}");
    }
    let error = parse("http://ghe.example.com/api/v3").expect_err("cleartext token");
    assert_eq!(error.kind, ProviderErrorKind::Configuration);
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
