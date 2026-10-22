use crate::support::{self, Run as _};
use octocode_contract_types::ErrorCode;

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
    let next = data["next"]["continueWindow"]["query"].clone();
    assert!(next.is_object(), "executable continuation");

    let second = call(&runtime, "localFetch", next)
        .await
        .expect("second page");
    assert_eq!(row_data(&second)["content"].as_str(), Some("2\ttwo 😀\n"));
    runtime.close().await;
}

/// A byte page states its view's total (`pagination.total`), as a
/// line page's total rides `totalLines`: `sourceBytes` is a stats extra, so
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
        query_path(&path, json!({"unit":"bytes","length":5,"extras":[]})),
    )
    .await
    .expect("byte page");
    let data = row_data(&page);
    assert!(data.get("sourceBytes").is_none(), "{data}");
    assert_eq!(data["pagination"]["total"], text.len(), "{data}");
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
        .run_mcp(
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
    support::checks::assert_absent(structured, "cursor");
    let rows = structured["results"].as_array().expect("MCP result rows");
    let continuation =
        |row: usize| rows[row]["data"]["next"]["continueWindow"]["query"]["queries"][0].clone();
    let (first_next, second_next) = (continuation(0), continuation(1));
    assert_eq!(
        first_next["snapshot"].as_str().map(str::len),
        Some(64),
        "{first_next}"
    );

    std::fs::write(&first_path, "changed-1\nchanged-2\n").expect("mutate first source");
    let resumed = runtime
        .run(
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

/// A streamed (over 10 MiB) read of several ranges whose snapshot no longer
/// names the source restarts the whole read: every requested range, without
/// the snapshot, from the runtime's one restart owner.
#[tokio::test]
async fn a_stale_streamed_range_read_restarts_every_range() {
    let workspace = Workspace::new();
    let line = "streamed payload payload payload payload\n";
    let path = workspace.write("big.log", line.repeat(11 * 1024 * 1024 / line.len()));
    let runtime = workspace.runtime(&[]);
    let stale = call(
        &runtime,
        "localFetch",
        query_path(
            &path,
            json!({"ranges": ["1-2", "5-6"], "snapshot": "0".repeat(64)}),
        ),
    )
    .await
    .expect("typed stale row");
    let data = row_data(&stale);
    assert_eq!(data["errorCode"], "staleSnapshot", "{data}");
    let restart = &data["next"]["restart"]["query"]["queries"][0];
    assert_eq!(restart["ranges"], json!(["1-2", "5-6"]), "{data}");
    assert!(restart.get("snapshot").is_none(), "{data}");
    let again = call(
        &runtime,
        "localFetch",
        data["next"]["restart"]["query"].clone(),
    )
    .await
    .expect("restart");
    let again = row_data(&again);
    let numbered = again["content"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('\t')?.0.parse::<u32>().ok())
        .collect::<Vec<_>>();
    assert_eq!(numbered, [1, 2, 5, 6], "{again}");
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
    assert_eq!(error.code, ErrorCode::InvalidInput);
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

/// `matchPageSize:1` is `rg -m1`: one row per file, and at most one
/// continuation for the whole response, never one per file.
#[tokio::test]
async fn local_search_match_page_size_one_is_first_hit_per_file() {
    let workspace = Workspace::new();
    for index in 0..18 {
        workspace.write(
            &format!("conv/c{index:02}.cs"),
            format!("#region License\nclass C{index} {{}}\n#region Other\n"),
        );
    }
    let runtime = workspace.runtime(&[]);
    let found = call(
        &runtime,
        "localSearch",
        json!({
            "path": workspace.workspace.join("conv"),
            "matchString": "#region",
            "matchPageSize": 1,
            "extras": []
        }),
    )
    .await
    .expect("search");
    assert_eq!(
        row_status(&found),
        "success",
        "{}",
        found.structured_content
    );
    let data = row_data(&found);
    let files = data["files"].as_array().expect("files");
    assert_eq!(files.len(), 18, "{data}");
    for file in files {
        assert_eq!(file["matches"].as_array().map(Vec::len), Some(1), "{file}");
        assert!(file.get("next").is_none(), "{file}");
        assert!(file.get("hints").is_none(), "{file}");
    }
    // The rest of every file is one match page, named once.
    let next = data["next"].as_object().expect("next");
    assert_eq!(next.keys().collect::<Vec<_>>(), ["nextMatchPage"], "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn structure_search_dispatches_on_both_surfaces_and_owns_file_discovery() {
    let workspace = Workspace::new();
    workspace.write("src/main.ts", "export const a = 1;\n");
    workspace.write("docs/readme.md", "# docs\n");
    let mcp = ToolRuntime::from_host(HostOptions {
        cwd: Some(workspace.workspace.clone()),
        // The fixture home, so the user's ~/.octocode is never read or written.
        env: Some(BTreeMap::from([
            ("OCTOCODE_ENABLE_LOCAL".into(), "true".into()),
            (
                "OCTOCODE_HOME".into(),
                workspace.home.to_string_lossy().into_owned(),
            ),
        ])),
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
    assert_eq!(retired.code, ErrorCode::InvalidInput);
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

/// The markdown server is a missing command, so the row does not depend on
/// which language servers this machine has installed.
#[tokio::test]
async fn lsp_search_returns_a_typed_row_when_no_server_is_configured() {
    let workspace = Workspace::new();
    let path = workspace.write("notes.md", "# title\n");
    // Inside the workspace: anchored operations read the server config
    // through the path policy.
    let config = workspace.write(
        "config/lsp.json",
        json!({"languageServers": {".md": {
            "command": "octocode-test-missing-lsp", "args": [], "languageId": "markdown"
        }}})
        .to_string(),
    );
    let runtime =
        workspace.runtime(&[("OCTOCODE_LSP_CONFIG", config.to_string_lossy().into_owned())]);
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
    assert_eq!(
        row_status(&outcome),
        "error",
        "{}",
        outcome.structured_content
    );
    assert_eq!(
        row_data(&outcome)["errorCode"],
        "serverUnavailable",
        "{}",
        outcome.structured_content
    );
    let symbol_failure = call(
        &runtime,
        "lspSearch",
        json!({
            "operation": "definition",
            "path": path,
            "symbolName": "title",
            "line": 1
        }),
    )
    .await
    .expect("symbol recovery output satisfies its contract");
    assert_eq!(row_status(&symbol_failure), "error");
    assert_eq!(
        row_data(&symbol_failure)["errorCode"],
        "serverUnavailable",
        "{}",
        symbol_failure.structured_content
    );
    assert_eq!(
        crate::support::checks::hint(row_data(&symbol_failure), "read")["tool"],
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

/// Resolve an AST row path against the envelope `base` the runtime attaches.
fn resolve_ast_row_path(
    outcome: &octocode_native::response::outcome::ToolOutcome,
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
    support::checks::assert_absent(row_data(&outcome), "redactedContent");
    runtime.close().await;
}

/// Every local walk prunes one default directory set, and a caller
/// `exclude` glob skips more rather than replacing it. Tool-config directories
/// such as `.github` are not on it: with `includeHidden`, text, structure and
/// AST walks all reach them.
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

    let paths_of =
        |outcome: &octocode_native::response::outcome::ToolOutcome, key: &str| -> Vec<String> {
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
        json!({"path":root,"matchString":"needle","includeHidden":true,"exclude":exclude}),
    )
    .await
    .expect("localSearch");
    assert_eq!(
        paths_of(&searched, "files"),
        syntax_visible,
        "{:?}",
        searched.structured_content
    );

    let files = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":root,"include":["*.rs"],"entryType":"f","includeHidden":true,"exclude":exclude}),
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
        json!({"operation":"tree","path":root,"includeHidden":true,"exclude":exclude}),
    )
    .await
    .expect("structureSearch tree");
    // Listed entries only: a withheld notice names policy entries.
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
        json!({"operation":"match","path":root,"language":"rust","pattern":"pub fn needle() {}","includeHidden":true,"exclude":exclude}),
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

#[tokio::test]
async fn default_excludes_false_walks_dependency_directories() {
    let workspace = Workspace::new();
    workspace.write("src/app.rs", "fn needle() {}\n");
    workspace.write("node_modules/dep/index.js", "function needle() {}\n");
    let runtime = workspace.runtime(&[]);
    let names = |outcome: &octocode_native::response::outcome::ToolOutcome| {
        serde_json::to_string(&outcome.structured_content).unwrap_or_default()
    };
    let pruned = call(
        &runtime,
        "localSearch",
        json!({"path":workspace.workspace,"matchString":"needle"}),
    )
    .await
    .expect("default prune");
    // Not searched, and disclosed separately from matched files.
    assert!(
        !serde_json::to_string(&row_data(&pruned)["files"])
            .unwrap_or_default()
            .contains("node_modules"),
        "{}",
        names(&pruned)
    );
    assert_eq!(
        row_data(&pruned)["skippedPaths"],
        json!({"entries":[{"path":"node_modules","cause":"default-excluded"}],"sampled":false}),
        "{}",
        names(&pruned)
    );
    // `skippedPaths` is the one disclosure: no warning restates it.
    assert!(
        row_data(&pruned)["warnings"]
            .as_array()
            .is_none_or(|warnings| warnings
                .iter()
                .all(|warning| !warning.to_string().contains("Default excludes"))),
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
    let mut paths = row_data(&all)["files"]
        .as_array()
        .expect("matched files")
        .iter()
        .map(|file| file["path"].as_str().expect("match path"))
        .collect::<Vec<_>>();
    paths.sort_unstable();
    assert_eq!(paths, ["node_modules/dep/index.js", "src/app.rs"]);
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

/// Credential-named files stay withheld by the security path policy, and
/// every local tool says so truthfully: a policy denial with what it matched
/// and that no flag or config setting lifts it, never "check spelling" or
/// "retry the path". Source folders named `secrets/` or `private/` are not
/// withheld: only the filename pattern is.
#[tokio::test]
async fn policy_withheld_files_are_disclosed_never_advised_as_retry_or_spelling() {
    let workspace = Workspace::new();
    let secret = workspace.write(
        "src/credentials.json",
        "{\"name\": \"ISecretStorageService\"}\n",
    );
    workspace.write(
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
    let notice = "1 entry withheld by path policy: 1 credential file.";

    let searched = call(
        &runtime,
        "localSearch",
        json!({"path":root,"matchString":"ISecretStorageService"}),
    )
    .await
    .expect("search");
    let text = searched.structured_content.to_string();
    assert_ne!(row_status(&searched), "empty", "{text}");
    assert!(
        text.contains("src/secrets/common/secrets.ts")
            || text.contains("secrets/common/secrets.ts"),
        "{text}"
    );
    assert!(text.contains("private/store.ts"), "{text}");
    assert!(!text.contains("credentials.json"), "{text}");
    assert!(text.contains(notice), "{text}");

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
                .contains("withheld by the security path policy (a credential file name)")),
        "{text}"
    );
    assert!(!text.to_lowercase().contains("then retry"), "{text}");
    assert!(text.contains("Do not retry or respell"), "{text}");

    let listed = call(
        &runtime,
        "structureSearch",
        json!({"operation":"files","path":root,"extensions":["json"]}),
    )
    .await
    .expect("files");
    let text = listed.structured_content.to_string();
    assert!(text.contains(notice), "{text}");
    let tree = call(
        &runtime,
        "structureSearch",
        json!({"operation":"tree","path":root}),
    )
    .await
    .expect("tree");
    let text = tree.structured_content.to_string();
    assert!(text.contains(notice), "{text}");
    assert!(
        text.contains("secrets") && text.contains("private"),
        "{text}"
    );
    runtime.close().await;
}

/// A localSearch that found matches still discloses what the default
/// excludes skipped (pruned build dirs and generated files such as
/// `*.min.js`), with the same search over them one call away; the escape
/// flag really searches them. Lockfiles pin dependency versions: evidence,
/// searched by default.
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
    let files_of = |outcome: &octocode_native::response::outcome::ToolOutcome| -> Vec<String> {
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
    assert_eq!(
        files_of(&found),
        ["Cargo.lock", "drivers/scsi/core.c"],
        "{text}"
    );
    assert_eq!(
        row_data(&found)["skippedPaths"],
        json!({"entries":[
            {"path":"dist","cause":"default-excluded"},
            {"path":"drivers/target","cause":"default-excluded"},
            {"path":"vendor/lib.min.js","cause":"default-excluded"}
        ],"sampled":false}),
        "{text}"
    );
    assert!(row_data(&found).get("warnings").is_none(), "{text}");
    let hint = crate::support::checks::hint(row_data(&found), "includeSkipped");
    let lead = (!hint.is_null())
        .then_some(&hint)
        .or_else(|| {
            found
                .structured_content
                .pointer("/results/0/data/next/includeSkipped")
        })
        .unwrap_or_else(|| panic!("includeSkipped lead: {text}"));
    assert_eq!(
        lead["query"]["queries"][0]["defaultExcludes"], false,
        "{lead}"
    );

    let all = call(&runtime, "localSearch", lead["query"].clone())
        .await
        .expect("rerun");
    assert_eq!(
        files_of(&all),
        [
            "Cargo.lock",
            "dist/bundle.js",
            "drivers/scsi/core.c",
            "drivers/target/transport.c",
            "vendor/lib.min.js",
        ],
        "{}",
        all.structured_content
    );
    runtime.close().await;
}

/// A regex read past the match limit says how to reach the unselected
/// matches without restating a call, on both surfaces, also on the first
/// page of a read that continues. The search lead that pages every match is
/// opt-in (`extras: ["hints"]`) and then rides `hints` (pages ride `next`).
#[tokio::test]
async fn regex_match_limit_warning_and_its_opt_in_search_lead() {
    let workspace = Workspace::new();
    let body: String = (1..=100_005).map(|n| format!("hit {n}\n")).collect();
    let path = workspace.write("hits.txt", body);
    let runtime = workspace.runtime(&[]);
    for hinted in [false, true] {
        let mut row = json!({"path":path.to_string_lossy(),"matchString":"hit","regex":"rust"});
        if hinted {
            row["extras"] = json!(["hints"]);
        }
        let query = json!({"queries":[row]});
        for mcp in [false, true] {
            let envelope = if mcp {
                runtime
                    .run_mcp("limit-mcp".into(), "localFetch".into(), query.clone())
                    .await
                    .expect("mcp call")["structuredContent"]
                    .clone()
            } else {
                runtime
                    .run("limit-cli".into(), "localFetch".into(), query.clone())
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
                data["next"]["continueWindow"].is_object(),
                "mcp={mcp}: the read pages"
            );
            assert!(
                warning.ends_with("narrow the regex to select them."),
                "mcp={mcp}: {warning}"
            );
            if hinted {
                assert_eq!(
                    crate::support::checks::hint(data, "findText")["tool"],
                    "localSearch",
                    "mcp={mcp}: {data}"
                );
            } else {
                assert!(
                    data.get("hints")
                        .is_none_or(|hints| hints.get("findText").is_none()),
                    "mcp={mcp}: the search lead is opt-in: {data}"
                );
            }
        }
    }
}

/// A value clipped to the display bound is not a gap in the rows: the row
/// stays complete, the clip is reported by `clipped` and `originalChars`, and its
/// `expandValues` lead is optional, so the call is not partial.
#[tokio::test]
async fn a_clipped_match_value_is_not_a_partial_result() {
    use octocode_native::response::exit::ExitClass;
    let workspace = Workspace::new();
    workspace.write("min.js", format!("needle {}\n", "x ".repeat(5_000)));
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .run(
            "clipped-value".into(),
            "localSearch".into(),
            json!({"queries":[{"path":workspace.workspace.to_string_lossy(),"matchString":"needle"}]}),
        )
        .await
        .expect("localSearch");
    let data = row_data(&outcome);
    let matched = &data["files"][0]["matches"][0];
    assert!(matched["originalChars"].as_u64().is_some(), "{data}");
    assert_eq!(matched["clipped"], true, "{data}");
    assert!(data.get("isPartial").is_none(), "{data}");
    assert!(data["next"]["expandValues"].is_object(), "{data}");
    assert_eq!(outcome.exit_class(), ExitClass::Success, "{data}");
    runtime.close().await;
}

/// On a line longer than a read page the match is far from the line's start:
/// the `expandValues` read windows the line by bytes at the match (a
/// multi-byte prefix included), not at the line's start.
#[tokio::test]
async fn expand_values_of_a_long_line_windows_around_the_match() {
    let workspace = Workspace::new();
    let line = format!(
        "{}needleHere{}",
        "é ".repeat(100_000),
        " tail".repeat(1_000)
    );
    workspace.write("min.js", format!("head\n{line}\ntail\n"));
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .run(
            "long-line".into(),
            "localSearch".into(),
            json!({"queries":[{"path":workspace.workspace.to_string_lossy(),"matchString":"needleHere"}]}),
        )
        .await
        .expect("localSearch");
    let data = row_data(&outcome);
    let lead = &data["next"]["expandValues"];
    let read = &lead["query"]["queries"][0];
    assert_eq!(lead["tool"], "localFetch", "{data}");
    assert_eq!(read["unit"], "bytes", "{data}");
    assert_eq!(read["ranges"], json!(["2-2"]), "{data}");
    let offset = read["offset"].as_u64().expect("offset") as usize;
    assert!(offset > 100_000, "{read}");
    let page = runtime
        .run(
            "long-line-read".into(),
            "localFetch".into(),
            lead["query"].clone(),
        )
        .await
        .expect("localFetch");
    let page = row_data(&page);
    assert!(
        page["content"]
            .as_str()
            .is_some_and(|text| text.contains("needleHere")),
        "the window holds the match: {page}"
    );
    runtime.close().await;
}
