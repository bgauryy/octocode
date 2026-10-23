//! File payloads: raw bytes, the blob fallbacks past the contents cap, and non-file entries.
use super::*;

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
        .get_file_bytes(
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
        .get_file_bytes(
            &main_file("big.txt", true),
            &with_token("secret", Duration::from_secs(2), 4096),
        )
        .await
        .expect("blob fallback");
    assert_eq!(result.bytes, &b"large body"[..]);
}

/// A file read asks for the raw media type: GitHub answers a file of any
/// size up to 100 MB with its own bytes (no base64 body, no second blob
/// read), and every file comes back as its bytes with the blob SHA its ETag
/// names: whether they are text is the caller's rule.
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
        .get_file_bytes(&main_file("big.txt", true), &context)
        .await
        .expect("raw read");
    assert_eq!(result.bytes, &b"large body"[..]);
    assert_eq!(result.blob.as_deref(), Some(blob));
    let binary = provider
        .get_file_bytes(&main_file("image.bin", true), &context)
        .await
        .expect("binary bytes");
    assert_eq!(binary.bytes, &b"\0binary"[..]);
    assert_eq!(binary.blob.as_deref(), Some(blob));
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
        assert_eq!(response.blob.as_deref(), Some(blob), "{route}");
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
                .get_file_bytes(
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
