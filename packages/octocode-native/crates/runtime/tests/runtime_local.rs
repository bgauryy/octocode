// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::panic)]

use crate::support;

use serde_json::json;
use std::collections::BTreeMap;
use support::{Workspace, call, query_path, row_data, row_status};

use octocode_native::config::RuntimeSurface;
use octocode_native::runtime::{HostOptions, ToolRuntime};

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
            .execute(
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
        .execute(
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
        .execute(
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
async fn local_fetch_pages_and_unions_through_the_runtime() {
    let workspace = Workspace::new();
    let path = workspace.write("source.txt", "one\ntwo 😀\nthree\n");
    let runtime = workspace.runtime(&[]);
    let first = call(
        &runtime,
        "localFetch",
        query_path(&path, json!({"unit":"lines","length":1})),
    )
    .await
    .expect("first page");
    assert_eq!(row_status(&first), "success");
    let data = row_data(&first);
    assert_eq!(data["content"].as_str(), Some("1\tone\n"));
    let next = data["next"]["continue"]["query"].clone();
    assert!(next.is_object(), "executable continuation");

    let second = call(&runtime, "localFetch", next)
        .await
        .expect("second page");
    assert_eq!(row_data(&second)["content"].as_str(), Some("2\ttwo 😀\n"));
    runtime.close().await;
}

/// P6: a byte page states its view's total (`pagination.totalBytes`), as a
/// line page's total rides `totalLines`: `sourceBytes` is debug-only, so
/// without it the page would name no total at all.
#[tokio::test]
async fn local_fetch_byte_pages_state_their_total() {
    let workspace = Workspace::new();
    let text = "one\ntwo 😀\nthree\n";
    let path = workspace.write("bytes.txt", text);
    let runtime = workspace.runtime(&[]);
    let page = call(
        &runtime,
        "localFetch",
        query_path(&path, json!({"unit":"bytes","length":5,"debug":false})),
    )
    .await
    .expect("byte page");
    let data = row_data(&page);
    assert!(data.get("sourceBytes").is_none(), "{data}");
    assert_eq!(data["pagination"]["totalBytes"], text.len(), "{data}");
    assert_eq!(data["pagination"]["hasMore"], true, "{data}");
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
                    "unit":"lines",
                    "length":1,
                    "mainGoal": "test", "reasoning":"Page the first snapshot fixture."
                },
                {
                    "path":second_path,
                    "unit":"lines",
                    "length":1,
                    "mainGoal": "test", "reasoning":"Page the second snapshot fixture."
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
    let continuation =
        |row: usize| rows[row]["data"]["next"]["continue"]["query"]["queries"][0].clone();
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
    let restart = &rows[0]["data"]["next"]["restart"]["query"]["queries"][0];
    assert_eq!(restart["path"], first_next["path"], "{}", rows[0]);
    assert!(restart.get("snapshot").is_none(), "{restart}");
    assert!(restart.get("offset").is_none(), "{restart}");
    assert_eq!(rows[1]["data"]["content"], "2\tsecond-2\n", "{}", rows[1]);
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
            "matchString": "needle",
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
        env: Some(BTreeMap::from([(
            "OCTOCODE_ENABLE_LOCAL".into(),
            "true".into(),
        )])),
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
    // A directory is a `{dir}` group, or a bare `name/` entry without one.
    let listed = |dir: &str| {
        rendered.contains(&format!("{dir}/")) || rendered.contains(&format!("\"dir\":\"{dir}\""))
    };
    assert!(listed("src") && listed("docs"), "{rendered}");

    let files = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":workspace.workspace,"include":["*.ts"],"entryType":"f"}),
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
    let runtime = workspace.runtime(&[("OCTOCODE_ENABLE_LOCAL", "false".into())]);
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
            "language": "typescript",
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
            "operation": "cycles",
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
            ("OCTOCODE_ENABLE_LOCAL".into(), "true".into()),
            ("OCTOCODE_BETA".into(), "true".into()),
        ])),
        surface: RuntimeSurface::Mcp,
        ..HostOptions::default()
    })
    .expect("embedded runtime with explicit environment");
    // Beta never opens a CLI-only tool on MCP.
    assert!(runtime.is_available("astSearch"));
    let catalog = runtime.catalog().expect("catalog");
    for name in ["astTopology", "astRewrite"] {
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
        .execute_mcp(
            "mcp-topology".into(),
            "astTopology".into(),
            json!({"queries":[{"operation":"cycles","path":workspace.workspace}]}),
        )
        .await
        .expect_err("astTopology is CLI-only");
    assert_eq!(error.code, "toolUnavailable");
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
            "operation": "cycles",
            "path": workspace.workspace
        }),
    )
    .await
    .expect("astTopology");
    assert_eq!(row_data(&outcome)["operation"], "cycles");
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
            "path": path
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
            "path": path,
            "symbolName": "title",
            "lineHint": 1
        }),
    )
    .await
    .expect("symbol recovery output satisfies its contract");
    assert_eq!(row_status(&symbol_failure), "error");
    assert_eq!(
        row_data(&symbol_failure)["hints"]["read"]["tool"],
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
            "path": outside
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
    assert_eq!(row_data(&outcome)["errorCode"], "outsideAllowedRoots");
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
            json!({"path": outside, "matchString": "denied"}),
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("typed denial");
        assert_eq!(row_status(&outcome), "error", "{tool}");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "outsideAllowedRoots", "{tool}: {data}");
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
            "path": path
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
    let base = outcome.structured_content["root"]
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
        json!({"operation":"match","path":scope,"language":"rust","pattern":"helper($A)"}),
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
    let path = row_data(&symbols)["files"][0]["path"]
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
        json!({"operation":"deadCode","path":workspace.workspace}),
    )
    .await
    .expect("astTopology deadCode");
    assert_ne!(
        row_status(&outcome),
        "error",
        "deadCode row must not be withheld: {}",
        outcome.structured_content
    );
    let verify = &row_data(&outcome)["hints"]["references"];
    assert_eq!(
        verify["tool"], "lspSearch",
        "{}",
        outcome.structured_content
    );
    let mut query = verify["query"]["queries"][0].clone();
    assert!(query.get("format").is_none(), "{query}");
    let path = query["path"].as_str().expect("path");
    assert!(
        workspace.workspace.join(path).is_file(),
        "path must name a real file: {path}"
    );
    query["reasoning"] = json!("Verify the dead-code candidate.");
    octocode_native::contracts::validate_query("lspSearch", query)
        .expect("the references lead must validate against the lspSearch input contract");
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
        json!({"operation":"dependencies","path":root,"source":"a.ts","pageSize":1}),
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
    assert_eq!(data["errorCode"], "staleSnapshot", "{data}");
    assert_eq!(data["results"], json!([]), "{data}");
    let restart = &data["next"]["restartDiagnostics"]["query"]["queries"][0];
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
    for extra in [json!({}), json!({"unit":"lines","length":5})] {
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

/// Every local walk prunes one default directory set, and a caller
/// `exclude` glob skips more rather than replacing it. `localSearch` (search-safe)
/// also prunes tool-config directories such as `.github`; structure and AST
/// walks (syntax-visible) keep them.
#[tokio::test]
async fn local_walks_share_one_default_prune_and_exclude_adds() {
    let workspace = Workspace::new();
    let source = "pub fn needle() {}\n";
    workspace.write("src/lib.rs", source);
    let pruned = [
        "node_modules",
        "target",
        "dist",
        "coverage",
        ".venv",
        "__pycache__",
        "DerivedData",
        "secrets",
        "extra",
    ];
    for dir in pruned {
        workspace.write(&format!("{dir}/lib.rs"), source);
    }
    workspace.write(".github/lib.rs", source);
    let runtime = workspace.runtime(&[]);
    let root = workspace.workspace.clone();
    let exclude = json!(["extra"]);
    let workspace_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    let paths_of = |outcome: &octocode_native::runtime::ToolOutcome, key: &str| -> Vec<String> {
        let data = row_data(outcome);
        let rows = data[key]
            .as_array()
            .unwrap_or_else(|| panic!("{key} rows: {}", outcome.structured_content));
        // structureSearch groups list entries by `dir`: `dir/<name> (<size>)`.
        let mut paths = rows
            .iter()
            .flat_map(|row| match (row["dir"].as_str(), row["files"].as_array()) {
                (Some(dir), Some(entries)) => entries
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(|entry| format!("{dir}/{entry}"))
                    .collect::<Vec<_>>(),
                _ => row
                    .as_str()
                    .or_else(|| row["path"].as_str())
                    .map(str::to_owned)
                    .into_iter()
                    .collect(),
            })
            .map(|path| {
                let path = path.as_str();
                path.split(' ').next().unwrap_or(path).replace('\\', "/")
            })
            .filter(|path| path.ends_with(".rs"))
            .map(|path| {
                // Rows are relative to `base`, the workspace's parent.
                path.strip_prefix(&format!("{}/", root.to_string_lossy()))
                    .or_else(|| path.strip_prefix(&format!("{workspace_name}/")))
                    .unwrap_or(&path)
                    .to_owned()
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths
    };
    let syntax_visible = vec![".github/lib.rs".to_owned(), "src/lib.rs".to_owned()];

    let searched = call(
        &runtime,
        "localSearch",
        json!({"path":root,"matchString":"needle","hidden":true,"exclude":exclude}),
    )
    .await
    .expect("localSearch");
    assert_eq!(paths_of(&searched, "files"), ["src/lib.rs"]);

    let files = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":root,"include":["*.rs"],"entryType":"f","exclude":exclude}),
    )
    .await
    .expect("structureSearch files");
    assert_eq!(
        paths_of(&files, "files"),
        syntax_visible,
        "{:?}",
        files.structured_content
    );

    let tree = call(
        &runtime,
        "structureSearch",
        json!({"operation":"tree","path":root,"hidden":true,"exclude":exclude}),
    )
    .await
    .expect("structureSearch tree");
    // Listed entries only: the withheld notice names `secrets/` by policy.
    let rendered = serde_json::to_string(&row_data(&tree)["files"]).expect("json");
    for dir in pruned {
        assert!(
            !rendered.contains(&format!("{dir}/")),
            "{dir} in {rendered}"
        );
    }
    let listed = |dir: &str| {
        rendered.contains(&format!("{dir}/")) || rendered.contains(&format!("\"dir\":\"{dir}\""))
    };
    assert!(listed(".github") && listed("src"), "{rendered}");

    let matched = call(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":root,"language":"rust","pattern":"pub fn needle() {}","hidden":true,"exclude":exclude}),
    )
    .await
    .expect("astSearch match");
    assert_eq!(
        paths_of(&matched, "files"),
        syntax_visible,
        "{:?}",
        matched.structured_content
    );

    let symbols = call(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":root,"exclude":exclude}),
    )
    .await
    .expect("astSearch symbols");
    let symbol_paths = paths_of(&symbols, "files");
    assert!(
        symbol_paths
            .iter()
            .all(|path| path == "src/lib.rs" || path == ".github/lib.rs")
            && symbol_paths.contains(&"src/lib.rs".to_owned()),
        "{symbol_paths:?}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn continuations_carry_the_input_brief_and_replay() {
    let workspace = Workspace::new();
    for name in ["a", "b", "c"] {
        workspace.write(&format!("{name}.txt"), "needle\n");
    }
    let runtime = workspace.runtime(&[]);
    let first = runtime
        .execute(
            "page-1".into(),
            "localSearch".into(),
            json!({"queries":[{"path":workspace.workspace,"matchString":"needle","pageSize":1,
                "mainGoal":"Find every needle file for the audit.","reasoning":"List files one page at a time."}]}),
        )
        .await
        .expect("first page");
    let next = &first.structured_content["results"][0]["data"]["next"]["nextPage"];
    let query = next["query"].clone();
    assert_eq!(
        query["queries"][0]["mainGoal"], "Find every needle file for the audit.",
        "{next}"
    );
    assert_eq!(
        query["queries"][0]["reasoning"], "List files one page at a time.",
        "{next}"
    );
    // The continuation is the whole call: it runs exactly as emitted.
    let second = runtime
        .execute(
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

#[tokio::test]
async fn default_excludes_false_walks_dependency_directories() {
    let workspace = Workspace::new();
    workspace.write("src/app.rs", "fn needle() {}\n");
    workspace.write("node_modules/dep/index.js", "function needle() {}\n");
    let runtime = workspace.runtime(&[]);
    let names = |outcome: &octocode_native::runtime::ToolOutcome| {
        serde_json::to_string(&outcome.structured_content).unwrap_or_default()
    };
    let pruned = call(
        &runtime,
        "localSearch",
        json!({"path":workspace.workspace,"matchString":"needle"}),
    )
    .await
    .expect("default prune");
    // Not searched, and disclosed: only the warning names it.
    assert!(
        !serde_json::to_string(&row_data(&pruned)["files"])
            .unwrap_or_default()
            .contains("node_modules"),
        "{}",
        names(&pruned)
    );
    assert!(
        names(&pruned).contains("Default excludes skipped 1 dir (node_modules/)"),
        "{}",
        names(&pruned)
    );
    let all = call(
        &runtime,
        "localSearch",
        json!({"path":workspace.workspace,"matchString":"needle","defaultExcludes":false}),
    )
    .await
    .expect("defaults off");
    assert!(names(&all).contains("node_modules"), "{}", names(&all));
    let tree = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":workspace.workspace,"defaultExcludes":false}),
    )
    .await
    .expect("structure defaults off");
    assert!(names(&tree).contains("index.js"), "{}", names(&tree));
    runtime.close().await;
}

/// Directories named like credential stores (`secrets/`, `private/`) stay
/// withheld by the security path policy, and every local tool says so
/// truthfully: a policy denial with what it matched and that no flag or
/// config setting lifts it — never "check spelling" or "retry the path".
#[tokio::test]
async fn policy_withheld_dirs_are_disclosed_never_advised_as_retry_or_spelling() {
    let workspace = Workspace::new();
    let secret = workspace.write(
        "src/secrets/common/secrets.ts",
        "export interface ISecretStorageService {}\n",
    );
    workspace.write(
        "src/private/store.ts",
        "export interface ISecretStorageService {}\n",
    );
    workspace.write("src/other.ts", "export const other = 1;\n");
    let runtime = workspace.runtime(&[]);
    let root = workspace.workspace.join("src");

    let searched = call(
        &runtime,
        "localSearch",
        json!({"path":root,"matchString":"ISecretStorageService"}),
    )
    .await
    .expect("search");
    let text = searched.structured_content.to_string();
    assert_eq!(row_status(&searched), "empty", "{text}");
    assert!(
        text.contains("2 entries withheld by path policy: security-policy dirs private/, secrets/"),
        "{text}"
    );
    assert!(
        text.contains("No flag or config setting lifts it"),
        "{text}"
    );
    assert!(!text.contains("shorter term"), "{text}");
    assert!(!text.contains("all-lowercase"), "{text}");

    let fetched = call(&runtime, "localFetch", json!({"path":secret}))
        .await
        .expect("fetch");
    let data = row_data(&fetched);
    let text = fetched.structured_content.to_string();
    assert_eq!(data["errorCode"], "pathPolicyDenied", "{text}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|error| error
                .contains("withheld by the security path policy (a `secrets/` directory)")),
        "{text}"
    );
    assert!(!text.to_lowercase().contains("then retry"), "{text}");
    assert!(text.contains("Do not retry or respell"), "{text}");

    let listed = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":root,"extensions":["ts"]}),
    )
    .await
    .expect("files");
    let text = listed.structured_content.to_string();
    assert!(
        text.contains("2 entries withheld by path policy: security-policy dirs private/, secrets/"),
        "{text}"
    );
    let tree = call(
        &runtime,
        "structureSearch",
        json!({"operation":"tree","path":root}),
    )
    .await
    .expect("tree");
    let text = tree.structured_content.to_string();
    assert!(
        text.contains("withheld by path policy: security-policy dirs private/, secrets/"),
        "{text}"
    );
    runtime.close().await;
}

/// A localSearch that found matches still discloses what the default
/// excludes skipped (pruned build dirs and generated files such as
/// `*.lock`/`*.min.js`), with the same search over them one call away; the
/// escape flag really searches them.
#[tokio::test]
async fn local_search_discloses_default_excluded_dirs_and_files_with_a_rerun() {
    let workspace = Workspace::new();
    workspace.write("drivers/scsi/core.c", "transport_free_cmd(cmd);\n");
    workspace.write(
        "drivers/target/transport.c",
        "void transport_free_cmd(void) {}\n",
    );
    workspace.write("dist/bundle.js", "transport_free_cmd();\n");
    workspace.write("Cargo.lock", "transport_free_cmd = 1\n");
    workspace.write("vendor/lib.min.js", "transport_free_cmd();\n");
    let runtime = workspace.runtime(&[]);
    let root = workspace.workspace.clone();
    let files_of = |outcome: &octocode_native::runtime::ToolOutcome| -> Vec<String> {
        let mut files = row_data(outcome)["files"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|file| file["path"].as_str().or_else(|| file.as_str()))
            .map(|path| path.split(' ').next().unwrap_or(path).to_owned())
            .collect::<Vec<_>>();
        files.sort();
        files
    };

    let found = call(
        &runtime,
        "localSearch",
        json!({"path":root,"matchString":"transport_free_cmd","resultView":"files"}),
    )
    .await
    .expect("search");
    let text = found.structured_content.to_string();
    assert_eq!(files_of(&found).len(), 1, "{text}");
    assert!(
        text.contains("Default excludes skipped 2 dirs (dist/, target/) and 2 files (*.lock, *.min.js); the same search with defaultExcludes:false covers them."),
        "{text}"
    );
    let lead = found
        .structured_content
        .pointer("/results/0/data/hints/includeIgnored")
        .or_else(|| {
            found
                .structured_content
                .pointer("/results/0/data/next/includeIgnored")
        })
        .unwrap_or_else(|| panic!("includeIgnored lead: {text}"));
    assert_eq!(
        lead["query"]["queries"][0]["defaultExcludes"], false,
        "{lead}"
    );

    let all = call(&runtime, "localSearch", lead["query"].clone())
        .await
        .expect("rerun");
    assert_eq!(files_of(&all).len(), 5, "{}", all.structured_content);
    runtime.close().await;
}

/// N11b replay: astTopology echoes a nested package `path` in the caller's
/// workspace-relative form on every page, and its next page replays as is.
#[tokio::test]
async fn topology_path_echo_matches_caller_form_on_paged_cycles() {
    let workspace = Workspace::new();
    for (name, other) in [("a", "b"), ("b", "a"), ("c", "d"), ("d", "c")] {
        workspace.write(
            &format!("packages/app/src/{name}.ts"),
            format!(
                "import {{ {other} }} from './{other}';\nexport const {name} = () => {other}();\n"
            ),
        );
    }
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    let first = call(
        &runtime,
        "astTopology",
        json!({"operation":"cycles","path":"packages/app","pageSize":1}),
    )
    .await
    .expect("cycles page 1");
    let data = row_data(&first);
    assert_eq!(data["path"], "packages/app", "{data}");
    let next = data["next"]["nextPage"]["query"].clone();
    assert!(next.is_object(), "a second page: {data}");
    let second = runtime
        .execute("test-2".into(), "astTopology".into(), next)
        .await
        .expect("cycles page 2");
    let data = row_data(&second);
    assert_eq!(data["path"], "packages/app", "{data}");
    assert_eq!(data["results"].as_array().map(Vec::len), Some(1), "{data}");
    runtime.close().await;
}

/// A regex read past the match limit names the lead that pages every
/// match, and that lead reaches the agent where the warning says, on both
/// surfaces, also on the first page of a read that continues. Leads ride
/// `hints` (pages ride `next`), so the warning must name `hints.textSearch`.
#[tokio::test]
async fn regex_match_limit_warning_names_the_lead_it_delivers() {
    let workspace = Workspace::new();
    let body: String = (1..=100_005).map(|n| format!("hit {n}\n")).collect();
    let path = workspace.write("hits.txt", body);
    let runtime = workspace.runtime(&[]);
    let query =
        json!({"queries":[{"path":path.to_string_lossy(),"matchString":"hit","regex":"rust"}]});
    for mcp in [false, true] {
        let envelope = if mcp {
            runtime
                .execute_mcp("limit-mcp".into(), "localFetch".into(), query.clone())
                .await
                .expect("mcp call")["structuredContent"]
                .clone()
        } else {
            runtime
                .execute("limit-cli".into(), "localFetch".into(), query.clone())
                .await
                .expect("cli call")
                .structured_content
        };
        let data = &envelope["results"][0]["data"];
        let warning = data["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .find(|w| w.contains("match limit"))
            .unwrap_or_else(|| panic!("mcp={mcp}: no match-limit warning: {data}"));
        assert!(
            data["next"]["continue"].is_object(),
            "mcp={mcp}: the read pages"
        );
        assert!(warning.contains("hints.textSearch"), "mcp={mcp}: {warning}");
        assert_eq!(
            data["hints"]["textSearch"]["tool"], "localSearch",
            "mcp={mcp}: {data}"
        );
    }
}
