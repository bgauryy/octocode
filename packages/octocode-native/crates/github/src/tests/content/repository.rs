//! Repository reads: renamed-repository redirects and shared metadata reads.
use super::*;

/// A followed `/repos/a/b/…` → `/repositories/<id>/…` redirect marks
/// `a/b` renamed; other repositories and plain reads stay unmarked.
#[tokio::test]
async fn a_followed_repository_redirect_marks_the_name_renamed() {
    let server = MockServer::builder().start().await;
    let moved = format!("{}/api/v3/repositories/7/commits/main", server.uri());
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", moved.as_str()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/7/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string("f".repeat(40)))
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/x/y/commits/main",
        422,
        serde_json::json!({"message":"No commit found for SHA: main"}),
    )
    .await;
    let provider = provider(&server).await;
    let context = with_token("secret", Duration::from_secs(5), 1 << 20);
    assert!(!provider.transport.followed_rename("a", "b"));
    let sha = provider
        .transport
        .commit_sha("a", "b", "main", &context)
        .await
        .expect("followed");
    assert_eq!(sha, "f".repeat(40));
    assert!(provider.transport.followed_rename("A", "b"));
    let _ = provider
        .transport
        .commit_sha("x", "y", "main", &context)
        .await;
    assert!(!provider.transport.followed_rename("x", "y"));
    // Clones share what was learned.
    assert!(provider.transport.clone().followed_rename("a", "b"));
}

#[tokio::test]
async fn concurrent_repository_metadata_reads_share_one_request() {
    let server = MockServer::builder().start().await;
    let repository = Delayed::new(
        Duration::from_millis(150),
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "full_name": "o/r", "name": "r", "html_url": "https://github.com/o/r",
            "default_branch": "main"
        })),
    );
    let hits = repository.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r"))
        .respond_with(repository)
        .mount(&server)
        .await;
    let endpoint = mock_endpoint(&server);
    let transport = anonymous_transport(&endpoint);
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    let (a, b, c) = tokio::join!(
        transport.repository_metadata("o", "r", &context),
        transport.repository_metadata("o", "r", &context),
        transport.repository("o", "r", &context),
    );
    assert_eq!(a.expect("metadata").default_branch, "main");
    assert_eq!(b.expect("metadata").full_name.as_deref(), Some("o/r"));
    assert_eq!(c.expect("repository").full_name, "o/r");
    assert_eq!(hits.count(), 1, "one GET /repos/o/r");
    // A later read in the same batch window is served from the memo.
    transport
        .repository_metadata("o", "r", &context)
        .await
        .expect("memo");
    assert_eq!(hits.count(), 1, "memoized");
    // Another credential never reads this one's answer.
    let other = with_token("other", Duration::from_secs(5), 1 << 20);
    transport
        .repository_metadata("o", "r", &other)
        .await
        .expect("other credential");
    assert_eq!(hits.count(), 2, "memo is per credential");
}

#[tokio::test]
async fn a_failed_repository_read_is_not_memoized() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"message":"Not Found"})),
        )
        .expect(2)
        .mount(&server)
        .await;
    let transport = anonymous_transport(&mock_endpoint(&server));
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    for _ in 0..2 {
        let error = transport
            .repository_metadata("o", "r", &context)
            .await
            .expect_err("missing");
        assert_eq!(error.reason, Some(ProviderErrorReason::RepositoryNotFound));
    }
}
