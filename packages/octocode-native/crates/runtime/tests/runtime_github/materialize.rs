//! Tree materialization and the opt-in MCP clone tool through the runtime.

use crate::support::{self, Run as _};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
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
        .respond_with(ResponseTemplate::new(200).set_body_json(json!((0..60).map(|n| json!({"name":format!("f{n:02}.rs"),"path":format!("f{n:02}.rs"),"type":"file","size":9,"sha":"1".repeat(40)})).collect::<Vec<_>>())))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    for name in (0..60).map(|n| format!("f{n:02}.rs")) {
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
            "operation": "tree",
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
    assert_eq!(data["location"]["coverage"]["state"], "partial", "{data}");
    let local = std::path::PathBuf::from(data["location"]["localPath"].as_str().expect("checkout"));
    let next = &data["next"]["continueMaterialize"];
    assert_eq!(next["kind"], "continue", "{data}");
    assert!(next["input"]["cursor"].is_string(), "{data}");
    let last = call(&runtime, "ghStructure", next["input"].clone())
        .await
        .expect("issued materialization continuation");
    let completed = row_data(&last);
    assert_eq!(
        completed["location"]["coverage"]["state"], "complete",
        "{completed}"
    );
    assert_eq!(
        completed["location"]["localPath"],
        data["location"]["localPath"]
    );
    for name in (0..60).map(|n| format!("f{n:02}.rs")) {
        assert_eq!(
            std::fs::read_to_string(local.join(name)).expect("materialized source"),
            "fn x(){}\n"
        );
    }
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
        json!({"operation":"tree","owner": "a", "repo": "b", "materialize": true}),
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
    assert!(deeper["input"]["cursor"].is_string(), "{data}");
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{sha}")))
        .and(query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"sha":sha,"truncated":false,"tree":[
                {"path":"one.rs","type":"blob","mode":"100644","sha":"a".repeat(40)},
                {"path":"router","type":"tree","mode":"040000","sha":"b".repeat(40)},
                {"path":"router/inside.rs","type":"blob","mode":"100644","sha":"c".repeat(40)}
            ]}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/router%2Finside.rs"))
        .and(query_param("ref",sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","encoding":"base64","content":STANDARD.encode("fn inside() {}\n"),"size":15,"sha":"c".repeat(40),"path":"router/inside.rs"})))
        .expect(1).mount(&server).await;
    let expanded = call(&runtime, "ghStructure", deeper["input"].clone())
        .await
        .expect("issued depth expansion");
    let completed = row_data(&expanded);
    assert_eq!(
        completed["location"]["coverage"]["state"], "complete",
        "{completed}"
    );
    let local = std::path::Path::new(
        completed["location"]["localPath"]
            .as_str()
            .expect("checkout"),
    );
    assert_eq!(
        std::fs::read_to_string(local.join("router/inside.rs")).expect("nested source"),
        "fn inside() {}\n"
    );
    runtime.close().await;
}

#[tokio::test]
async fn clone_mcp_opt_in_is_independent_of_storage_and_cli_host() {
    let server = MockServer::start().await;
    for surface in [
        octocode_native::config::RuntimeSurface::Cli,
        octocode_native::config::RuntimeSurface::Mcp,
    ] {
        for enabled in ["false", "true"] {
            let workspace = Workspace::new();
            let mut input = workspace.config(&[
                ("OCTOCODE_ENABLE_CLONE", enabled.into()),
                ("OCTOCODE_STORAGE_MODE", "memory".into()),
                ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
            ]);
            input.runtime_surface = surface;
            let runtime = octocode_native::runtime::ToolRuntime::with_limits(
                input,
                crate::support::FIXTURE_LIMITS,
            )
            .expect("runtime");
            assert_eq!(
                runtime.is_available("ghCloneRepo"),
                surface == octocode_native::config::RuntimeSurface::Cli || enabled == "true"
            );
            let result = runtime
                .run_mcp(
                    "mcp-clone".into(),
                    "ghCloneRepo".into(),
                    json!({"queries":[{"owner":"a","repo":"b"}]}),
                )
                .await
                .expect("MCP response");
            if enabled == "false" {
                assert_eq!(
                    result["structuredContent"]["errorCode"], "toolUnavailable",
                    "{result}"
                );
                assert!(
                    result.to_string().contains("OCTOCODE_ENABLE_CLONE"),
                    "{result}"
                );
            } else {
                assert_eq!(result["isError"], true, "{result}");
                assert_eq!(
                    result["structuredContent"]["errorCode"], "persistentStorageDisabled",
                    "{result}"
                );
                assert!(result.to_string().contains("persistent"), "{result}");
                assert!(!result.to_string().contains("toolUnavailable"), "{result}");
            }
            runtime.close().await;
        }
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "storage prerequisite precedes GitHub work"
    );
}
