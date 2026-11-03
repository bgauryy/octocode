//! Fresh search and history reads, credentials and SSO-withheld results.
use super::*;

mod sso;

fn code_search_request() -> CodeSearchRequest {
    CodeSearchRequest {
        include_fragments: false,
        query: "secret org:acme".into(),
        page: 1,
        per_page: 10,
    }
}

/// Production shape: one process-wide transport with an anonymous resolver,
/// the credential resolved per request. Each call asks GitHub using that credential.
#[tokio::test]
async fn code_search_uses_each_request_credential() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 0, "incomplete_results": false, "items": []
        })))
        .expect(4)
        .mount(&server)
        .await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
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

/// `confirm_user` reads `/users/{login}` for each call; a 404 is the typed missing user, and a
/// failure of any other kind keeps its own kind.
#[tokio::test]
async fn confirm_user_reads_fresh_and_types_a_missing_login() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/users/dev"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"login": "dev"})))
        .expect(3)
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
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = |token: &str| {
        RequestContext::new(
            RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
            Some(ResolvedCredential::new(token, CredentialSource::Storage)),
        )
    };
    // Every call independently checks the account, including repeats.
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
async fn incomplete_code_search_pages_keep_partiality_on_every_read() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "total_count": 0, "incomplete_results": true, "items": []
        })))
        .expect(2)
        .mount(&server)
        .await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = || with_token("t", Duration::from_secs(5), 1 << 20);
    for _ in 0..2 {
        let page = transport
            .search_code(&code_search_request(), &context())
            .await
            .expect("search");
        assert!(page.incomplete_results);
    }
}

#[tokio::test]
async fn incomplete_repository_and_history_pages_keep_partiality_on_every_read() {
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
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
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
async fn history_reads_fetch_mutable_and_pinned_items_every_time() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls/1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_json(serde_json::json!({"number": 1})),
        )
        .with_priority(2)
        .expect(3)
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
        .expect(3)
        .mount(&server)
        .await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
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
