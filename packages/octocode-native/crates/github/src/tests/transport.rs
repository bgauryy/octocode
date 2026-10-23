//! Statuses, redirects, pagination links, credentials and routes.
use super::*;

mod statuses;

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
