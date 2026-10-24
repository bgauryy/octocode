//! Materialized trees: the layout Octocode owns, content identity reuse,
//! and no dating request.
use super::*;

/// octocode-mcp 19.x rewrites `tmp/tree/<owner>/<repo>/<sha>/`; a
/// materialize never shares it or the size-only v2 layout. Neither old cache
/// is treated as a content-verified snapshot or modified.
#[tokio::test]
async fn materialize_never_trusts_a_foreign_directory() {
    let server = tree_server(1).await;
    let workspace = Workspace::new();
    let legacy = workspace.home.join("tmp/tree/a/b").join(SHA);
    std::fs::create_dir_all(&legacy).expect("legacy dir");
    std::fs::write(legacy.join("one.rs"), "prod 19.x content").expect("legacy file");
    let old = workspace.home.join("tmp/materialize/v2/a/b").join(SHA);
    let own = workspace.home.join("tmp/materialize/v3/a/b").join(SHA);
    std::fs::create_dir_all(old.join("stale")).expect("foreign dir");
    std::fs::write(old.join("one.rs"), "// foreign").expect("foreign file");
    std::fs::write(old.join("stale/ghost.rs"), "not at this commit").expect("foreign extra");

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
    let local =
        std::path::PathBuf::from(data["location"]["localPath"].as_str().expect("localPath"));
    assert_eq!(local, own, "{data}");
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
    assert_eq!(
        std::fs::read_to_string(old.join("stale/ghost.rs")).unwrap(),
        "not at this commit"
    );
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

/// Matching content is reused; a same-size edit on disk is written again.
#[tokio::test]
async fn materialize_reuses_only_recorded_content_identity() {
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
    for name in ["one.rs", "two.rs"] {
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
    let local = std::path::PathBuf::from(
        row_data(&first)["location"]["localPath"]
            .as_str()
            .expect("localPath"),
    );
    std::fs::write(local.join("two.rs"), "modified!!").expect("same-size tamper");
    // An old mtime on the reusable file shows whether it was rewritten.
    let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    std::fs::File::options()
        .write(true)
        .open(local.join("one.rs"))
        .expect("open one")
        .set_modified(old)
        .expect("age one");
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
        std::fs::metadata(local.join("one.rs"))
            .and_then(|meta| meta.modified())
            .expect("mtime"),
        old,
        "a file matching its recorded identity is reused, not rewritten"
    );
    assert_eq!(
        std::fs::read_to_string(local.join("two.rs")).expect("two"),
        "// two.rs\n"
    );
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
    let data = row_data(&outcome);
    assert!(updated(&data["files"][0]).is_empty(), "{data}");
    assert!(graphql_requests(&server).await.is_empty());
    runtime.close().await;
}
