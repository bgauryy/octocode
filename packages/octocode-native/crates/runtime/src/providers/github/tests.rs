use base64::{Engine as _, engine::general_purpose::STANDARD};
use secrecy::ExposeSecret;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use wiremock::{
    Mock, MockServer, Request, Respond, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

use super::*;

#[derive(Clone)]
struct RotatingResolver(Arc<AtomicUsize>);
impl CredentialResolver for RotatingResolver {
    fn resolve<'a>(
        &'a self,
        _: CredentialRequest<'a>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Option<ResolvedCredential>, ProviderError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let n = self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Some(ResolvedCredential::new(
                if n == 0 { "one" } else { "two" },
                CredentialSource::Storage,
            )))
        })
    }
}
struct FailingSource;
impl CredentialSourceProvider for FailingSource {
    fn load_blocking(&self, _: &str) -> Result<Option<secrecy::SecretString>, ProviderError> {
        Err(ProviderError::new(
            ProviderErrorKind::CredentialStoreUnavailable,
            "headless",
        ))
    }
}
struct FixtureSource;
impl CredentialSourceProvider for FixtureSource {
    fn load_blocking(&self, _: &str) -> Result<Option<secrecy::SecretString>, ProviderError> {
        Ok(Some(secrecy::SecretString::from("fallback")))
    }
}

#[derive(Default)]
struct MemoryCache(Mutex<Option<CachedContent>>);
#[derive(Clone)]
struct FailOnce(Arc<AtomicUsize>);
impl Respond for FailOnce {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(500)
        } else {
            ResponseTemplate::new(200).set_body_bytes(b"ok")
        }
    }
}
#[derive(Clone)]
struct SecondaryThenOk(Arc<AtomicUsize>, Option<&'static str>);
impl Respond for SecondaryThenOk {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            let mut response = ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "21")
                .set_body_json(serde_json::json!({
                    "message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."
                }));
            if let Some(retry_after) = self.1 {
                response = response.insert_header("retry-after", retry_after);
            }
            response
        } else {
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok":true}))
        }
    }
}
#[derive(Clone)]
struct AlwaysPrOnlyWithNext(String);
impl Respond for AlwaysPrOnlyWithNext {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let page: u64 = req
            .url
            .query_pairs()
            .find(|(k, _)| k == "page")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(1);
        ResponseTemplate::new(200)
            .insert_header(
                "link",
                &format!(
                    "<{}/api/v3/repos/acme/repo/issues?page={}>; rel=\"next\"",
                    self.0,
                    page + 1
                ),
            )
            .set_body_json(serde_json::json!([{
                "number": page,
                "title": "pr",
                "pull_request": {"url": "https://example.test"}
            }]))
    }
}
impl ConditionalCache for MemoryCache {
    fn get<'a>(
        &'a self,
        _: &'a CachePartition,
        _: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<CachedContent>> + Send + 'a>>
    {
        Box::pin(async move { self.0.lock().expect("cache lock").clone() })
    }
    fn put<'a>(
        &'a self,
        _: &'a CachePartition,
        _: String,
        value: CachedContent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            *self.0.lock().expect("cache lock") = Some(value);
        })
    }
}

#[test]
fn credential_resolution_handle_joins_on_drop() {
    let finished = Arc::new(AtomicUsize::new(0));
    let marker = finished.clone();
    let handle = CredentialResolutionHandle::from_join(std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        marker.store(1, Ordering::SeqCst);
        Ok(None)
    }));
    drop(handle);
    assert_eq!(finished.load(Ordering::SeqCst), 1);
}

async fn provider(server: &MockServer) -> GitHubProvider<StaticCredentialResolver, MemoryCache> {
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint,
        Arc::new(StaticCredentialResolver::new(
            "secret",
            CredentialSource::Environment,
        )),
        RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_retry_after: Duration::from_secs(1),
        },
    )
    .expect("transport");
    GitHubProvider {
        transport,
        cache: MemoryCache::default(),
    }
}

#[tokio::test]
async fn ghes_content_route_auth_and_decode() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"sha":sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path("/api/v3/repos/acme/repo/contents/src%2Flib.rs")).and(query_param("ref",sha)).and(header("authorization","Bearer secret"))
        .respond_with(ResponseTemplate::new(200).insert_header("etag","\"v1\"").set_body_json(serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("hello\n")}))).mount(&server).await;
    let result = provider(&server)
        .await
        .get_file_content(
            &ContentRequest {
                owner: "acme".into(),
                repo: "repo".into(),
                path: "src/lib.rs".into(),
                reference: Some("main".into()),
                force_refresh: false,
                session_id: None,
            },
            &RequestContext::with_timeout(Duration::from_secs(2), 1024),
        )
        .await
        .expect("content");
    assert_eq!(result.bytes, b"hello\n");
    assert_eq!(result.etag.as_deref(), Some("\"v1\""));
    assert!(!result.from_cache);
}

#[tokio::test]
async fn conditional_304_reuses_cached_body() {
    let server = MockServer::start().await;
    let provider = provider(&server).await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"sha":sha})))
        .mount(&server)
        .await;
    *provider.cache.0.lock().expect("cache lock") = Some(CachedContent {
        etag: Some("\"v1\"".into()),
        bytes: b"cached".to_vec(),
        resolved_ref: sha.into(),
    });
    Mock::given(method("GET"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&server)
        .await;
    let result = provider
        .get_file_content(
            &ContentRequest {
                owner: "a".into(),
                repo: "b".into(),
                path: "x".into(),
                reference: Some("main".into()),
                force_refresh: false,
                session_id: None,
            },
            &RequestContext::with_timeout(Duration::from_secs(2), 1024),
        )
        .await
        .expect("cached");
    assert_eq!(result.bytes, b"cached");
    assert!(result.from_cache);
    assert_eq!(result.raw_response_bytes, 0);
    assert_eq!(result.resolved_ref, sha);
}

#[tokio::test]
async fn distinguishes_permission_from_rate_limit_and_bounds_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-github-request-id", "R1")
                .insert_header("x-ratelimit-remaining", "1")
                .set_body_json(serde_json::json!({"message":"forbidden"})),
        )
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy::default(),
    )
    .expect("transport");
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["x"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(2), 1024),
        )
        .await
        .expect_err("permission");
    assert_eq!(error.kind, ProviderErrorKind::Permission);
    assert_eq!(error.request_id.as_deref(), Some("R1"));

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0; 17]))
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy::default(),
    )
    .expect("transport");
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["x"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(2), 16),
        )
        .await
        .expect_err("limit");
    assert_eq!(error.kind, ProviderErrorKind::ResponseTooLarge);
}

#[tokio::test]
async fn returns_same_origin_next_page_and_rejects_redirects() {
    let server = MockServer::start().await;
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
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy::default(),
    )
    .expect("transport");
    let page = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["items"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(2), 16),
        )
        .await
        .expect("page");
    assert_eq!(page.next.expect("next").as_str(), next);

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "https://example.com"))
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy::default(),
    )
    .expect("transport");
    let error = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["x"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(2), 16),
        )
        .await
        .expect_err("redirect");
    assert_eq!(error.kind, ProviderErrorKind::RedirectDenied);
}

#[tokio::test]
async fn retries_secondary_rate_limit_without_remaining_zero() {
    let server = MockServer::start().await;
    let hits = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .respond_with(SecondaryThenOk(hits.clone(), Some("1")))
        .expect(2)
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::with_budget(
        endpoint.clone(),
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy {
            max_attempts: 3,
            base_delay: Duration::from_millis(1),
            max_retry_after: Duration::from_secs(5),
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
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn lists_issues_skipping_pull_request_only_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo/issues"))
        .and(query_param("page", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    &format!(
                        "<{}/api/v3/repos/acme/repo/issues?page=2>; rel=\"next\"",
                        server.uri()
                    ),
                )
                .set_body_json(serde_json::json!([{
                    "number": 1,
                    "title": "pr",
                    "pull_request": {"url": "https://example.test"}
                }])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo/issues"))
        .and(query_param("page", "2"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "number": 2,
                "title": "real issue"
            }])),
        )
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint,
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_retry_after: Duration::from_secs(1),
        },
    )
    .expect("transport");
    let page = transport
        .list_issues(
            &IssueListRequest {
                owner: "acme".into(),
                repo: "repo".into(),
                state: None,
                assignee: None,
                author: None,
                mentions: None,
                labels: None,
                sort: None,
                order: None,
                page: 1,
                per_page: 30,
            },
            &RequestContext::with_timeout(Duration::from_secs(5), 16 * 1024),
        )
        .await
        .expect("list");
    assert_eq!(page.skipped_pull_request_pages, 1);
    assert_eq!(page.provider_page, 2);
    assert_eq!(page.items[0]["number"], 2);
}

/// A repo where issues never turn up before the skip budget runs out (e.g. a
/// very high PR-to-issue ratio) must still return promptly with a clear
/// signal instead of an agent having to make one external tool call per
/// skipped page. `has_more` staying `true` here is correct — GitHub really
/// does have more pages — but the walk should absorb `MAX_PR_ONLY_PAGES_TO_SKIP`
/// of that work in a single call, not force the caller into it one page at a
/// time.
#[tokio::test]
async fn list_issues_stops_at_skip_budget_and_reports_has_more() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo/issues"))
        .respond_with(AlwaysPrOnlyWithNext(server.uri()))
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint,
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_retry_after: Duration::from_secs(1),
        },
    )
    .expect("transport");
    let page = transport
        .list_issues(
            &IssueListRequest {
                owner: "acme".into(),
                repo: "repo".into(),
                state: None,
                assignee: None,
                author: None,
                mentions: None,
                labels: None,
                sort: None,
                order: None,
                page: 1,
                per_page: 30,
            },
            &RequestContext::with_timeout(Duration::from_secs(5), 16 * 1024),
        )
        .await
        .expect("list");
    assert!(page.items.is_empty());
    assert_eq!(page.skipped_pull_request_pages, MAX_PR_ONLY_PAGES_TO_SKIP);
    assert_eq!(page.provider_page, 1 + MAX_PR_ONLY_PAGES_TO_SKIP);
    assert!(
        page.has_more,
        "GitHub genuinely reports more pages; hasMore must stay true"
    );
    assert!(
        page.warnings
            .iter()
            .any(|w| w.contains(&format!("{MAX_PR_ONLY_PAGES_TO_SKIP}-page skip budget"))),
        "warnings should explain why the scan stopped: {:?}",
        page.warnings
    );
}

#[tokio::test]
async fn projects_rate_limit_metadata_without_retrying_long_delays() {
    let server = MockServer::start().await;
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
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy::default(),
    )
    .expect("transport");
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
        Some(RateLimit {
            remaining: Some(0),
            reset_epoch_seconds: Some(1_900_000_000),
            retry_after_seconds: Some(60),
            resource: Some("core".into()),
        })
    );
}

#[tokio::test]
async fn cancellation_interrupts_an_inflight_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy::default(),
    )
    .expect("transport");
    let context = RequestContext::with_timeout(Duration::from_secs(3), 16);
    let cancellation = context.cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
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
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("link", "<https://evil.example/items?page=2>; rel=\"next\"")
                .set_body_bytes(b"[]"),
        )
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy::default(),
    )
    .expect("transport");
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
async fn preserves_graphql_partial_data_and_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data":{"repository":{"name":"repo"}},
            "errors":[{"message":"field denied","path":["repository","secret"],"extensions":{"type":"FORBIDDEN"}}]
        })))
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint,
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy::default(),
    )
    .expect("transport");
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
async fn cache_partition_covers_endpoint_credential_and_session() {
    let server = MockServer::start().await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint,
        Arc::new(StaticCredentialResolver::new(
            "one",
            CredentialSource::Storage,
        )),
        RetryPolicy::default(),
    )
    .expect("transport");
    let context = RequestContext::with_timeout(Duration::from_secs(2), 16);
    let a = transport
        .cache_partition(&context, Some("a"))
        .await
        .expect("partition");
    let b = transport
        .cache_partition(&context, Some("b"))
        .await
        .expect("partition");
    assert_ne!(a, b);
    let mut override_context = RequestContext::with_timeout(Duration::from_secs(2), 16);
    override_context.override_token = Some(secrecy::SecretString::from("two"));
    let overridden = transport
        .cache_partition(&override_context, Some("a"))
        .await
        .expect("partition");
    assert_ne!(a, overridden);
}

#[tokio::test]
async fn pins_one_credential_across_partition_and_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(header("authorization", "Bearer one"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"ok"))
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let calls = Arc::new(AtomicUsize::new(0));
    let transport = GitHubTransport::new(
        endpoint.clone(),
        Arc::new(RotatingResolver(calls.clone())),
        RetryPolicy::default(),
    )
    .expect("transport");
    let context = RequestContext::with_timeout(Duration::from_secs(2), 16);
    transport
        .cache_partition(&context, Some("s"))
        .await
        .expect("partition");
    transport
        .execute(
            RequestSpec::get(endpoint.rest(&["x"]).expect("route")),
            &context,
        )
        .await
        .expect("request");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn chained_source_falls_back_when_primary_store_is_unavailable() {
    let source = ChainedCredentialSource::new(FailingSource, FixtureSource);
    let value = source
        .load_blocking("github.com")
        .expect("fallback")
        .expect("token");
    assert_eq!(value.expose_secret(), "fallback");
}

#[test]
fn canonicalizes_github_dot_com_credential_host() {
    assert_eq!(GitHubEndpoint::github_com().credential_host(), "github.com");
    let ghes = GitHubEndpoint::new(url::Url::parse("https://ghe.example/api/v3").expect("URL"))
        .expect("endpoint");
    assert_eq!(ghes.credential_host(), "ghe.example");
}

#[tokio::test]
async fn content_413_falls_back_to_parent_directory_and_blob() {
    let server = MockServer::start().await;
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let blob = "89abcdef0123456789abcdef0123456789abcdef";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"sha":commit})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/dir%2Flarge.txt"))
        .respond_with(
            ResponseTemplate::new(413).set_body_json(serde_json::json!({"message":"too large"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/dir"))
        .and(query_param("ref", commit))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!([{"name":"large.txt","sha":blob,"type":"file"}])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/blobs/{blob}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"encoding":"base64","content":STANDARD.encode("large body")}),
        ))
        .mount(&server)
        .await;
    let result = provider(&server)
        .await
        .get_file_content(
            &ContentRequest {
                owner: "a".into(),
                repo: "b".into(),
                path: "dir/large.txt".into(),
                reference: Some("main".into()),
                force_refresh: false,
                session_id: None,
            },
            &RequestContext::with_timeout(Duration::from_secs(2), 4096),
        )
        .await
        .expect("fallback");
    assert_eq!(result.bytes, b"large body");
    assert_eq!(result.resolved_ref, commit);
}

#[tokio::test]
async fn content_encoding_none_for_large_file_fetches_blob() {
    // GitHub returns 200 with `encoding:"none"` and empty content for files
    // between 1 MB and 100 MB; the bytes must come from git/blobs/{sha}.
    let server = MockServer::start().await;
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let blob = "89abcdef0123456789abcdef0123456789abcdef";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"sha":commit})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/big.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "type":"file","encoding":"none","content":"","size":2_000_000,"sha":blob
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/blobs/{blob}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"encoding":"base64","content":STANDARD.encode("large body")}),
        ))
        .mount(&server)
        .await;
    let result = provider(&server)
        .await
        .get_file_content(
            &ContentRequest {
                owner: "a".into(),
                repo: "b".into(),
                path: "big.txt".into(),
                reference: Some("main".into()),
                force_refresh: true,
                session_id: None,
            },
            &RequestContext::with_timeout(Duration::from_secs(2), 4096),
        )
        .await
        .expect("blob fallback");
    assert_eq!(result.bytes, b"large body");
}

#[tokio::test]
async fn content_directory_symlink_and_submodule_get_clear_errors() {
    let server = MockServer::start().await;
    let commit = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"sha":commit})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"name":"lib.rs","path":"src/lib.rs","type":"file"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/link"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"type":"symlink","target":"src/lib.rs"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/vendor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"type":"submodule","submodule_git_url":"https://x/y.git"}),
        ))
        .mount(&server)
        .await;
    let provider = provider(&server).await;
    let read = |p: &'static str| {
        let provider = &provider;
        async move {
            provider
                .get_file_content(
                    &ContentRequest {
                        owner: "a".into(),
                        repo: "b".into(),
                        path: p.into(),
                        reference: Some("main".into()),
                        force_refresh: true,
                        session_id: None,
                    },
                    &RequestContext::with_timeout(Duration::from_secs(2), 4096),
                )
                .await
                .expect_err("not a file")
        }
    };
    let dir = read("src").await;
    assert_eq!(dir.kind, ProviderErrorKind::Validation);
    assert!(dir.message.contains("is a directory"), "{}", dir.message);
    let link = read("link").await;
    assert!(link.message.contains("symlink"), "{}", link.message);
    assert!(link.message.contains("src/lib.rs"), "{}", link.message);
    let module = read("vendor").await;
    assert!(module.message.contains("submodule"), "{}", module.message);
}

#[tokio::test]
async fn retries_transient_server_failure_once() {
    let server = MockServer::start().await;
    let attempts = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .respond_with(FailOnce(attempts.clone()))
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let transport = GitHubTransport::new(
        endpoint.clone(),
        Arc::new(StaticCredentialResolver::anonymous()),
        RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_retry_after: Duration::from_secs(1),
        },
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
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
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

// ── Keyed executor: Octokit throttling/retry parity ─────────────────────────

fn executor_transport(
    server: &MockServer,
    token: Option<&str>,
    budget: Arc<GitHubBudget>,
    retry: RetryPolicy,
) -> (GitHubTransport<StaticCredentialResolver>, GitHubEndpoint) {
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    let resolver = match token {
        Some(token) => StaticCredentialResolver::new(token, CredentialSource::Environment),
        None => StaticCredentialResolver::anonymous(),
    };
    let transport =
        GitHubTransport::with_budget(endpoint.clone(), Arc::new(resolver), retry, budget)
            .expect("transport");
    (transport, endpoint)
}

fn short_retry() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_retry_after: Duration::from_secs(5),
    }
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

#[derive(Clone)]
struct PrimaryThenOk(Arc<AtomicUsize>, u64);
impl Respond for PrimaryThenOk {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", self.1.to_string().as_str())
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(serde_json::json!({"message": "API rate limit exceeded"}))
        } else {
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true}))
        }
    }
}

#[tokio::test]
async fn primary_limit_waits_for_a_short_reset_then_retries() {
    let server = MockServer::start().await;
    let hits = Arc::new(AtomicUsize::new(0));
    let reset = epoch_secs() + 1;
    Mock::given(method("GET"))
        .respond_with(PrimaryThenOk(hits.clone(), reset))
        .expect(2)
        .mount(&server)
        .await;
    let (transport, endpoint) =
        executor_transport(&server, Some("t"), GitHubBudget::relaxed(), short_retry());
    let page = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["repos", "a", "b"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(5), 1024),
        )
        .await
        .expect("retried after reset");
    assert_eq!(page.status, 200);
    assert!(epoch_secs() >= reset, "retried before the reset");
}

#[tokio::test]
async fn primary_limit_fails_fast_and_blocks_later_sends_until_reset() {
    let server = MockServer::start().await;
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
    let (transport, endpoint) =
        executor_transport(&server, Some("t"), GitHubBudget::relaxed(), short_retry());
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1024);
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
    let server = MockServer::start().await;
    let hits = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .respond_with(SecondaryThenOk(hits.clone(), None))
        .expect(1)
        .mount(&server)
        .await;
    let (transport, endpoint) =
        executor_transport(&server, None, GitHubBudget::relaxed(), short_retry());
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
    // 403 secondary keeps rate-limit metadata (previously dropped).
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
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn do_not_retry_statuses_are_sent_once() {
    for status in [400_u16, 401, 403, 404, 410, 422, 451] {
        let server = MockServer::start().await;
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
            executor_transport(&server, None, GitHubBudget::relaxed(), short_retry());
        let error = transport
            .execute(
                RequestSpec::get(endpoint.rest(&["x"]).expect("route")),
                &RequestContext::with_timeout(Duration::from_secs(2), 1024),
            )
            .await
            .expect_err("final status");
        assert_ne!(error.kind, ProviderErrorKind::RateLimited, "{status}");
        assert_eq!(error.status, Some(status));
        server.verify().await;
    }
}

#[tokio::test]
async fn graphql_top_level_rate_limited_type_is_a_primary_limit() {
    let server = MockServer::start().await;
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
    let (transport, _) =
        executor_transport(&server, Some("t"), GitHubBudget::relaxed(), short_retry());
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1024);
    assert!(transport.graphql_available(&context).await);
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
    assert!(!transport.graphql_available(&context).await);
    let (other, _) = executor_transport(
        &server,
        Some("other"),
        GitHubBudget::relaxed(),
        short_retry(),
    );
    assert!(other.graphql_available(&context).await);
}

#[derive(Clone)]
struct GraphqlWentWrongOnce(Arc<AtomicUsize>);
impl Respond for GraphqlWentWrongOnce {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": null,
                "errors": [{"message": "Something went wrong while executing your query. This may be the result of a timeout."}]
            }))
        } else {
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"data": {"viewer": {"login": "me"}}}))
        }
    }
}

#[tokio::test]
async fn graphql_something_went_wrong_is_retried_like_a_500() {
    let server = MockServer::start().await;
    let hits = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(GraphqlWentWrongOnce(hits.clone()))
        .expect(2)
        .mount(&server)
        .await;
    let (transport, _) =
        executor_transport(&server, Some("t"), GitHubBudget::relaxed(), short_retry());
    let page = transport
        .execute_graphql(
            "query { viewer { login } }",
            serde_json::json!({}),
            &RequestContext::with_timeout(Duration::from_secs(5), 1024),
        )
        .await
        .expect("retried");
    assert!(page.errors.is_empty());
    assert_eq!(page.data.expect("data")["viewer"]["login"], "me");
}

#[derive(Clone)]
struct ServerErrorRetryAfterOnce(Arc<AtomicUsize>);
impl Respond for ServerErrorRetryAfterOnce {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(503).insert_header("retry-after", "1")
        } else {
            ResponseTemplate::new(200).set_body_bytes(b"ok")
        }
    }
}

#[tokio::test]
async fn permits_are_released_while_backing_off() {
    let server = MockServer::start().await;
    let hits = Arc::new(AtomicUsize::new(0));
    Mock::given(method("GET"))
        .and(path("/api/v3/flaky"))
        .respond_with(ServerErrorRetryAfterOnce(hits.clone()))
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
    let (transport, endpoint) = executor_transport(&server, Some("t"), budget, short_retry());
    let slow = {
        let transport = transport.clone();
        let url = endpoint.rest(&["flaky"]).expect("route");
        tokio::spawn(async move {
            transport
                .execute(
                    RequestSpec::get(url),
                    &RequestContext::with_timeout(Duration::from_secs(5), 1024),
                )
                .await
        })
    };
    while hits.load(Ordering::SeqCst) == 0 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    let started = std::time::Instant::now();
    let fast = transport
        .execute(
            RequestSpec::get(endpoint.rest(&["fast"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(5), 1024),
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
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[derive(Clone)]
struct Arrivals(Arc<Mutex<Vec<std::time::Instant>>>);
impl Respond for Arrivals {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.0
            .lock()
            .expect("arrivals")
            .push(std::time::Instant::now());
        ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": []}))
    }
}

#[tokio::test]
async fn search_requests_are_spaced_per_key() {
    let server = MockServer::start().await;
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(Arrivals(arrivals.clone()))
        .mount(&server)
        .await;
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        search_spacing: Duration::from_millis(150),
        ..ExecutorConfig::relaxed()
    }));
    let (transport, endpoint) = executor_transport(&server, Some("t"), budget, short_retry());
    let calls = (0..3).map(|_| {
        let transport = transport.clone();
        let url = endpoint.rest(&["search", "issues"]).expect("route");
        async move {
            transport
                .execute(
                    RequestSpec::get(url),
                    &RequestContext::with_timeout(Duration::from_secs(5), 1024),
                )
                .await
                .expect("search")
        }
    });
    futures_util::future::join_all(calls).await;
    let arrivals = arrivals.lock().expect("arrivals").clone();
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
    let server = MockServer::start().await;
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
    let (exhausted, endpoint) =
        executor_transport(&server, Some("exhausted"), budget.clone(), short_retry());
    let (fresh, _) = executor_transport(&server, Some("fresh"), budget, short_retry());
    let context = || RequestContext::with_timeout(Duration::from_secs(5), 1024);
    let url = endpoint.rest(&["repos", "a", "b"]).expect("route");
    exhausted
        .execute(RequestSpec::get(url.clone()), &context())
        .await
        .expect_err("exhausted");
    exhausted
        .execute(RequestSpec::get(url.clone()), &context())
        .await
        .expect_err("still exhausted, not sent");
    for _ in 0..2 {
        fresh
            .execute(RequestSpec::get(url.clone()), &context())
            .await
            .expect("other token unaffected");
    }
}
