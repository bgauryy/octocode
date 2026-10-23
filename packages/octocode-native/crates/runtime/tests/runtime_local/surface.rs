//! Runtime surface: briefs, bulk rows, tool availability, the catalog and replay.

use crate::support::{self, Run as _, Workspace, call};
use octocode_contract_types::ErrorCode;
use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};
use serde_json::json;
use std::collections::BTreeMap;

#[tokio::test]
async fn ordinary_tools_take_an_optional_brief() {
    let workspace = Workspace::new();
    let path = workspace.write("reasoning.txt", "ok\n");
    let runtime = workspace.runtime(&[]);
    let path = path.to_string_lossy().into_owned();

    // The brief is optional: a call without one runs, and a blank one is
    // dropped rather than rejected.
    for (label, query) in [
        ("brief-omitted", json!({"path":path})),
        (
            "brief-blank",
            json!({"path":path,"mainGoal": "  ", "reasoning":"   "}),
        ),
        (
            "brief-valid",
            json!({"path":path,"mainGoal": "test", "reasoning":"Read the fixture."}),
        ),
    ] {
        let outcome = runtime
            .run(
                label.into(),
                "localFetch".into(),
                json!({"queries":[query]}),
            )
            .await
            .unwrap_or_else(|error| panic!("{label} must be accepted: {error:?}"));
        assert_eq!(
            outcome.structured_content["results"][0]["data"]["content"], "1\tok\n",
            "{label}"
        );
    }
    runtime.close().await;
}

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
                {"path":first,"mainGoal": "test", "reasoning":"Read the first fixture."},
                {"path":second,"mainGoal": "test", "reasoning":"Read the second fixture."}
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
                {"path":missing,"mainGoal": "test", "reasoning":"Exercise one missing fixture."},
                {"path":first,"mainGoal": "test", "reasoning":"Retain the successful fixture."}
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
                && tool["shortDescription"].is_string()
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
async fn continuations_replay_without_the_request_brief() {
    let workspace = Workspace::new();
    for name in ["a", "b", "c"] {
        workspace.write(&format!("{name}.txt"), "needle\n");
    }
    let runtime = workspace.runtime(&[]);
    let first = runtime
        .run(
            "page-1".into(),
            "localSearch".into(),
            json!({"queries":[{"path":workspace.workspace,"matchString":"needle","pageSize":1,
                "mainGoal":"Find every needle file for the audit.","reasoning":"List files one page at a time."}]}),
        )
        .await
        .expect("first page");
    let next = &first.structured_content["results"][0]["data"]["next"]["nextPage"];
    let query = next["query"].clone();
    // The brief is request-level (docs/TOOL_DATA_CONTRACT.md): the page
    // carries the scope and cursor, never the mainGoal or reasoning.
    assert!(
        query["queries"][0].get("mainGoal").is_none()
            && query["queries"][0].get("reasoning").is_none(),
        "{next}"
    );
    // The continuation is the whole call: it runs exactly as emitted.
    let second = runtime
        .run(
            "page-2".into(),
            next["tool"].as_str().unwrap().into(),
            query,
        )
        .await
        .expect("the continuation replays unchanged");
    assert_eq!(
        second.structured_content["results"][0]["data"]["files"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );
    runtime.close().await;
}
