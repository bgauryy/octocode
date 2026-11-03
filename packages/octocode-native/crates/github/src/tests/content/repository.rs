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
    assert!(!context.followed_rename("a", "b"));
    let sha = provider
        .transport
        .commit_sha("a", "b", "main", &context)
        .await
        .expect("followed");
    assert_eq!(sha, "f".repeat(40));
    assert!(context.followed_rename("A", "b"));
    let _ = provider
        .transport
        .commit_sha("x", "y", "main", &context)
        .await;
    assert!(!context.followed_rename("x", "y"));
    // Context clones share observations, independent requests do not.
    assert!(context.clone().followed_rename("a", "b"));
    assert!(!with_token("secret", Duration::from_secs(5), 1024).followed_rename("a", "b"));
}

/// A metadata read names the repository's canonical `full_name` for the
/// rest of its request only; another request has observed nothing.
#[tokio::test]
async fn a_metadata_read_names_the_canonical_repository_for_its_request() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        serde_json::json!({"default_branch":"main","full_name":"c/d"}),
    )
    .await;
    let provider = provider(&server).await;
    let context = with_token("secret", Duration::from_secs(5), 1 << 20);
    assert_eq!(context.read_full_name("a", "b"), None);
    provider
        .transport
        .repository_metadata("a", "b", &context)
        .await
        .expect("metadata");
    assert_eq!(
        context.clone().read_full_name("A", "b").as_deref(),
        Some("c/d")
    );
    assert_eq!(context.read_full_name("x", "y"), None);
    let other = with_token("secret", Duration::from_secs(5), 1024);
    assert_eq!(other.read_full_name("a", "b"), None);
}

#[tokio::test]
async fn concurrent_repository_metadata_reads_are_independent() {
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
    assert_eq!(hits.count(), 3, "each read sends GET /repos/o/r");
    // A later read also consults GitHub.
    transport
        .repository_metadata("o", "r", &context)
        .await
        .expect("metadata");
    assert_eq!(hits.count(), 4);
    // Another credential never reads this one's answer.
    let other = with_token("other", Duration::from_secs(5), 1 << 20);
    transport
        .repository_metadata("o", "r", &other)
        .await
        .expect("other credential");
    assert_eq!(hits.count(), 5);
}

#[tokio::test]
async fn a_failed_repository_read_is_reported_on_each_request() {
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
