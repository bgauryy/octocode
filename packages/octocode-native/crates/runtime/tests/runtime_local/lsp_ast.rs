//! lspSearch server discovery and astSearch row paths through the runtime.

use crate::support::{Workspace, call, row_data, row_status};
use serde_json::json;

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
