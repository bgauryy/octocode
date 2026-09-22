// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod support;

use serde_json::json;
use std::collections::BTreeMap;
use support::{Workspace, call, query_path, row_data, row_status};

use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};

#[tokio::test]
async fn ordinary_tools_require_nonempty_reasoning() {
    let workspace = Workspace::new();
    let path = workspace.write("reasoning.txt", "ok\n");
    let runtime = workspace.runtime(&[]);
    let path = path.to_string_lossy().into_owned();

    // Omitting reasoning entirely is rejected (required since core 19.1.1).
    let error = runtime
        .execute(
            "reasoning-omitted".into(),
            "localFetch".into(),
            json!({"path":path}),
        )
        .await
        .expect_err("omitted reasoning must be rejected");
    assert_eq!(error.code, "invalidInput");

    // Blank reasoning (whitespace-only) is also rejected.
    let error = runtime
        .execute(
            "reasoning-blank".into(),
            "localFetch".into(),
            json!({"path":path,"reasoning":"   "}),
        )
        .await
        .expect_err("blank reasoning must be rejected when supplied");
    assert_eq!(error.code, "invalidInput");

    // Valid reasoning succeeds.
    let outcome = runtime
        .execute(
            "reasoning-valid".into(),
            "localFetch".into(),
            json!({"path":path,"reasoning":"Read the fixture."}),
        )
        .await
        .expect("valid reasoning must be accepted");
    assert_eq!(
        outcome.structured_content["results"][0]["data"]["content"],
        "ok\n"
    );
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
        .execute(
            "bulk-success".into(),
            "localFetch".into(),
            json!({"queries":[
                {"path":first,"reasoning":"Read the first fixture."},
                {"path":second,"reasoning":"Read the second fixture."}
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
    assert_eq!(rows[0]["data"]["content"], "first\n");
    assert_eq!(rows[1]["data"]["content"], "second\n");
    assert!(!successful.all_failed);

    let mixed = runtime
        .execute(
            "bulk-mixed".into(),
            "localFetch".into(),
            json!({"queries":[
                {"path":missing,"reasoning":"Exercise one missing fixture."},
                {"path":first,"reasoning":"Retain the successful fixture."}
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
async fn local_fetch_pages_and_unions_through_the_runtime() {
    let workspace = Workspace::new();
    let path = workspace.write("source.txt", "one\ntwo 😀\nthree\n");
    let runtime = workspace.runtime(&[]);
    let first = call(
        &runtime,
        "localFetch",
        query_path(&path, json!({"chunkType":"lines","chunkSize":1})),
    )
    .await
    .expect("first page");
    assert_eq!(row_status(&first), "success");
    let data = row_data(&first);
    assert_eq!(data["content"].as_str(), Some("one\n"));
    let next = data["next"]["continue"]["query"].clone();
    assert!(next.is_object(), "executable continuation");

    let second = call(&runtime, "localFetch", next)
        .await
        .expect("second page");
    assert_eq!(row_data(&second)["content"].as_str(), Some("two 😀\n"));
    runtime.close().await;
}

#[tokio::test]
async fn mcp_local_fetch_cursors_stale_only_the_mutated_batch_row() {
    let workspace = Workspace::new();
    let first_path = workspace.write("cursor-first.txt", "first-1\nfirst-2\n");
    let second_path = workspace.write("cursor-second.txt", "second-1\nsecond-2\n");
    let runtime = workspace.runtime(&[]);
    let initial = runtime
        .execute_mcp(
            "mcp-local-cursor-batch".into(),
            "localFetch".into(),
            json!({"queries":[
                {
                    "path":first_path,
                    "chunkType":"lines",
                    "chunkSize":1,
                    "reasoning":"Page the first cursor fixture."
                },
                {
                    "path":second_path,
                    "chunkType":"lines",
                    "chunkSize":1,
                    "reasoning":"Page the second cursor fixture."
                }
            ]}),
        )
        .await
        .expect("MCP localFetch batch");
    let rows = initial["structuredContent"]["results"]
        .as_array()
        .expect("MCP result rows");
    let first_cursor = rows[0]["data"]["next"]["continue"]["cursor"]
        .as_str()
        .expect("first row cursor")
        .to_owned();
    let second_cursor = rows[1]["data"]["next"]["continue"]["cursor"]
        .as_str()
        .expect("second row cursor")
        .to_owned();

    std::fs::write(&first_path, "changed-1\nchanged-2\n").expect("mutate first source");
    let stale = runtime
        .execute(
            "resume-mutated-local-row".into(),
            "localFetch".into(),
            json!({"cursor":first_cursor}),
        )
        .await
        .expect_err("mutated source must stale its cursor");
    assert_eq!(stale.code, "staleCursor");

    let resumed = runtime
        .execute(
            "resume-unchanged-local-row".into(),
            "localFetch".into(),
            json!({"cursor":second_cursor}),
        )
        .await
        .expect("unchanged source cursor remains valid");
    assert_eq!(
        resumed.structured_content["results"][0]["data"]["content"],
        "second-2\n"
    );
    runtime.close().await;
}

#[tokio::test]
async fn local_fetch_rejects_unknown_fields_at_the_contract() {
    let workspace = Workspace::new();
    let path = workspace.write("a.txt", "ok\n");
    let runtime = workspace.runtime(&[]);
    let error = call(
        &runtime,
        "localFetch",
        json!({"path": path, "madeUp": true}),
    )
    .await
    .expect_err("unknown field");
    assert_eq!(error.code, "invalidInput");
    runtime.close().await;
}

#[tokio::test]
async fn local_search_finds_literal_matches() {
    let workspace = Workspace::new();
    workspace.write("src/main.ts", "export function needle() { return 1; }\n");
    workspace.write("src/other.ts", "const unused = 2;\n");
    let runtime = workspace.runtime(&[]);
    let found = call(
        &runtime,
        "localSearch",
        json!({
            "path": workspace.workspace,
            "searchText": "needle",
            "regex": "literal"
        }),
    )
    .await
    .expect("search");
    assert_eq!(row_status(&found), "success");
    let rendered = serde_json::to_string(row_data(&found)).expect("json");
    assert!(
        rendered.contains("main.ts"),
        "expected main.ts in {rendered}"
    );

    runtime.close().await;
}

#[tokio::test]
async fn disabled_local_family_is_unavailable() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("ENABLE_LOCAL", "false".into())]);
    let error = call(
        &runtime,
        "localFetch",
        json!({"path": workspace.workspace.join("missing.txt")}),
    )
    .await
    .expect_err("disabled");
    assert_eq!(error.code, "toolUnavailable");
    runtime.close().await;
}

#[tokio::test]
async fn beta_ast_tools_require_the_shared_opt_in() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    assert!(!runtime.is_available("astRewrite"));
    assert!(!runtime.is_available("astTopology"));
    let error = call(
        &runtime,
        "astRewrite",
        json!({
            "path": workspace.workspace,
            "langType": "typescript",
            "ruleKind": "pattern",
            "pattern": "console.log($A)",
            "rewrite": "logger.info($A)"
        }),
    )
    .await
    .expect_err("astRewrite must be opt-in");
    assert_eq!(error.code, "missingConfiguration");
    assert!(error.message.contains("OCTOCODE_BETA"));
    let topology_error = call(
        &runtime,
        "astTopology",
        json!({
            "operation": "topology",
            "analysis": "cycles",
            "path": workspace.workspace
        }),
    )
    .await
    .expect_err("astTopology must be opt-in");
    assert_eq!(topology_error.code, "missingConfiguration");
    assert!(topology_error.message.contains("OCTOCODE_BETA"));
    runtime.close().await;

    let enabled = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    assert!(enabled.is_available("astRewrite"));
    assert!(enabled.is_available("astTopology"));
    enabled.close().await;
}

#[tokio::test]
async fn host_options_environment_controls_embedded_tool_availability() {
    let workspace = Workspace::new();
    let runtime = ToolRuntime::from_host(HostOptions {
        cwd: Some(workspace.workspace.clone()),
        env: Some(BTreeMap::from([
            ("ENABLE_LOCAL".into(), "true".into()),
            ("OCTOCODE_BETA".into(), "true".into()),
        ])),
        surface: RuntimeSurface::Mcp,
        ..HostOptions::default()
    })
    .expect("embedded runtime with explicit environment");
    assert!(runtime.is_available("astRewrite"));
    assert!(runtime.is_available("astTopology"));
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
    let catalog = runtime.catalog().expect("catalog");
    assert_eq!(catalog["fingerprint"], internal["fingerprint"]);
    assert!(
        !catalog.to_string().contains("\"outputSchema\""),
        "public runtime catalog must not expose output schemas"
    );
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
    assert!(names.contains(&"ghSearch"));
    runtime.close().await;
}

#[tokio::test]
async fn ast_search_lists_files_through_the_runtime() {
    let workspace = Workspace::new();
    workspace.write("src/lib.rs", "pub fn needle() {}\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "astSearch",
        json!({
            "operation": "files",
            "path": workspace.workspace
        }),
    )
    .await
    .expect("astSearch");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(rendered.contains("lib.rs"), "expected lib.rs in {rendered}");
    runtime.close().await;
}

#[tokio::test]
async fn ast_topology_executes_only_after_beta_opt_in() {
    let workspace = Workspace::new();
    workspace.write(
        "src/a.ts",
        "import { b } from './b';\nexport const a = b;\n",
    );
    workspace.write("src/b.ts", "export const b = 1;\n");
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    let outcome = call(
        &runtime,
        "astTopology",
        json!({
            "operation": "topology",
            "analysis": "cycles",
            "path": workspace.workspace
        }),
    )
    .await
    .expect("astTopology");
    assert_eq!(row_data(&outcome)["analysis"], "cycles");
    runtime.close().await;
}

#[tokio::test]
async fn lsp_search_returns_a_typed_row_when_no_server_is_configured() {
    let workspace = Workspace::new();
    let path = workspace.write("notes.md", "# title\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "documentSymbols",
            "uri": path
        }),
    )
    .await
    .expect("lspSearch");
    let status = row_status(&outcome);
    assert!(
        status == "error" || status == "empty" || status == "success",
        "unexpected status {status}: {}",
        outcome.structured_content
    );
    let symbol_failure = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "definition",
            "uri": path,
            "symbolName": "title",
            "lineHint": 1
        }),
    )
    .await
    .expect("symbol recovery output satisfies its contract");
    assert_eq!(row_status(&symbol_failure), "error");
    assert_eq!(
        row_data(&symbol_failure)["next"]["readFile"]["tool"],
        "localFetch"
    );
    runtime.close().await;
}

#[tokio::test]
async fn lsp_search_rejects_files_outside_allowed_roots_before_server_discovery() {
    let workspace = Workspace::new();
    let outside = workspace.write_outside_allowed_roots("secret.rs", "pub fn secret() {}\n");
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "documentSymbols",
            "uri": outside
        }),
    )
    .await
    .expect("typed lsp denial");
    assert_eq!(row_status(&outcome), "error");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("outside allowed directories"),
        "expected path-policy denial, got {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn resolved_lsp_config_path_reaches_engine_discovery() {
    let workspace = Workspace::new();
    let path = workspace.write("source.custom", "symbol\n");
    let config_path = workspace.write(
        ".octocode/lsp.json",
        r#"{"languageServers":{".custom":{"command":"missing-custom-lsp","args":[],"languageId":"custom"}}}"#,
    );
    let runtime = workspace.runtime(&[(
        "OCTOCODE_LSP_CONFIG",
        config_path.to_string_lossy().into_owned(),
    )]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "documentSymbols",
            "uri": path
        }),
    )
    .await
    .expect("typed configured server failure");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        !rendered.contains("No language server is configured"),
        "resolved config was ignored: {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn close_joins_a_fresh_runtime() {
    let workspace = Workspace::new();
    workspace.runtime(&[]).close().await;
}
