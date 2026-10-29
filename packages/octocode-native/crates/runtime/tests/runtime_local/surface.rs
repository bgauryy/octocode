//! Runtime surface: briefs, bulk rows, tool availability, the catalog and replay.

use crate::support::{self, Run as _, Workspace, call};
use octocode_contract_types::ErrorCode;
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};
use serde_json::json;
use std::collections::BTreeMap;

#[tokio::test]
async fn bulk_queries_preserve_indexes_and_isolate_domain_failures() {
    let workspace = Workspace::new();
    let first = workspace.write("first.txt", "first\n");
    let second = workspace.write("second.txt", "second\n");
    let missing = workspace.workspace.join("missing.txt");
    let runtime = workspace.runtime(&[]);

    let successful = runtime
        .run(
            "bulk-success".into(),
            "localFetch".into(),
            json!({"queries":[
                {"path":first},
                {"path":second}
            ]}),
        )
        .await
        .expect("bulk success");
    let rows = successful.structured_content["results"]
        .as_array()
        .expect("result rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["index"], 0);
    assert_eq!(rows[1]["index"], 1);
    assert_eq!(rows[0]["data"]["content"], "1\tfirst\n");
    assert_eq!(rows[1]["data"]["content"], "1\tsecond\n");
    assert!(!successful.all_failed);

    let mixed = runtime
        .run(
            "bulk-mixed".into(),
            "localFetch".into(),
            json!({"queries":[
                {"path":missing},
                {"path":first}
            ]}),
        )
        .await
        .expect("mixed bulk result");
    let rows = mixed.structured_content["results"]
        .as_array()
        .expect("mixed rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["status"], "error");
    assert!(rows[1].get("status").is_none());
    assert!(!mixed.all_failed);
    runtime.close().await;
}

#[tokio::test]
async fn disabled_local_family_is_unavailable() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_ENABLE_LOCAL", "false".into())]);
    let error = call(
        &runtime,
        "localFetch",
        json!({"path": workspace.workspace.join("missing.txt")}),
    )
    .await
    .expect_err("disabled");
    assert_eq!(error.code, ErrorCode::ToolUnavailable);
    runtime.close().await;
}

#[tokio::test]
async fn host_options_environment_controls_embedded_tool_availability() {
    let workspace = Workspace::new();
    let runtime = ToolRuntime::from_host(HostOptions {
        cwd: Some(workspace.workspace.clone()),
        // The fixture home, so the user's ~/.octocode is never read or written.
        env: Some(BTreeMap::from([
            ("OCTOCODE_ENABLE_LOCAL".into(), "true".into()),
            ("OCTOCODE_BETA".into(), "true".into()),
            (
                "OCTOCODE_HOME".into(),
                workspace.home.to_string_lossy().into_owned(),
            ),
        ])),
        surface: RuntimeSurface::Mcp,
        ..HostOptions::default()
    })
    .expect("embedded runtime with explicit environment");
    // Beta never opens a CLI-only tool on MCP.
    assert!(runtime.is_available("astSearch"));
    let catalog = runtime.catalog();
    {
        let name = "ghCloneRepo";
        assert!(!runtime.is_available(name), "{name}");
        let entry = catalog["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("{name} entry"));
        assert_eq!(entry["available"], false, "{name}");
        assert_eq!(entry["unavailableReason"], "cliOnly", "{name}");
    }
    let error = runtime
        .run_mcp(
            "mcp-clone".into(),
            "ghCloneRepo".into(),
            json!({"queries":[{"owner":"octocat","repo":"Hello-World"}]}),
        )
        .await
        .expect("MCP answers a refused call as a tool error");
    assert_eq!(
        error["structuredContent"]["errorCode"], "toolUnavailable",
        "ghCloneRepo is CLI-only: {error}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn runtime_catalog_lists_available_tools() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let internal = octocode_native::contracts::parsed_contract().expect("embedded contract");
    assert!(
        internal["tools"]
            .as_array()
            .expect("internal tools")
            .iter()
            .all(|tool| tool["outputSchema"].is_object()),
        "internal output schemas must remain available for runtime validation"
    );
    let catalog = runtime.catalog();
    assert_eq!(catalog["fingerprint"], internal["fingerprint"]);
    support::checks::assert_absent(&catalog, "outputSchema");
    assert!(
        catalog["tools"]
            .as_array()
            .expect("public tools")
            .iter()
            .all(|tool| tool["available"].is_boolean()
                && tool["title"].is_string()
                && tool.get("inputSchema").is_none()
                && tool.get("querySchema").is_none()),
        "runtime catalog is names + availability only; schemas ship via core and `scheme`"
    );
    let names: Vec<_> = catalog["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"localFetch"));
    assert!(names.contains(&"localSearch"));
    assert!(names.contains(&"ghSearchCode"));
    let grammars = catalog["grammarCapabilities"]
        .as_array()
        .expect("runtime grammar capabilities");
    assert!(grammars.iter().any(|entry| {
        entry["language"] == "Rust"
            && entry["extensions"]
                .as_array()
                .is_some_and(|extensions| extensions.contains(&json!("rs")))
            && entry["structuralSearch"] == true
    }));
    let servers = catalog["lspServers"]
        .as_array()
        .expect("runtime lspSearch server languages");
    let order = ["ts/js", "py", "rust", "c/c++", "go", "c#", "java"];
    let positions: Vec<_> = servers
        .iter()
        .map(|label| {
            order
                .iter()
                .position(|known| label == *known)
                .unwrap_or_else(|| panic!("unknown server label {label}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{servers:?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn graph_mcp_gate_covers_discovery_calls_and_cli_host_embedding() {
    for surface in [RuntimeSurface::Cli, RuntimeSurface::Mcp] {
        for beta in ["false", "true"] {
            let workspace = Workspace::new();
            workspace.write("entry.ts", "export function entry() { return 1; }\n");
            let runtime = ToolRuntime::from_host(HostOptions {
                cwd: Some(workspace.workspace.clone()),
                env: Some(BTreeMap::from([
                    ("OCTOCODE_ENABLE_LOCAL".into(), "true".into()),
                    ("OCTOCODE_BETA".into(), beta.into()),
                    (
                        "OCTOCODE_HOME".into(),
                        workspace.home.to_string_lossy().into_owned(),
                    ),
                ])),
                surface,
                ..HostOptions::default()
            })
            .expect("runtime");
            for name in ["graphBuild", "graphQuery"] {
                assert_eq!(
                    runtime.is_available(name),
                    surface == RuntimeSurface::Cli || beta == "true"
                );
                let query = if name == "graphBuild" {
                    json!({"path": workspace.workspace})
                } else {
                    json!({"operation": "stats", "path": workspace.workspace})
                };
                let result = runtime
                    .run_mcp(
                        format!("{name}-{beta}"),
                        name.into(),
                        json!({"queries": [query]}),
                    )
                    .await
                    .expect("MCP response");
                if beta == "false" {
                    assert_eq!(
                        result["structuredContent"]["errorCode"], "toolUnavailable",
                        "{result}"
                    );
                    assert!(
                        result["structuredContent"]
                            .to_string()
                            .contains("OCTOCODE_BETA"),
                        "{result}"
                    );
                    assert!(!workspace.workspace.join(".octocode/graph").exists());
                } else {
                    assert_ne!(result["isError"], true, "{result}");
                    let data = &result["structuredContent"]["results"][0]["data"];
                    assert!(data.get("errorCode").is_none(), "{result}");
                    assert_eq!(data["counts"]["nodesByKind"]["symbol"], 1, "{result}");
                }
            }
            runtime.close().await;
        }
    }
}
