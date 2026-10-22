//! Contents, blobs, single-flight and the response cache for reads.
use super::*;

#[tokio::test]
async fn ghes_content_route_auth_and_decode() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/acme/repo/commits/main",
        200,
        serde_json::json!({"sha":sha}),
    )
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
            },
            &with_token("secret", Duration::from_secs(2), 1024),
        )
        .await
        .expect("content");
    assert_eq!(result.bytes, &b"hello\n"[..]);
    assert_eq!(result.etag.as_deref(), Some("\"v1\""));
    assert!(!result.from_cache);
}

async fn keyed_provider(server: &MockServer) -> GitHubProvider {
    GitHubProvider::new(provider(server).await.transport, KeyedCache::default())
}

fn file_at(path: &str, reference: &str) -> ContentRequest {
    ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: path.into(),
        reference: Some(reference.into()),
        force_refresh: false,
    }
}

async fn mount_file(server: &MockServer, path_pattern: &str, delay: Duration) {
    Mock::given(method("GET"))
        .and(path(path_pattern))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_delay(delay)
                .set_body_json(serde_json::json!({
                    "type":"file","encoding":"base64","content":STANDARD.encode("body\n")})),
        )
        .mount(server)
        .await;
}

async fn requests_to(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path().starts_with(route))
        .count()
}

/// Identical concurrent reads take turns on one flight: the first fetches,
/// the others read what it cached.
#[tokio::test]
async fn concurrent_identical_reads_send_one_request() {
    let server = MockServer::builder().start().await;
    let sha = "1111111111111111111111111111111111111111";
    mount_file(
        &server,
        "/api/v3/repos/a/b/contents/flight.rs",
        Duration::from_millis(150),
    )
    .await;
    let provider = keyed_provider(&server).await;
    let context = with_token("secret", Duration::from_secs(5), 1024);
    let request = file_at("flight.rs", sha);
    let (first, second) = tokio::join!(
        provider.get_file_content(&request, &context),
        provider.get_file_content(&request, &context),
    );
    let (first, second) = (first.expect("first"), second.expect("second"));
    assert_eq!(first.bytes, second.bytes);
    assert!(
        first.from_cache != second.from_cache,
        "one read is the cached one"
    );
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/contents").await, 1);
}

/// A waiter on an identical read's or ref resolution's flight stops on its
/// own cancellation, while the first request is still in flight.
#[tokio::test]
async fn a_cancelled_flight_waiter_stops_before_the_holder_finishes() {
    // The holders' answers outlast any runner's stall: each waiter must
    // answer while its holder's request is still at the server, and the
    // test then cancels the holder instead of waiting for the answer.
    const HELD: Duration = Duration::from_secs(60);
    let server = MockServer::builder().start().await;
    let sha = "4444444444444444444444444444444444444444";
    let arrived = Arc::new(tokio::sync::Notify::new());
    let held = |template: ResponseTemplate| Delayed {
        arrived: Some(Arc::clone(&arrived)),
        ..Delayed::new(HELD, template)
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/held.rs"))
        .respond_with(held(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_json(serde_json::json!({
                    "type":"file","encoding":"base64","content":STANDARD.encode("body\n")})),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/held-ref"))
        .respond_with(held(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"sha":sha})),
        ))
        .mount(&server)
        .await;
    let provider = keyed_provider(&server).await;
    let request = file_at("held.rs", sha);
    let call = |call: &'static str, context: RequestContext| {
        let (provider, request) = (&provider, &request);
        async move {
            match call {
                "content" => provider.get_file_content(request, &context).await.map(drop),
                _ => provider
                    .resolve_reference("a", "b", Some("held-ref"), false, &context)
                    .await
                    .map(drop),
            }
        }
    };
    for name in ["content", "ref"] {
        let holder = with_token("secret", Duration::from_secs(10), 1024);
        let release_holder = holder.budget.cancellation.clone();
        let mut holding = std::pin::pin!(call(name, holder));
        // The waiter joins once the holder's request reached the server,
        // so the holder has the turn; then it is cancelled while waiting.
        let waiting = async {
            arrived.notified().await;
            let context = with_token("secret", Duration::from_secs(10), 1024);
            let cancellation = context.budget.cancellation.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                cancellation.cancel();
            });
            call(name, context).await.expect_err("cancelled").kind
        };
        let waited = tokio::select! {
            kind = waiting => kind,
            answered = &mut holding => panic!("the {name} holder answered first: {answered:?}"),
        };
        assert_eq!(waited, ProviderErrorKind::Cancelled, "{name}");
        release_holder.cancel();
        assert_eq!(
            holding.await.expect_err("the holder is cancelled").kind,
            ProviderErrorKind::Cancelled,
            "{name}"
        );
    }
    // Neither waiter sent a request of its own.
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/contents").await, 1);
    assert_eq!(requests_to(&server, "/api/v3/repos/a/b/commits").await, 1);
}

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
            .get_file_content(&file_at("pinned.rs", sha), &context)
            .await
            .expect("pinned read");
    }
    assert_eq!(
        server.received_requests().await.unwrap_or_default().len(),
        1
    );
    for _ in 0..2 {
        provider
            .get_file_content(&file_at("branch.rs", "main"), &context)
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
        .get_file_content(
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

#[tokio::test]
async fn content_413_falls_back_to_parent_directory_and_blob() {
    let server = MockServer::builder().start().await;
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let blob = "89abcdef0123456789abcdef0123456789abcdef";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":commit}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/dir%2Flarge.txt",
        413,
        serde_json::json!({"message":"too large"}),
    )
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
    // The blob is read raw: its bytes, no base64 JSON body.
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/blobs/{blob}")))
        .and(header("accept", "application/vnd.github.raw"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes("large body"))
        .mount(&server)
        .await;
    let result = provider(&server)
        .await
        .get_file_content(
            &main_file("dir/large.txt", false),
            &with_token("secret", Duration::from_secs(2), 4096),
        )
        .await
        .expect("fallback");
    assert_eq!(result.bytes, &b"large body"[..]);
    assert_eq!(result.resolved_ref, commit);
}

#[tokio::test]
async fn content_encoding_none_for_large_file_fetches_blob() {
    // GitHub returns 200 with `encoding:"none"` and empty content for files
    // between 1 MB and 100 MB; the bytes must come from git/blobs/{sha}.
    let server = MockServer::builder().start().await;
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let blob = "89abcdef0123456789abcdef0123456789abcdef";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":commit}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big.txt",
        200,
        serde_json::json!({
            "type":"file","encoding":"none","content":"","size":2_000_000,"sha":blob
        }),
    )
    .await;
    // The blob is read raw: its bytes, no base64 JSON body.
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/blobs/{blob}")))
        .and(header("accept", "application/vnd.github.raw"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes("large body"))
        .mount(&server)
        .await;
    let result = provider(&server)
        .await
        .get_file_content(
            &main_file("big.txt", true),
            &with_token("secret", Duration::from_secs(2), 4096),
        )
        .await
        .expect("blob fallback");
    assert_eq!(result.bytes, &b"large body"[..]);
}

/// A file read asks for the raw media type: GitHub answers a file of any
/// size up to 100 MB with its own bytes (no base64 body, no second blob
/// read), and a binary one is refused with the blob SHA its ETag names.
#[tokio::test]
async fn a_file_read_takes_the_raw_bytes_in_one_request() {
    let server = MockServer::builder().start().await;
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let blob = "89abcdef0123456789abcdef0123456789abcdef";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":commit}),
    )
    .await;
    for (file, body) in [
        ("big.txt", &b"large body"[..]),
        ("image.bin", &b"\0binary"[..]),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{file}")))
            .and(header("accept", "application/vnd.github.raw"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/vnd.github.raw; charset=utf-8")
                    .insert_header("etag", format!("\"{blob}\"").as_str())
                    .set_body_bytes(body),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/git/blobs/"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let provider = provider(&server).await;
    let context = with_token("secret", Duration::from_secs(2), 4096);
    let result = provider
        .get_file_content(&main_file("big.txt", true), &context)
        .await
        .expect("raw read");
    assert_eq!(result.bytes, &b"large body"[..]);
    let binary = provider
        .get_file_content(&main_file("image.bin", true), &context)
        .await
        .expect_err("binary text read");
    assert_eq!(binary.reason, Some(crate::ProviderErrorReason::BinaryFile));
    assert!(binary.message.contains(blob), "{}", binary.message);
}

#[tokio::test]
async fn raw_content_keeps_binary_bytes_through_every_acquisition_and_cache_path() {
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let blob = "89abcdef0123456789abcdef0123456789abcdef";
    let bytes = b"\x89PNG\r\n\x1a\n\0binary";
    for route in ["inline", "blob", "directory"] {
        let server = MockServer::builder().start().await;
        let payload = match route {
            "inline" => {
                serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode(bytes),"sha":blob})
            }
            "blob" => {
                serde_json::json!({"type":"file","encoding":"none","content":"","size":bytes.len(),"sha":blob})
            }
            _ => serde_json::json!({"message":"too large"}),
        };
        mount_json(
            &server,
            "/api/v3/repos/a/b/contents/dir%2Fimage.png",
            if route == "directory" { 413 } else { 200 },
            payload,
        )
        .await;
        if route == "directory" {
            mount_json(
                &server,
                "/api/v3/repos/a/b/contents/dir",
                200,
                serde_json::json!([{"name":"image.png","type":"file","sha":blob}]),
            )
            .await;
        }
        if route != "inline" {
            Mock::given(method("GET"))
                .and(path(format!("/api/v3/repos/a/b/git/blobs/{blob}")))
                .and(header("accept", "application/vnd.github.raw"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(&bytes[..]))
                .mount(&server)
                .await;
        }
        let provider = provider(&server).await;
        let request = file_at("dir/image.png", commit);
        let context = with_token("secret", Duration::from_secs(2), 4096);
        let response = provider
            .get_file_bytes(&request, &context)
            .await
            .expect(route);
        assert_eq!(response.bytes, &bytes[..], "{route}");
        assert_eq!(response.resolved_ref, commit);
        assert!(!response.from_cache);
        let cached = provider
            .get_file_bytes(&request, &context)
            .await
            .expect("cached bytes");
        assert_eq!(cached.bytes, response.bytes);
        assert!(cached.from_cache);
        let error = provider
            .get_file_content(&request, &context)
            .await
            .expect_err("text reader refuses cached binary");
        assert_eq!(error.reason, Some(ProviderErrorReason::BinaryFile));
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            match route {
                "inline" => 1,
                "blob" => 2,
                _ => 3,
            }
        );
    }
}

#[tokio::test]
async fn content_directory_symlink_and_submodule_get_clear_errors() {
    let server = MockServer::builder().start().await;
    let commit = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/main",
        200,
        serde_json::json!({"sha":commit}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src",
        200,
        serde_json::json!([
            {"name":"lib.rs","path":"src/lib.rs","type":"file"}
        ]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/link",
        200,
        serde_json::json!({"type":"symlink","target":"src/lib.rs"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/vendor",
        200,
        serde_json::json!({"type":"submodule","submodule_git_url":"https://x/y.git"}),
    )
    .await;
    let provider = provider(&server).await;
    let read = |p: &'static str| {
        let provider = &provider;
        async move {
            provider
                .get_file_content(
                    &main_file(p, true),
                    &with_token("secret", Duration::from_secs(2), 4096),
                )
                .await
                .expect_err("not a file")
        }
    };
    let dir = read("src").await;
    assert_eq!(dir.kind, ProviderErrorKind::Validation);
    assert!(dir.message.contains("is a directory"), "{}", dir.message);
    assert_eq!(
        dir.reason,
        Some(super::ProviderErrorReason::PathIsDirectory)
    );
    let link = read("link").await;
    assert!(link.message.contains("symlink"), "{}", link.message);
    assert!(link.message.contains("src/lib.rs"), "{}", link.message);
    let module = read("vendor").await;
    assert!(module.message.contains("submodule"), "{}", module.message);
}

/// Key-aware cache so ref memo and body entries coexist.
#[derive(Default)]
struct KeyedCache(Mutex<std::collections::HashMap<String, CachedContent>>);

impl ConditionalCache for KeyedCache {
    fn get<'a>(
        &'a self,
        _: &'a Partition,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<CachedContent>> + Send + 'a>>
    {
        Box::pin(async move { self.0.lock().expect("cache lock").get(key).cloned() })
    }
    fn put<'a>(
        &'a self,
        _: &'a Partition,
        key: String,
        value: CachedContent,
        _: Retention,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.0.lock().expect("cache lock").insert(key, value);
        })
    }
}

#[tokio::test]
async fn default_branch_resolves_with_one_sha_request_and_is_memoized_per_batch() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    // No repository-metadata round trip: HEAD resolves the default branch.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme/repo/commits/HEAD"))
        .and(header("accept", "application/vnd.github.sha"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .expect(1)
        .mount(&server)
        .await;
    for name in ["a.rs", "b.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/acme/repo/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "type":"file","encoding":"base64","content":STANDARD.encode("x\n")
            })))
            .expect(1)
            .mount(&server)
            .await;
    }
    let base = provider(&server).await;
    let provider = GitHubProvider::new(base.transport, KeyedCache::default());
    let context = with_token("secret", Duration::from_secs(2), 1024);
    for name in ["a.rs", "b.rs", "a.rs"] {
        let result = provider
            .get_file_content(
                &ContentRequest {
                    owner: "acme".into(),
                    repo: "repo".into(),
                    path: name.into(),
                    reference: None,
                    force_refresh: false,
                },
                &context,
            )
            .await
            .expect("content");
        assert_eq!(result.resolved_ref, sha);
        assert_eq!(result.bytes, &b"x\n"[..]);
    }
}

#[tokio::test]
async fn a_resolved_full_sha_is_cached_per_credential() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .expect(2)
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
        let resolved = transport
            .commit_sha("o", "r", sha, &context(token))
            .await
            .expect("resolved");
        assert_eq!(resolved, sha);
    }
}

/// A ref GitHub cannot resolve (422 "No commit found") is not-found input
/// with a typed reason, not a malformed request; a 404 stays the repository.
#[tokio::test]
async fn missing_ref_is_not_found_with_a_ref_reason() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/commits/nope",
        422,
        serde_json::json!({"message": "No commit found for SHA: nope"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/gone/commits/main",
        404,
        serde_json::json!({"message": "Not Found"}),
    )
    .await;
    let provider = provider(&server).await;
    let context = with_token("secret", Duration::from_secs(5), 1 << 20);
    let error = provider
        .transport
        .commit_sha("o", "r", "nope", &context)
        .await
        .expect_err("missing ref");
    assert_eq!(error.kind, ProviderErrorKind::NotFound, "{error:?}");
    assert_eq!(error.reason, Some(ProviderErrorReason::RefNotFound));
    assert_eq!(error.status, Some(422));
    let error = provider
        .transport
        .commit_sha("o", "gone", "main", &context)
        .await
        .expect_err("missing repository");
    assert_eq!(error.reason, Some(ProviderErrorReason::RepositoryNotFound));
}

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

/// A release tag is a tag ref: a branch of the same name (which
/// `commits/{name}` would resolve) is not one, and a tag name keeps its
/// `/` as path segments.
#[tokio::test]
async fn a_tag_exists_only_as_a_tag_ref() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/ref/tags/pkg/v1.2.3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "ref": "refs/tags/pkg/v1.2.3", "object": {"sha": "1".repeat(40), "type": "tag"}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/git/ref/tags/1.2.3"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(serde_json::json!({"message": "Not Found"})),
        )
        .mount(&server)
        .await;
    // The branch `1.2.3` resolves as a commit, but is no tag.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/1.2.3"))
        .respond_with(ResponseTemplate::new(200).set_body_string("2".repeat(40)))
        .mount(&server)
        .await;
    let transport = anonymous_transport(&mock_endpoint(&server));
    let context = RequestContext::with_timeout(Duration::from_secs(5), 1 << 20);
    assert!(
        transport
            .tag_exists("a", "b", "pkg/v1.2.3", &context)
            .await
            .expect("tag read")
    );
    assert!(
        !transport
            .tag_exists("a", "b", "1.2.3", &context)
            .await
            .expect("tag read")
    );
}
