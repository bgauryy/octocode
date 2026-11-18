//! File payloads: raw bytes, the blob fallbacks past the contents cap, and non-file entries.
use super::*;
use octocode_test_support::json_fixture;

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
    mount_trees(&server, "a", "b", commit, &["dir/large.txt"]).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/dir%2Flarge.txt",
        413,
        json_fixture(r#"{"message": "too large"}"#),
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
            &main_file("dir/large.txt"),
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
    mount_trees(&server, "a", "b", commit, &["big.txt"]).await;
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
            &main_file("big.txt"),
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
    mount_trees(&server, "a", "b", commit, &["big.txt", "image.bin"]).await;
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
        .get_file_bytes(&main_file("big.txt"), &context)
        .await
        .expect("raw read");
    assert_eq!(result.bytes, &b"large body"[..]);
    assert_eq!(result.blob.as_deref(), Some(blob));
    let binary = provider
        .get_file_bytes(&main_file("image.bin"), &context)
        .await
        .expect("binary bytes");
    assert_eq!(binary.bytes, &b"\0binary"[..]);
    assert_eq!(binary.blob.as_deref(), Some(blob));
}

#[tokio::test]
async fn raw_content_keeps_binary_bytes_through_every_acquisition_path() {
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
            _ => json_fixture(r#"{"message": "too large"}"#),
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
        mount_trees(&server, "a", "b", commit, &["dir/image.png"]).await;
        let provider = provider(&server).await;
        let request = file_at("dir/image.png", commit);
        let context = with_token("secret", Duration::from_secs(2), 4096);
        let response = provider
            .get_file_bytes(&request, &context)
            .await
            .expect(route);
        assert_eq!(response.bytes, &bytes[..], "{route}");
        assert_eq!(response.resolved_ref, commit);
        let repeated = provider
            .get_file_bytes(&request, &context)
            .await
            .expect("fresh bytes");
        assert_eq!(repeated.bytes, response.bytes);
        assert_eq!(response.blob.as_deref(), Some(blob), "{route}");
        // Every request but the tree reads that say the path is no link.
        let asked = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| !request.url.path().contains("/git/trees/"))
            .count();
        assert_eq!(
            asked,
            match route {
                "inline" => 2,
                "blob" => 4,
                _ => 6,
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
        json_fixture(r#"[{"name": "lib.rs", "path": "src/lib.rs", "type": "file"}]"#),
    )
    .await;
    // A link to a file: GitHub's contents read of it would answer the
    // target's bytes as `type: "file"`; the tree names it a link and its
    // blob its target, so its contents are never read.
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/link",
        200,
        serde_json::json!({"type":"file","encoding":"base64","content":STANDARD.encode("pub fn lib() {}\n")}),
    )
    .await;
    octocode_test_support::mock_http::GitRepository::new()
        .file("src/lib.rs", "pub fn lib() {}\n")
        .link("link", "src/lib.rs")
        .mount_trees(&server, "a", "b", commit)
        .await;
    // A submodule is a `commit` entry of its parent's tree.
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{commit}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sha": commit, "truncated": false, "tree": [
                {"path":"src","mode":"040000","type":"tree","sha":"1".repeat(40)},
                {"path":"link","mode":"120000","type":"blob","sha":octocode_test_support::mock_http::GitRepository::blob_sha("link")},
                {"path":"vendor","mode":"160000","type":"commit","sha":"2".repeat(40)}
            ]})))
        .with_priority(1)
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/vendor",
        200,
        json_fixture(
            r#"{
                "type": "submodule",
                "submodule_git_url": "https://x/y.git"
            }"#,
        ),
    )
    .await;
    let provider = provider(&server).await;
    let read = |p: &'static str| {
        let provider = &provider;
        async move {
            provider
                .get_file_bytes(
                    &main_file(p),
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
    assert_eq!(
        requests_to(&server, "/api/v3/repos/a/b/contents/link").await,
        0
    );
    let module = read("vendor").await;
    assert!(module.message.contains("submodule"), "{}", module.message);
}
