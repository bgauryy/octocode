//! Search and history reads: caching, credentials and SSO-withheld results.
use super::*;

fn code_search_request() -> CodeSearchRequest {
    CodeSearchRequest {
        include_fragments: false,
        query: "secret org:acme".into(),
        page: 1,
        per_page: 10,
    }
}

/// Production shape: one process-wide transport with an anonymous resolver,
/// the credential resolved per request. A cached search page fetched with
/// one token must never answer a request made with another.
#[tokio::test]
async fn code_search_cache_is_scoped_to_the_request_credential() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 0, "incomplete_results": false, "items": []
        })))
        .expect(3)
        .mount(&server)
        .await;
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = |token: Option<&str>| {
        RequestContext::new(
            RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
            token.map(|token| ResolvedCredential::new(token, CredentialSource::Storage)),
        )
    };
    for token in [Some("alice"), Some("alice"), Some("bob"), None] {
        transport
            .search_code(&code_search_request(), &context(token))
            .await
            .expect("search");
    }
}

/// `confirm_user` reads `/users/{login}` through the response cache: one
/// request per credential and login; a 404 is the typed missing user, and a
/// failure of any other kind keeps its own kind.
#[tokio::test]
async fn confirm_user_is_cached_per_credential_and_types_a_missing_login() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/dev"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"login": "dev"})))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/ghost"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"message": "Not Found"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/flaky"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = |token: &str| {
        RequestContext::new(
            RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
            Some(ResolvedCredential::new(token, CredentialSource::Storage)),
        )
    };
    // alice twice (one request), bob once (his own partition): two requests.
    for token in ["alice", "alice", "bob"] {
        transport
            .confirm_user("dev", &context(token))
            .await
            .expect("user exists");
    }
    let missing = transport
        .confirm_user("ghost", &context("alice"))
        .await
        .expect_err("missing user");
    assert_eq!(missing.kind, ProviderErrorKind::NotFound);
    assert_eq!(missing.reason, Some(ProviderErrorReason::UserNotFound));
    assert!(missing.message.contains("ghost"), "{missing:?}");
    let flaky = transport
        .confirm_user("flaky", &context("alice"))
        .await
        .expect_err("server failure");
    assert_eq!(flaky.reason, None, "{flaky:?}");
    assert_ne!(flaky.kind, ProviderErrorKind::NotFound);
}

#[tokio::test]
async fn incomplete_code_search_pages_are_not_cached() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 0, "incomplete_results": true, "items": []
        })))
        .expect(2)
        .mount(&server)
        .await;
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = || with_token("t", Duration::from_secs(5), 1 << 20);
    for _ in 0..2 {
        let page = transport
            .search_code(&code_search_request(), &context())
            .await
            .expect("search");
        assert!(page.incomplete_results);
    }
}

/// Repository, issue and commit searches use the code-search cache policy:
/// an identical repeat is a cache hit, so it sends no request and does not
/// wait out the search spacing the first request reserved.
#[tokio::test]
async fn repeated_searches_are_served_from_cache_without_spacing() {
    let server = MockServer::builder().start().await;
    let page = serde_json::json!({"total_count": 0, "incomplete_results": false, "items": []});
    for route in [
        "/api/v3/search/code",
        "/api/v3/search/repositories",
        "/api/v3/search/issues",
        "/api/v3/search/commits",
    ] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(page.clone()))
            .expect(1)
            .mount(&server)
            .await;
    }
    // Wide enough that twelve cache hits finish in a quarter of it.
    let spacing = Duration::from_millis(600);
    let budget = Arc::new(GitHubBudget::with_config(ExecutorConfig {
        search_spacing: spacing,
        ..ExecutorConfig::relaxed()
    }));
    let (mut transport, _) = executor_transport(&server, budget, short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = || with_token("t", Duration::from_secs(30), 1 << 20);
    let repositories = RepositorySearchRequest {
        query: "ripgrep".into(),
        sort: None,
        page: 1,
        per_page: 5,
    };
    let history = HistoryRequest {
        query: "repo:o/r is:pr is:merged".into(),
        page: 1,
        per_page: 10,
        sort: None,
        order: None,
    };
    // Cold: the four searches share one spacing lane, so three of them wait.
    transport
        .search_code(&code_search_request(), &context())
        .await
        .expect("code");
    transport
        .search_repositories(&repositories, &context())
        .await
        .expect("repositories");
    transport
        .search_issues(&history, &context())
        .await
        .expect("issues");
    transport
        .search_commits(&history, &context())
        .await
        .expect("commits");
    let started = std::time::Instant::now();
    for _ in 0..3 {
        transport
            .search_code(&code_search_request(), &context())
            .await
            .expect("code");
        transport
            .search_repositories(&repositories, &context())
            .await
            .expect("repositories");
        transport
            .search_issues(&history, &context())
            .await
            .expect("issues");
        transport
            .search_commits(&history, &context())
            .await
            .expect("commits");
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < spacing / 4,
        "cache hits must not wait for search spacing: {elapsed:?}"
    );
}

#[tokio::test]
async fn incomplete_repository_and_history_search_pages_are_not_cached() {
    let server = MockServer::builder().start().await;
    for route in ["/api/v3/search/repositories", "/api/v3/search/issues"] {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 0, "incomplete_results": true, "items": []
            })))
            .expect(2)
            .mount(&server)
            .await;
    }
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = || with_token("t", Duration::from_secs(5), 1 << 20);
    for _ in 0..2 {
        let repositories = transport
            .search_repositories(
                &RepositorySearchRequest {
                    query: "ripgrep".into(),
                    sort: None,
                    page: 1,
                    per_page: 5,
                },
                &context(),
            )
            .await
            .expect("repositories");
        assert!(repositories.incomplete_results);
        let issues = transport
            .search_issues(
                &HistoryRequest {
                    query: "repo:o/r".into(),
                    page: 1,
                    per_page: 10,
                    sort: None,
                    order: None,
                },
                &context(),
            )
            .await
            .expect("issues");
        assert!(issues.incomplete_results);
    }
}

#[tokio::test]
async fn history_reads_revalidate_and_pinned_commits_are_served_from_cache() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls/1"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .with_priority(1)
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls/1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_json(serde_json::json!({"number": 1})),
        )
        .with_priority(2)
        .expect(1)
        .mount(&server)
        .await;
    let sha = "a".repeat(40);
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/commits/{sha}")))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"c1\"")
                .set_body_json(serde_json::json!({"sha": sha})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = || with_token("t", Duration::from_secs(5), 1 << 20);
    for _ in 0..3 {
        let item = transport
            .history_item(&["repos", "o", "r", "pulls", "1"], &[], &context())
            .await
            .expect("pull request");
        assert_eq!(item.value["number"], 1);
        let commit = transport
            .history_item(&["repos", "o", "r", "commits", &sha], &[], &context())
            .await
            .expect("commit");
        assert_eq!(commit.value["sha"], sha.as_str());
    }
}

/// `partial-results` names the organizations that withheld results.
#[tokio::test]
async fn partial_sso_results_name_the_withheld_organizations() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "x-github-sso",
                    "partial-results; organizations=21955855,20582480",
                )
                .set_body_json(serde_json::json!({"items": []})),
        )
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let page = get_status(&transport, &endpoint, &["search", "issues"])
        .await
        .expect("ok");
    assert_eq!(page.sso_partial_organizations(), vec![21955855, 20582480]);
    let plain = ResponsePage {
        status: 200,
        headers: reqwest::header::HeaderMap::new(),
        body: bytes::Bytes::new(),
        next: None,
    };
    assert!(plain.sso_partial_organizations().is_empty());
}

/// A search the token only partly covered says so, and is not
/// cached (a cached copy would hide the authorization that fixes it).
#[tokio::test]
async fn a_search_partly_withheld_by_saml_names_the_organizations_and_is_not_cached() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "x-github-sso",
                    "partial-results; organizations=21955855,20582480",
                )
                .set_body_json(serde_json::json!({
                    "total_count": 0, "incomplete_results": false, "items": []
                })),
        )
        .expect(4)
        .mount(&server)
        .await;
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    for _ in 0..2 {
        let page = transport
            .search_code(&code_search_request(), &context)
            .await
            .expect("code search");
        assert_eq!(page.sso_partial_organizations, vec![21955855, 20582480]);
    }
    for _ in 0..2 {
        let history = transport
            .search_issues(
                &HistoryRequest {
                    query: "repo:o/r".into(),
                    page: 1,
                    per_page: 10,
                    sort: None,
                    order: None,
                },
                &context,
            )
            .await
            .expect("issues");
        assert!(
            history
                .warnings
                .iter()
                .any(|warning| warning.contains("SAML")),
            "{:?}",
            history.warnings
        );
    }
}

/// Counts the store reads a search makes.
#[derive(Default)]
struct CountingCache {
    inner: PartitionedCache,
    gets: std::sync::atomic::AtomicUsize,
}

impl ConditionalCache for CountingCache {
    fn get<'a>(
        &'a self,
        partition: &'a Partition,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<CachedContent>> + Send + 'a>>
    {
        self.gets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.get(partition, key)
    }
    fn put<'a>(
        &'a self,
        partition: &'a Partition,
        key: String,
        value: CachedContent,
        retention: Retention,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        self.inner.put(partition, key, value, retention)
    }
}

/// A search its caller just found uncached is sent without reading the
/// store again, and stored for the next one; two identical searches in
/// flight together still send once (the second reads what the first stored).
#[tokio::test]
async fn a_search_known_to_miss_reads_the_store_once() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({
                    "total_count": 0, "incomplete_results": false, "items": []
                }))
                .set_delay(Duration::from_millis(100)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let cache = Arc::new(CountingCache::default());
    transport.cache = cache.clone();
    let context = RequestContext::new(
        RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
        None,
    );
    let request = code_search_request();
    let gets = || cache.gets.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        transport
            .cached_code_search(&request, &context)
            .await
            .expect("lookup")
            .is_none()
    );
    let (first, second) = tokio::join!(
        transport.search_code_uncached(&request, &context),
        transport.search_code_uncached(&request, &context),
    );
    first.expect("first search");
    second.expect("second search");
    // The lookup, then the waiter's read of what the sender stored.
    assert_eq!(gets(), 2);
    assert!(
        transport
            .cached_code_search(&request, &context)
            .await
            .expect("lookup")
            .is_some()
    );
}
