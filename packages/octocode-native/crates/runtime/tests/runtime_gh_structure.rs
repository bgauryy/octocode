//! ghStructure: [`materialize`] layout ownership, [`refs`], tree and
//! languages [`listing`]s, and tree [`page_walks`]; the shared mock repository
//! lives here.
use crate::support;
use octocode_contract_types::ErrorCode;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod listing;
mod materialize;
mod page_walks;
mod refs;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Mounts repository `a/b` with default branch `main`.
async fn mount_default_branch(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(server)
        .await;
}

/// Mounts [`mount_default_branch`] and the branch's head at [`SHA`].
async fn mount_head_at_sha(server: &MockServer) {
    mount_default_branch(server).await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": SHA})))
        .mount(server)
        .await;
}

/// A repository `a/b` whose default branch `main` is at [`SHA`] with
/// `one.rs` and `two.rs`; each file read is expected `reads` times.
async fn tree_server(reads: u64) -> MockServer {
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
            .expect(reads)
            .mount(&server)
            .await;
    }
    server
}

fn github(workspace: &Workspace, server: &MockServer) -> octocode_native::runtime::ToolRuntime {
    workspace.runtime(&support::github_api(server))
}

/// A repository `a/b` at [`SHA`] whose root lists `entries` (name, type).
async fn listing_server(entries: &[(String, &str)]) -> MockServer {
    let server = MockServer::builder().start().await;
    mount_head_at_sha(&server).await;
    let rows = entries
        .iter()
        .map(|(name, kind)| json!({"name": name, "path": name, "type": kind, "size": 10}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(rows)))
        .mount(&server)
        .await;
    server
}

async fn graphql_requests(server: &MockServer) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .map(|request| serde_json::from_slice(&request.body).expect("graphql body"))
        .collect()
}
