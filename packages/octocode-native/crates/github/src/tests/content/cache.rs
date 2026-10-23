//! Cached reads at a resolved commit: served without a request or revalidation.
use super::*;

/// Content at a commit SHA never changes: a cached read is served without
/// asking GitHub again, whether the caller named the SHA or a branch that
/// resolved to it (the ref resolution decides freshness).
#[tokio::test]
async fn a_cached_read_at_a_resolved_sha_is_not_revalidated() {
    let server = MockServer::builder().start().await;
    let sha = "2222222222222222222222222222222222222222";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":sha}),
    )
    .await;
    mount_file(
        &server,
        "/api/v3/repos/a/b/contents/pinned.rs",
        Duration::ZERO,
    )
    .await;
    mount_file(
        &server,
        "/api/v3/repos/a/b/contents/branch.rs",
        Duration::ZERO,
    )
    .await;
    let provider = keyed_provider(&server).await;
    let context = with_token("secret", Duration::from_secs(5), 1024);
    for _ in 0..2 {
        provider
            .get_file_bytes(&file_at("pinned.rs", sha), &context)
            .await
            .expect("pinned read");
    }
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        1
    );
    for _ in 0..2 {
        provider
            .get_file_bytes(&file_at("branch.rs", "main"), &context)
            .await
            .expect("branch read");
    }
    // One ref resolution (memoized) and one content read.
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/commits").await, 1);
    assert_eq!(
        requests_to(&server, "/api/v3/repos/a/b/contents/branch.rs").await,
        1
    );
}

/// A cached body at the resolved commit is served as is: no conditional
/// request, no 304 round trip.
#[tokio::test]
async fn a_cached_body_is_served_without_a_request() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":sha}),
    )
    .await;
    let cache = MemoryCache::default();
    *cache.0.lock().expect("cache lock") = Some(CachedContent {
        etag: Some("\"v1\"".into()),
        bytes: b"cached".to_vec().into(),
        resolved_ref: sha.into(),
        has_more: false,
    });
    let provider = GitHubProvider::new(provider(&server).await.transport, cache);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/x"))
        .respond_with(ResponseTemplate::new(304))
        .expect(0)
        .mount(&server)
        .await;
    let result = provider
        .get_file_bytes(
            &main_file("x", false),
            &with_token("secret", Duration::from_secs(2), 1024),
        )
        .await
        .expect("cached");
    assert_eq!(result.bytes, &b"cached"[..]);
    assert!(result.from_cache);
    assert_eq!(result.raw_response_bytes, 0);
    assert_eq!(result.resolved_ref, sha);
}

/// A directory listing at a full commit SHA is immutable: a second read is
/// served from the cache with no request, while a listing at a branch name
/// revalidates with its ETag.
#[tokio::test]
async fn listings_at_a_commit_sha_are_served_from_the_cache() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let listing = || {
        ResponseTemplate::new(200)
            .insert_header("etag", "\"tree\"")
            .set_body_json(serde_json::json!([
                {"name":"a.rs","path":"src/a.rs","type":"file","size":1}
            ]))
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo/contents/src"))
        .and(query_param("ref", sha))
        .respond_with(listing())
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo/contents/src"))
        .and(query_param("ref", "main"))
        .respond_with(listing())
        .expect(2)
        .mount(&server)
        .await;
    let base = provider(&server).await;
    let provider = GitHubProvider::new(base.transport, KeyedCache::default());
    let context = with_token("secret", Duration::from_secs(2), 1 << 16);
    for reference in [sha, sha, "main", "main"] {
        let listed = provider
            .repository_contents("acme", "repo", "src", reference, &context)
            .await
            .expect("listing");
        assert_eq!(listed.entries.len(), 1, "{reference}");
    }
}
