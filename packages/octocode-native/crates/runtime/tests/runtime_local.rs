// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod support;

use serde_json::json;
use std::collections::BTreeMap;
use support::{Workspace, call, query_path, row_data, row_status};

use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};

#[tokio::test]
async fn ordinary_tools_accept_optional_trace_context() {
    let workspace = Workspace::new();
    let path = workspace.write("reasoning.txt", "ok\n");
    let runtime = workspace.runtime(&[]);
    let path = path.to_string_lossy().into_owned();

    let outcome = runtime
        .execute(
            "reasoning-omitted".into(),
            "localFetch".into(),
            json!({"path":path}),
        )
        .await
        .expect("trace context is optional");
    assert_eq!(
        outcome.structured_content["results"][0]["data"]["content"],
        "ok\n"
    );

    let outcome = runtime
        .execute(
            "reasoning-blank".into(),
            "localFetch".into(),
            json!({"path":path,"goal": "test", "reasoning":"   "}),
        )
        .await
        .expect("blank trace context is harmless");
    assert_eq!(
        outcome.structured_content["results"][0]["data"]["content"],
        "ok\n"
    );

    let outcome = runtime
        .execute(
            "reasoning-valid".into(),
            "localFetch".into(),
            json!({"path":path,"goal": "test", "reasoning":"Read the fixture."}),
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
                {"path":first,"goal": "test", "reasoning":"Read the first fixture."},
                {"path":second,"goal": "test", "reasoning":"Read the second fixture."}
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
                {"path":missing,"goal": "test", "reasoning":"Exercise one missing fixture."},
                {"path":first,"goal": "test", "reasoning":"Retain the successful fixture."}
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
async fn mcp_local_fetch_snapshots_stale_only_the_mutated_batch_row() {
    let workspace = Workspace::new();
    let first_path = workspace.write("snapshot-first.txt", "first-1\nfirst-2\n");
    let second_path = workspace.write("snapshot-second.txt", "second-1\nsecond-2\n");
    let runtime = workspace.runtime(&[]);
    let initial = runtime
        .execute_mcp(
            "mcp-local-snapshot-batch".into(),
            "localFetch".into(),
            json!({"queries":[
                {
                    "path":first_path,
                    "chunkType":"lines",
                    "chunkSize":1,
                    "goal": "test", "reasoning":"Page the first snapshot fixture."
                },
                {
                    "path":second_path,
                    "chunkType":"lines",
                    "chunkSize":1,
                    "goal": "test", "reasoning":"Page the second snapshot fixture."
                }
            ]}),
        )
        .await
        .expect("MCP localFetch batch");
    let structured = &initial["structuredContent"];
    assert!(
        !structured.to_string().contains("\"cursor\""),
        "the replayable query carries source identity; no cursor duplicate: {structured}"
    );
    let rows = structured["results"].as_array().expect("MCP result rows");
    let continuation = |row: usize| rows[row]["data"]["next"]["continue"]["query"].clone();
    let (first_next, second_next) = (continuation(0), continuation(1));
    assert_eq!(
        first_next["snapshot"].as_str().map(str::len),
        Some(64),
        "{first_next}"
    );

    std::fs::write(&first_path, "changed-1\nchanged-2\n").expect("mutate first source");
    let resumed = runtime
        .execute(
            "resume-local-snapshot-batch".into(),
            "localFetch".into(),
            json!({"queries":[first_next, second_next]}),
        )
        .await
        .expect("typed batch rows");
    let rows = resumed.structured_content["results"]
        .as_array()
        .expect("resumed rows");
    assert_eq!(rows[0]["status"], "error", "{}", rows[0]);
    assert_eq!(rows[0]["data"]["errorCode"], "staleSnapshot", "{}", rows[0]);
    let restart = &rows[0]["data"]["next"]["restart"]["query"];
    assert_eq!(restart["path"], json!(first_path), "{}", rows[0]);
    assert!(restart.get("snapshot").is_none(), "{restart}");
    assert!(restart.get("offset").is_none(), "{restart}");
    assert_eq!(rows[1]["data"]["content"], "second-2\n", "{}", rows[1]);
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
async fn structure_search_dispatches_on_both_surfaces_and_owns_file_discovery() {
    let workspace = Workspace::new();
    workspace.write("src/main.ts", "export const a = 1;\n");
    workspace.write("docs/readme.md", "# docs\n");
    let mcp = ToolRuntime::from_host(HostOptions {
        cwd: Some(workspace.workspace.clone()),
        env: Some(BTreeMap::from([("ENABLE_LOCAL".into(), "true".into())])),
        surface: RuntimeSurface::Mcp,
        ..HostOptions::default()
    })
    .expect("mcp runtime");
    assert!(mcp.is_available("structureSearch"));
    mcp.close().await;

    let runtime = workspace.runtime(&[]);
    assert!(runtime.is_available("structureSearch"));
    let tree = call(
        &runtime,
        "structureSearch",
        json!({"operation":"tree","path":workspace.workspace,"maxDepth":1}),
    )
    .await
    .expect("tree");
    assert_eq!(
        row_status(&tree),
        "success",
        "{:?}",
        tree.structured_content
    );
    let rendered = serde_json::to_string(row_data(&tree)).expect("json");
    assert!(
        rendered.contains("src/") && rendered.contains("docs/"),
        "{rendered}"
    );

    let files = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":workspace.workspace,"names":["*.ts"],"entryType":"f"}),
    )
    .await
    .expect("files");
    assert_eq!(
        row_status(&files),
        "success",
        "{:?}",
        files.structured_content
    );
    let rendered = serde_json::to_string(row_data(&files)).expect("json");
    assert!(
        rendered.contains("main.ts") && !rendered.contains("readme.md"),
        "{rendered}"
    );

    let retired = call(
        &runtime,
        "astSearch",
        json!({"operation":"files","path":workspace.workspace}),
    )
    .await
    .expect_err("astSearch no longer discovers files");
    assert_eq!(retired.code, "invalidInput");
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
    // Beta enables astTopology on MCP; astRewrite mutates files and is CLI-only.
    assert!(runtime.is_available("astTopology"));
    assert!(!runtime.is_available("astRewrite"));
    let catalog = runtime.catalog().expect("catalog");
    let rewrite = catalog["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "astRewrite")
        .expect("astRewrite entry");
    assert_eq!(rewrite["available"], false);
    assert_eq!(rewrite["unavailableReason"], "cliOnly");
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
async fn local_fetch_denial_echoes_the_relative_request_not_an_expanded_prefix() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "localFetch",
        json!({"path": "../../octocode-outside-probe.txt"}),
    )
    .await
    .expect("typed denial row");
    assert_eq!(row_status(&outcome), "error");
    let error = row_data(&outcome)["error"]
        .as_str()
        .expect("error")
        .to_owned();
    let denied_path = error.split(" is outside").next().unwrap_or_default();
    assert_eq!(
        denied_path, "Path '../../octocode-outside-probe.txt'",
        "{error}"
    );
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    let parent = std::path::Path::new(crate_dir)
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate grandparent");
    assert!(!denied_path.contains(&*parent.to_string_lossy()), "{error}");
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
    assert_eq!(row_data(&outcome)["errorCode"], "pathOutsideAllowedRoots");
    runtime.close().await;
}

/// Every local tool reports a sandbox refusal under one dedicated code, and
/// the recovery hint is keyed on that code.
#[tokio::test]
async fn local_tools_share_the_path_outside_allowed_roots_code() {
    let workspace = Workspace::new();
    let outside = workspace.write_outside_allowed_roots("denied.rs", "pub fn denied() {}\n");
    let runtime = workspace.runtime(&[]);
    for (tool, query) in [
        ("localFetch", json!({"path": outside})),
        (
            "localSearch",
            json!({"path": outside, "searchText": "denied"}),
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("typed denial");
        assert_eq!(row_status(&outcome), "error", "{tool}");
        let data = row_data(&outcome);
        assert_eq!(
            data["errorCode"], "pathOutsideAllowedRoots",
            "{tool}: {data}"
        );
        assert!(
            data["hints"].to_string().contains("ALLOWED_PATHS"),
            "{tool}: {data}"
        );
    }
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

/// Resolve an AST row path against the envelope `base` the runtime attaches.
fn resolve_ast_row_path(
    outcome: &octocode_native::runtime::ToolOutcome,
    path: &str,
) -> std::path::PathBuf {
    let base = outcome.structured_content["base"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "astSearch rows must carry a base: {}",
                outcome.structured_content
            )
        });
    std::path::Path::new(base).join(path)
}

#[tokio::test]
async fn ast_search_match_and_symbol_paths_resolve_against_the_base() {
    let workspace = Workspace::new();
    let source = workspace.write("pkg/src/lib.rs", "pub fn needle() { helper(1); }\n");
    let scope = source.parent().expect("scope").to_path_buf();
    let runtime = workspace.runtime(&[]);

    let matched = call(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":scope,"langType":"rust","pattern":"helper($A)"}),
    )
    .await
    .expect("astSearch match");
    let path = row_data(&matched)["files"][0]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("match row path: {}", matched.structured_content));
    assert!(
        resolve_ast_row_path(&matched, path).is_file(),
        "base + match path must resolve: {}",
        matched.structured_content
    );

    let symbols = call(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":scope}),
    )
    .await
    .expect("astSearch symbols");
    let path = row_data(&symbols)["declarations"][0]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("symbol row path: {}", symbols.structured_content));
    assert!(
        resolve_ast_row_path(&symbols, path).is_file(),
        "base + symbol path must resolve: {}",
        symbols.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn ast_topology_dead_code_verify_references_is_a_valid_lsp_query() {
    let workspace = Workspace::new();
    workspace.write(
        "package.json",
        r#"{"name":"fixture","main":"src/index.ts"}"#,
    );
    workspace.write(
        "src/index.ts",
        "import { used } from './used';\nexport const main = used;\n",
    );
    workspace.write("src/used.ts", "export const used = 1;\n");
    workspace.write("src/orphan.ts", "export function orphan() { return 2; }\n");
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    let outcome = call(
        &runtime,
        "astTopology",
        json!({"analysis":"deadCode","path":workspace.workspace}),
    )
    .await
    .expect("astTopology deadCode");
    assert_ne!(
        row_status(&outcome),
        "error",
        "deadCode row must not be withheld: {}",
        outcome.structured_content
    );
    let verify = &row_data(&outcome)["next"]["verifyReferences"];
    assert_eq!(
        verify["tool"], "lspSearch",
        "{}",
        outcome.structured_content
    );
    let mut query = verify["query"].clone();
    assert!(query.get("format").is_none(), "{query}");
    let uri = query["uri"].as_str().expect("uri");
    assert!(
        std::path::Path::new(uri).is_file(),
        "uri must be a real file: {uri}"
    );
    query["reasoning"] = json!("Verify the dead-code candidate.");
    octocode_native::contracts::validate_query("lspSearch", query)
        .expect("verifyReferences must validate against the lspSearch input contract");
    runtime.close().await;
}

#[tokio::test]
async fn workspace_root_symbol_queries_infer_the_server_from_project_markers() {
    let workspace = Workspace::new();
    workspace.write("Cargo.toml", "[package]\nname = \"demo\"\n");
    let config_path = workspace.write(
        ".octocode/lsp.json",
        r#"{"languageServers":{".rs":{"command":"missing-rust-lsp-for-root-test","args":[],"languageId":"rust"}}}"#,
    );
    let runtime = workspace.runtime(&[(
        "OCTOCODE_LSP_CONFIG",
        config_path.to_string_lossy().into_owned(),
    )]);
    let outcome = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "workspaceSymbol",
            "workspaceRoot": workspace.workspace,
            "symbolName": "main"
        }),
    )
    .await
    .expect("typed workspace-root result");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        !rendered.contains("No language server is configured")
            && !rendered.contains("could be inferred"),
        "workspace root must select a server from Cargo.toml: {rendered}"
    );
    assert!(
        rendered.contains("missing-rust-lsp-for-root-test"),
        "the Rust route for the root should have been attempted: {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn ast_topology_result_pages_reject_a_changed_graph() {
    let workspace = Workspace::new();
    workspace.write(
        "graph/a.ts",
        "import { b } from './b';\nimport { c } from './c';\nexport const a = b + c;\n",
    );
    workspace.write("graph/b.ts", "export const b = 1;\n");
    workspace.write("graph/c.ts", "export const c = 2;\n");
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    let root = workspace.workspace.join("graph");
    let first = call(
        &runtime,
        "astTopology",
        json!({"analysis":"dependencies","path":root,"file":"a.ts","pageSize":1}),
    )
    .await
    .expect("first topology page");
    let next = row_data(&first)["next"]["nextPage"]["query"].clone();
    assert!(next.is_object(), "{}", row_data(&first));
    let second = call(&runtime, "astTopology", next.clone())
        .await
        .expect("unchanged second page");
    assert_ne!(row_status(&second), "error", "{}", row_data(&second));
    assert_eq!(
        row_data(&second)["results"].as_array().map(Vec::len),
        Some(1)
    );

    workspace.write("graph/aa.ts", "export const aa = 1;\n");
    workspace.write(
        "graph/a.ts",
        "import { aa } from './aa';\nimport { b } from './b';\nimport { c } from './c';\nexport const a = aa + b + c;\n",
    );
    let stale = call(&runtime, "astTopology", next)
        .await
        .expect("typed stale row");
    let data = row_data(&stale);
    assert_eq!(data["errorCode"], "graphSnapshotChanged", "{data}");
    assert_eq!(data["results"], json!([]), "{data}");
    let restart = &data["next"]["restartDiagnostics"]["query"];
    // Page 1 is the default, so compaction may omit it.
    assert!(restart.get("page").is_none_or(|page| page == 1), "{data}");
    assert!(restart.get("diagnosticSnapshot").is_none(), "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn local_fetch_binary_hint_does_not_blame_an_absent_match_string() {
    let workspace = Workspace::new();
    let path = workspace.write("blob.ts", b"\x00\x01\x02binary\x00".as_slice());
    let runtime = workspace.runtime(&[]);
    let outcome = call(&runtime, "localFetch", query_path(&path, json!({})))
        .await
        .expect("typed binary row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "binaryFileUnsupported", "{data}");
    let hints = outcome.structured_content.to_string();
    assert!(!hints.contains("remove matchString"), "{hints}");
    runtime.close().await;
}

#[tokio::test]
async fn local_fetch_redacted_content_is_marked_not_verbatim() {
    let workspace = Workspace::new();
    let path = workspace.write(
        "fixture.ts",
        "export const clean = 1;\nconst url = \"https://admin:hunter2secret@db.example.com:5432/app\";\n",
    );
    let clean = workspace.write("clean.ts", "export const clean = 1;\n");
    let runtime = workspace.runtime(&[]);
    for extra in [json!({}), json!({"chunkType":"lines","chunkSize":5})] {
        let outcome = call(&runtime, "localFetch", query_path(&path, extra.clone()))
            .await
            .expect("redacted read");
        let data = row_data(&outcome);
        assert!(
            data["content"]
                .as_str()
                .unwrap_or_default()
                .contains("[REDACTED"),
            "{extra}: {data}"
        );
        let warning = data["warnings"]
            .as_array()
            .and_then(|warnings| {
                warnings
                    .iter()
                    .filter_map(|w| w.as_str())
                    .find(|w| w.starts_with("redactedContent"))
            })
            .unwrap_or_else(|| panic!("{extra}: missing redaction warning: {data}"));
        assert!(warning.contains("not verbatim"), "{warning}");
    }
    let outcome = call(&runtime, "localFetch", query_path(&clean, json!({})))
        .await
        .expect("clean read");
    assert!(
        !row_data(&outcome).to_string().contains("redactedContent"),
        "{}",
        row_data(&outcome)
    );
    runtime.close().await;
}
