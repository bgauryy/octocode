// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::panic)]

//! ghStructure: materialize layout ownership, refs and languages listings.
use crate::support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// A repository `a/b` whose default branch `main` is at [`SHA`] with
/// `one.rs` and `two.rs`; each file read is expected `reads` times.
async fn tree_server(reads: u64) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
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
            .expect(reads)
            .mount(&server)
            .await;
    }
    server
}

fn github(workspace: &Workspace, server: &MockServer) -> octocode_native::runtime::ToolRuntime {
    workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))])
}

/// A3: octocode-mcp 19.x rewrites `tmp/tree/<owner>/<repo>/<sha>/`; a
/// materialize never shares it, and a directory in its own layout that has
/// no manifest (foreign or partial) is cleared, not trusted.
#[tokio::test]
async fn materialize_never_trusts_a_foreign_directory() {
    let server = tree_server(1).await;
    let workspace = Workspace::new();
    let legacy = workspace.home.join("tmp/tree/a/b").join(SHA);
    std::fs::create_dir_all(&legacy).expect("legacy dir");
    std::fs::write(legacy.join("one.rs"), "prod 19.x content").expect("legacy file");
    let own = workspace.home.join("tmp/materialize/v2/a/b").join(SHA);
    std::fs::create_dir_all(own.join("stale")).expect("foreign dir");
    std::fs::write(own.join("one.rs"), "// foreign").expect("foreign file");
    std::fs::write(own.join("stale/ghost.rs"), "not at this commit").expect("foreign extra");

    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "materialize": true}),
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
    assert_eq!(data["location"]["complete"], true, "{data}");
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
        recorded["files"],
        json!({"one.rs": 10, "two.rs": 10}),
        "{recorded}"
    );
    runtime.close().await;
}

/// A file the manifest recorded at its listed size is reused; one changed on
/// disk is written again.
#[tokio::test]
async fn materialize_reuses_only_recorded_files_at_their_size() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(&server)
        .await;
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
    let query = json!({"owner": "a", "repo": "b", "ref": SHA, "materialize": true});
    let first = call(&runtime, "ghStructure", query.clone())
        .await
        .expect("first");
    let local = std::path::PathBuf::from(
        row_data(&first)["location"]["localPath"]
            .as_str()
            .expect("localPath"),
    );
    std::fs::write(local.join("two.rs"), "tampered").expect("tamper");
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
    assert_eq!(row_data(&second)["location"]["complete"], true);
    assert_eq!(
        std::fs::read_to_string(local.join("one.rs")).expect("one"),
        "// one.rs\n"
    );
    assert_eq!(
        std::fs::metadata(local.join("one.rs"))
            .and_then(|meta| meta.modified())
            .expect("mtime"),
        old,
        "a recorded file at its size is reused, not rewritten"
    );
    assert_eq!(
        std::fs::read_to_string(local.join("two.rs")).expect("two"),
        "// two.rs\n"
    );
    runtime.close().await;
}

/// Branches and tags page together: each page holds the same page number of
/// both lists, and `next.nextPage` continues while either has more, so every
/// ref is listed exactly once.
#[tokio::test]
async fn refs_list_every_branch_and_tag_once_with_the_default_branch() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    let sha = |n: u8| format!("{n}").repeat(40);
    let pages = [
        (
            "branches",
            "1",
            json!([{"name":"dev","commit":{"sha":sha(1)}},{"name":"main","commit":{"sha":sha(2)}}]),
            true,
        ),
        (
            "branches",
            "2",
            json!([{"name":"release","commit":{"sha":sha(3)}}]),
            false,
        ),
        (
            "tags",
            "1",
            json!([{"name":"v1.0.0","commit":{"sha":sha(4)}}]),
            false,
        ),
        ("tags", "2", json!([]), false),
    ];
    for (kind, page, body, more) in pages {
        let mut response = ResponseTemplate::new(200).set_body_json(body);
        if more {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/{kind}?per_page=2&page=2>; rel=\"next\"",
                    server.uri()
                ),
            );
        }
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/{kind}")))
            .and(query_param("per_page", "2"))
            .and(query_param("page", page))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let first = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "refs", "pageSize": 2}),
    )
    .await
    .expect("refs");
    assert_eq!(
        row_status(&first),
        "success",
        "{}",
        first.structured_content
    );
    let data = row_data(&first);
    assert_eq!(data["defaultBranch"], "main", "{data}");
    assert_eq!(
        data["branches"],
        json!([{"name":"dev","sha":sha(1)},{"name":"main","sha":sha(2)}]),
        "{data}"
    );
    assert_eq!(
        data["tags"],
        json!([{"name":"v1.0.0","sha":sha(4)}]),
        "{data}"
    );
    let next = data["next"]["nextPage"]["query"]["queries"][0].clone();
    assert_eq!(next["operation"], "refs", "{data}");
    assert_eq!(next["page"], 2, "{data}");
    let second = call(&runtime, "ghStructure", next)
        .await
        .expect("refs page 2");
    let data = row_data(&second);
    assert_eq!(
        data["branches"],
        json!([{"name":"release","sha":sha(3)}]),
        "{data}"
    );
    assert_eq!(data["tags"], json!([]), "{data}");
    assert!(data.get("next").is_none(), "{data}");
    runtime.close().await;
}

/// Tree-only fields are rejected on refs and languages.
#[tokio::test]
async fn refs_and_languages_reject_tree_fields() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", "http://127.0.0.1:9/api/v3".into())]);
    for (operation, field) in [
        ("refs", json!({"path": "src"})),
        ("refs", json!({"materialize": true})),
        ("languages", json!({"ref": "main"})),
    ] {
        let mut query = json!({"owner": "a", "repo": "b", "operation": operation});
        for (key, value) in field.as_object().expect("field") {
            query[key] = value.clone();
        }
        let outcome = call(&runtime, "ghStructure", query).await;
        let text = match outcome {
            Ok(outcome) => outcome.structured_content.to_string(),
            Err(error) => format!("{error:?}"),
        };
        assert!(
            text.contains(&format!("only applies to tree; remove it from {operation}")),
            "{operation} {field}: {text}"
        );
    }
    runtime.close().await;
}

/// Languages: bytes of code per language, largest first.
#[tokio::test]
async fn languages_list_bytes_per_language_largest_first() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/languages"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"Shell":40,"Rust":9000,"TypeScript":700}"#)
                .insert_header("content-type", "application/json"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "operation": "languages"}),
    )
    .await
    .expect("languages");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        serde_json::to_string(&data["languages"]).expect("json"),
        r#"{"Rust":9000,"TypeScript":700,"Shell":40}"#,
        "{data}"
    );
    runtime.close().await;
}
