//! Materialized trees: the layout Octocode owns, the snapshot as the only
//! store of each body, and no dating request.
use super::*;

/// octocode-mcp 19.x rewrites `tmp/tree/<owner>/<repo>/<sha>/`; a
/// materialize never shares it or an earlier snapshot layout (the size-only
/// v2, the skip-less v3 manifest). No old cache is treated as a
/// content-verified snapshot or modified.
#[tokio::test]
async fn materialize_never_trusts_a_foreign_directory() {
    let server = tree_server(1).await;
    let workspace = Workspace::new();
    let legacy = workspace.home.join("tmp/tree/a/b").join(SHA);
    std::fs::create_dir_all(&legacy).expect("legacy dir");
    std::fs::write(legacy.join("one.rs"), "prod 19.x content").expect("legacy file");
    let layouts = workspace.home.join("tmp/materialize");
    let foreign = ["v2", "v3"].map(|layout| layouts.join(layout).join("a/b").join(SHA));
    for old in &foreign {
        std::fs::create_dir_all(old.join("stale")).expect("foreign dir");
        std::fs::write(old.join("one.rs"), "// foreign").expect("foreign file");
        std::fs::write(old.join("stale/ghost.rs"), "not at this commit").expect("foreign extra");
    }

    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b", "materialize": true}),
    )
    .await
    .expect("materialize");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    let local = std::path::PathBuf::from(data["location"]["path"].as_str().expect("path"));
    // The snapshot sits at `<layout>/owner/repo/commit` in a layout of its own.
    let own = local.clone();
    let layout = own
        .strip_prefix(&layouts)
        .ok()
        .and_then(|rest| rest.components().next())
        .map(|layout| layouts.join(layout))
        .unwrap_or_else(|| panic!("snapshot outside {}: {data}", layouts.display()));
    assert_eq!(own, layout.join("a/b").join(SHA), "{data}");
    assert!(
        !foreign.contains(&own),
        "a foreign layout was reused: {data}"
    );
    assert_eq!(data["location"]["coverage"]["state"], "complete", "{data}");
    assert_eq!(
        std::fs::read_to_string(local.join("one.rs")).expect("one"),
        "// one.rs\n"
    );
    assert_eq!(
        std::fs::read_to_string(local.join("two.rs")).expect("two"),
        "// two.rs\n"
    );
    assert!(!local.join("stale").exists(), "a foreign file survived");
    for old in &foreign {
        assert_eq!(
            std::fs::read_to_string(old.join("stale/ghost.rs")).unwrap(),
            "not at this commit"
        );
        assert_eq!(
            std::fs::read_to_string(old.join("one.rs")).unwrap(),
            "// foreign"
        );
    }
    assert_eq!(
        std::fs::read_to_string(legacy.join("one.rs")).expect("legacy kept"),
        "prod 19.x content",
        "another writer's cache is not touched"
    );
    // The manifest sits beside the checkout, so the checkout holds only
    // repository files.
    let manifest = own.with_file_name(format!("{SHA}.manifest.json"));
    let recorded: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).expect("manifest")).expect("json");
    assert_eq!(
        recorded["files"].as_object().map(serde_json::Map::len),
        Some(2)
    );
    for name in ["one.rs", "two.rs"] {
        use sha2::{Digest, Sha256};
        let expected = std::fs::read(local.join(name)).expect("snapshot file");
        assert_eq!(recorded["files"][name]["bytes"], json!(expected.len()));
        assert_eq!(
            recorded["files"][name]["sha256"],
            json!(hex::encode(Sha256::digest(expected)))
        );
    }
    runtime.close().await;
}

/// A repeated materialize restores every file: the snapshot is the only
/// store of a body, so a copy that still matches its manifest identity is
/// kept untouched and not read, and a same-size edit on disk is read from
/// the provider again and overwritten with the commit's bytes.
#[tokio::test]
async fn a_second_materialize_restores_every_file_reading_only_the_changed_one() {
    let server = MockServer::builder().start().await;
    mount_head_at_sha(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":10,"sha":"1".repeat(40)},
            {"name":"two.rs","path":"two.rs","type":"file","size":10,"sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    // one.rs stays intact on disk, so only the first materialize reads it;
    // two.rs is edited on disk, and no saved answer keeps its body, so the
    // second materialize reads it again.
    for (name, reads) in [("one.rs", 1), ("two.rs", 2)] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode(format!("// {name}\n")),
                "size": 10,
                "sha": "a".repeat(40),
                "path": name
            })))
            .expect(reads)
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let query =
        json!({"operation":"tree","owner": "a", "repo": "b", "ref": SHA, "materialize": true});
    let first = call(&runtime, "ghStructure", query.clone())
        .await
        .expect("first");
    let local =
        std::path::PathBuf::from(row_data(&first)["location"]["path"].as_str().expect("path"));
    // An atomic write replaces the file (a new inode): an untouched file
    // keeps its own, a tampered one gets a new one.
    #[cfg(unix)]
    let inode = |name: &str| {
        std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(local.join(name)).expect("file"))
    };
    #[cfg(unix)]
    let (one_before, two_before) = (inode("one.rs"), inode("two.rs"));
    std::fs::write(local.join("two.rs"), "modified!!").expect("same-size tamper");
    #[cfg(unix)]
    let two_tampered = inode("two.rs");
    let second = call(&runtime, "ghStructure", query).await.expect("second");
    assert_eq!(
        row_status(&second),
        "success",
        "{}",
        second.structured_content
    );
    assert_eq!(
        row_data(&second)["location"]["coverage"]["state"],
        "complete"
    );
    assert_eq!(
        std::fs::read_to_string(local.join("one.rs")).expect("one"),
        "// one.rs\n"
    );
    assert_eq!(
        std::fs::read_to_string(local.join("two.rs")).expect("two"),
        "// two.rs\n"
    );
    #[cfg(unix)]
    {
        assert_eq!(
            inode("one.rs"),
            one_before,
            "an identical file was rewritten"
        );
        assert_ne!(inode("two.rs"), two_tampered, "the tampered file was kept");
        let _ = two_before;
    }
    server.verify().await;
    runtime.close().await;
}

/// Materialize writes files for local tools; its listing stays undated.
#[tokio::test]
async fn materialize_sends_no_date_request() {
    let server = tree_server(1).await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b", "materialize": true}),
    )
    .await
    .expect("materialize");
    assert_eq!(row_status(&outcome), "success");
    assert!(graphql_requests(&server).await.is_empty());
    runtime.close().await;
}

/// A credential-named file is never written to disk, and never read from the
/// provider either; the row names it and stays complete (nothing a local
/// tool may read is missing).
#[tokio::test]
async fn materialize_skips_credential_named_files_without_reading_them() {
    let server = MockServer::builder().start().await;
    mount_head_at_sha(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":10,"sha":"1".repeat(40)},
            {"name":".env","path":".env","type":"file","size":12,"sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/one.rs"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode("// one.rs\n"),
            "size": 10, "sha": "a".repeat(40), "path": "one.rs"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/.env"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode("TOKEN=hunter2\n"),
            "size": 12, "sha": "b".repeat(40), "path": ".env"
        })))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b", "ref": SHA, "materialize": true}),
    )
    .await
    .expect("materialize");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    let local = std::path::PathBuf::from(data["location"]["path"].as_str().expect("path"));
    assert!(local.join("one.rs").exists(), "{data}");
    assert!(
        !local.join(".env").exists(),
        "a credential file was written"
    );
    assert_eq!(data["location"]["skipped"], json!([".env"]), "{data}");
    assert_eq!(data["location"]["coverage"]["state"], "complete", "{data}");
    let text = data.to_string();
    assert!(text.contains("credential"), "the row says why: {data}");
    assert!(!text.contains("hunter2"), "{data}");
    server.verify().await;
    runtime.close().await;
}
