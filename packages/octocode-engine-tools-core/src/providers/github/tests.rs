use aes_gcm::{AesGcm, KeyInit, Nonce, aead::AeadInPlace, aead::consts::U16, aes::Aes256};
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
        Ok(Some(secrecy::SecretString::from("legacy")))
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
struct SecondaryThenOk(Arc<AtomicUsize>);
impl Respond for SecondaryThenOk {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "21")
                .set_body_json(serde_json::json!({
                    "message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."
                }))
        } else {
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok":true}))
        }
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
        .respond_with(SecondaryThenOk(hits.clone()))
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
async fn legacy_fallback_survives_unavailable_platform_store() {
    let source = ChainedCredentialSource::new(FailingSource, FixtureSource);
    let value = source
        .load_blocking("github.com")
        .expect("fallback")
        .expect("token");
    assert_eq!(value.expose_secret(), "legacy");
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

#[tokio::test]
async fn legacy_encrypted_store_is_read_only_and_strict() {
    let home = std::env::temp_dir().join(format!("octocode-legacy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("home");
    let key = [7_u8; 32];
    let nonce = [3_u8; 16];
    let mut plaintext = serde_json::to_vec(&serde_json::json!({"version":1,"credentials":{"github.com":{"hostname":"github.com","username":"u","gitProtocol":"https","createdAt":"x","updatedAt":"x","token":{"token":"legacy-secret","tokenType":"oauth"}}}})).expect("json");
    let cipher = AesGcm::<Aes256, U16>::new_from_slice(&key).expect("cipher");
    let tag = cipher
        .encrypt_in_place_detached(Nonce::from_slice(&nonce), b"", &mut plaintext)
        .expect("encrypt");
    std::fs::write(home.join(".key"), hex::encode(key)).expect("key");
    std::fs::write(
        home.join("credentials.json"),
        format!(
            "{}:{}:{}",
            hex::encode(nonce),
            hex::encode(tag),
            hex::encode(plaintext)
        ),
    )
    .expect("credentials");
    let source = LegacyCredentialStore::new(&home);
    let secret = source
        .load_blocking("HTTPS://GITHUB.COM/")
        .expect("read")
        .expect("token");
    assert_eq!(secret.expose_secret(), "legacy-secret");
    assert!(home.join(".key").exists());
    assert!(home.join("credentials.json").exists());
    std::fs::write(home.join("credentials.json"), "bad").expect("corrupt");
    let error = source.load_blocking("github.com").expect_err("corruption");
    assert_eq!(error.kind, ProviderErrorKind::CredentialStoreUnavailable);
    std::fs::remove_dir_all(home).expect("cleanup");
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
