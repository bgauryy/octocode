//! A tree walk lists a symlink as a link entry with its target path, and
//! never reads or serves the target's bytes.
use crate::support::{Workspace, call, row_data, row_status};
use base64::Engine as _;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const BLOB: &str = "f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0";

async fn mount_tree(server: &MockServer, entries: Vec<Value>) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"sha": SHA, "truncated": false, "tree": entries})),
        )
        .mount(server)
        .await;
}

async fn mount_link(server: &MockServer, name: &str, target: &str) {
    // The link's own blob is its target path; the target is never read.
    let _ = name;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/blobs/{BLOB}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "encoding": "base64",
            "content": base64::engine::general_purpose::STANDARD.encode(target)
        })))
        .mount(server)
        .await;
}

async fn list(server: &MockServer) -> Value {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner":"a","repo":"b","operation":"tree","ref":SHA,"maxDepth":4}),
    )
    .await
    .expect("tree");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome).clone();
    runtime.close().await;
    data
}

fn listed(data: &Value) -> Vec<String> {
    data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row["files"].as_array().cloned().unwrap_or_default())
        .filter_map(|entry| entry.as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn a_symlink_is_listed_with_its_target_and_counted() {
    let server = MockServer::builder().start().await;
    mount_tree(
        &server,
        vec![
            json!({"path":"a.js","mode":"100644","type":"blob","sha":"1".repeat(40),"size":5}),
            json!({"path":"link.js","mode":"120000","type":"blob","sha":BLOB,"size":4}),
        ],
    )
    .await;
    mount_link(&server, "link.js", "a.js").await;
    let data = list(&server).await;
    let entries = listed(&data);
    let link = entries
        .iter()
        .find(|entry| entry.starts_with("link.js"))
        .unwrap_or_else(|| panic!("the symlink is listed: {data}"));
    assert!(link.contains("a.js"), "the entry names its target: {link}");
    assert!(
        !data.to_string().contains("\"omitted\""),
        "a listed link is not also omitted: {data}"
    );
}

#[tokio::test]
async fn a_link_to_a_credential_file_is_listed_and_its_target_is_never_read() {
    let server = MockServer::builder().start().await;
    mount_tree(
        &server,
        vec![json!({"path":"env","mode":"120000","type":"blob","sha":BLOB,"size":4})],
    )
    .await;
    mount_link(&server, "env", ".env").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/.env"))
        .respond_with(ResponseTemplate::new(200).set_body_string("TOKEN=secret"))
        .expect(0)
        .mount(&server)
        .await;
    let data = list(&server).await;
    let entries = listed(&data);
    assert!(
        entries
            .iter()
            .any(|entry| entry.starts_with("env") && entry.contains(".env")),
        "{data}"
    );
    assert!(!data.to_string().contains("TOKEN=secret"));
}

/// A folder holding only links lists them and is not empty.
#[tokio::test]
async fn a_folder_of_only_links_lists_them() {
    let server = MockServer::builder().start().await;
    let links: Vec<Value> = (0..3)
        .map(|n| {
            json!({"path": format!("l{n}"), "mode":"120000", "type":"blob",
                   "sha": format!("{n:040x}"), "size": 4})
        })
        .collect();
    mount_tree(&server, links).await;
    let data = list(&server).await;
    assert_eq!(listed(&data).len(), 3, "{data}");
    assert!(
        data["files"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{data}"
    );
}
