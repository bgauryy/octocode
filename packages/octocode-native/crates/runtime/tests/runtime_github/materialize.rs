//! Tree materialization and the CLI-only clone tool through the runtime.

use crate::support::{self, Run as _};
use octocode_contract_types::ErrorCode;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_github::mock_http::{Arrivals, Delayed};
use serde_json::json;
use std::time::Duration;
use support::{ALL_EXTRAS, Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn github_tree_materialize_is_accepted_and_emits_location() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":4,"sha":"1".repeat(40)},
            {"name":"two.rs","path":"two.rs","type":"file","size":4,"sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    for name in ["one.rs", "two.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode("fn x(){}\n"),
                "size": 8,
                "sha": "a".repeat(40),
                "path": name
            })))
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({
            "owner": "a",
            "repo": "b",
            "materialize": true
        }),
    )
    .await
    .expect("tree materialize");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert!(data["location"]["localPath"].as_str().is_some(), "{}", data);
    // Only the path and coverage: kind/source/cached are constants, hasMore
    // restates `complete`, and the ref is the snapshot the path names.
    let mut keys: Vec<&str> = data["location"]
        .as_object()
        .expect("location")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["coverage", "localPath"], "{data}");
    let exhausted = call(&runtime, "ghStructure", json!({"owner":"a","repo":"b","ref":sha,"pageSize":1,"materialize":true,"materializeOffset":1})).await.expect("boundary offset");
    let boundary = row_data(&exhausted);
    assert_eq!(boundary["pagination"]["hasMore"], true, "{boundary}");
    // A materialize resumed inside its page does not re-send the
    // entries its first call listed; `location` anchors it.
    let resumed = call(
        &runtime,
        "ghStructure",
        json!({"owner":"a","repo":"b","ref":sha,"materialize":true,"materializeOffset":1}),
    )
    .await
    .expect("mid-page offset");
    let resumed = row_data(&resumed);
    assert!(resumed.get("files").is_none(), "{resumed}");
    assert!(
        resumed["location"]["localPath"].as_str().is_some(),
        "{resumed}"
    );
    assert_eq!(
        resumed["location"]["coverage"]["state"], "complete",
        "{resumed}"
    );
    let next = &boundary["next"]["continueMaterialize"]["query"]["queries"][0];
    assert_eq!(next["ref"], sha, "{boundary}");
    assert_eq!(next["page"], 2, "{boundary}");
    assert_eq!(next["materializeOffset"], 0, "{boundary}");
    let last = call(&runtime, "ghStructure", next.clone())
        .await
        .expect("replay boundary continuation");
    let completed = row_data(&last);
    assert_eq!(
        completed["location"]["coverage"]["state"], "complete",
        "{completed}"
    );
    runtime.close().await;
}

/// A materialize that lists folders below its depth without writing
/// them is not complete, and `next.expandDepth` writes them.
#[tokio::test]
async fn github_tree_materialize_with_unwritten_folders_is_not_complete() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":4,"sha":"1".repeat(40)},
            {"name":"router","path":"router","type":"dir","sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/one.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode("fn x(){}\n"),
            "size": 8, "sha": "a".repeat(40), "path": "one.rs"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "materialize": true}),
    )
    .await
    .expect("tree materialize");
    let data = row_data(&outcome);
    assert_eq!(data["location"]["coverage"]["state"], "partial", "{data}");
    assert_eq!(data["isPartial"], true, "{data}");
    assert!(
        data["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&json!("materializeDepth"))),
        "{data}"
    );
    let deeper = &data["next"]["expandDepth"];
    assert_eq!(deeper["tool"], "ghStructure", "{data}");
    let row = &deeper["query"]["queries"][0];
    assert_eq!(row["maxDepth"], 20, "{data}");
    assert_eq!(row["materialize"], true, "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn github_clone_is_cli_only() {
    let workspace = Workspace::new();
    let cli = workspace.runtime(&[]);
    assert!(cli.is_available("ghCloneRepo"));
    let mcp_call = cli
        .run_mcp(
            "mcp-clone".into(),
            "ghCloneRepo".into(),
            json!({"queries":[{"owner":"a","repo":"b","mainGoal": "test", "reasoning":"check MCP gate"}]}),
        )
        .await
        .expect("MCP answers a refused call as a tool error");
    assert_eq!(mcp_call["isError"], true, "{mcp_call}");
    assert_eq!(
        mcp_call["structuredContent"]["errorCode"], "toolUnavailable",
        "MCP channel cannot clone through a CLI runtime: {mcp_call}"
    );
    cli.close().await;

    let mut input = workspace.config(&[]);
    input.runtime_surface = octocode_native::config::RuntimeSurface::Mcp;
    let mcp =
        octocode_native::runtime::ToolRuntime::with_limits(input, crate::support::FIXTURE_LIMITS)
            .expect("MCP runtime");
    assert!(!mcp.is_available("ghCloneRepo"));
    let error = call(&mcp, "ghCloneRepo", json!({"owner":"a","repo":"b"}))
        .await
        .expect_err("clone disabled for MCP");
    assert_eq!(error.code, ErrorCode::ToolUnavailable);
    mcp.close().await;
}
