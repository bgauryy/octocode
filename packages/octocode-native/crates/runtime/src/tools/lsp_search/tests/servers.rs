//! Language servers and workspace roots: launch reports, inferred roots,
//! start-failure receipts and workspace-relative paths.

use super::*;

/// A user server for `extension` launched by `command`, through a config
/// file written under `dir`.
#[cfg(unix)]
fn user_server(
    dir: &std::path::Path,
    extension: &str,
    command: &str,
) -> super::super::LspExecutionConfig {
    let config = dir.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {extension: {
            "command": command, "args": [], "languageId": "plaintext"
        }}})
        .to_string(),
    )
    .expect("config");
    super::super::LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..super::super::LspExecutionConfig::default()
    }
}

/// An executable stand-in server: never started by these tests.
#[cfg(unix)]
fn executable(path: &std::path::Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, "#!/bin/sh\n").expect("server");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path.to_string_lossy().into_owned()
}

#[cfg(unix)]
async fn run_row(
    row: serde_json::Value,
    paths: &crate::policy::path::PathPolicy,
    execution: &super::super::LspExecutionConfig,
) -> serde_json::Value {
    super::super::execute(
        query(row),
        &crate::execution::cancel::NeverCancel,
        &fake_server_pool(),
        paths,
        execution,
        None,
    )
    .await
    .expect("row")
}

/// An anchor that resolves nowhere starts no server; the row still says
/// whether the configured server would launch.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn an_unresolved_anchor_reports_whether_its_server_would_launch() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "const a = 1;\n").expect("file");
    let installed = executable(&root.join("server"));
    let missing = root.join("missing-server").to_string_lossy().into_owned();
    for (command, available) in [(installed, true), (missing, false)] {
        let row = serde_json::json!({
            "operation": "hover", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "absent", "line": 1
        });
        let result = run_row(row, &paths, &user_server(&root, ".ts", &command)).await;
        assert_eq!(result["errorCode"], "anchorUnresolved", "{result}");
        assert_eq!(
            result["lsp"]["serverAvailable"], available,
            "{command}: {result}"
        );
    }
}

/// From a member crate, the Cargo workspace inferred for a file lies above
/// the allowed roots. With no workspaceRoot asked for, the server runs at
/// the allowed root and the row says so; an explicit root outside the
/// allowed roots still fails.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn an_inferred_root_above_the_allowed_roots_falls_back_with_a_warning() {
    let (_dir, root, _) = temp_workspace();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/a\"]\n",
    )
    .expect("workspace");
    let krate = root.join("crates/a");
    std::fs::create_dir_all(krate.join("src")).expect("crate");
    std::fs::write(krate.join("Cargo.toml"), "[package]\nname = \"a\"\n").expect("package");
    let file = krate.join("src/lib.rs");
    std::fs::write(&file, "fn f() {}\n").expect("source");
    let crates = root.join("crates");
    let paths = crate::tools::test_support::workspace_policy(&crates);
    let execution = user_server(&crates, ".rs", &executable(&crates.join("server")));
    let row = serde_json::json!({
        "operation": "hover", "path": file.to_string_lossy(),
        "symbolName": "absent", "line": 1
    });
    let inferred = run_row(row.clone(), &paths, &execution).await;
    assert_eq!(inferred["errorCode"], "anchorUnresolved", "{inferred}");
    let warning = inferred["warnings"][0].as_str().unwrap_or_default();
    assert!(
        warning.contains("outside the allowed roots") && warning.contains("runs at ."),
        "{inferred}"
    );

    let mut explicit = row;
    explicit["workspaceRoot"] = serde_json::json!(root.to_string_lossy());
    let refused = run_row(explicit, &paths, &execution).await;
    assert_eq!(refused["errorCode"], "workspaceRootInvalid", "{refused}");
}

/// A fallback names the capability it actually used: the receipt keeps it
/// instead of the operation's own provider, which never answered.
#[test]
fn the_receipt_keeps_the_provider_a_fallback_answered_with() {
    let mut fallback = serde_json::json!({"provider": "implementationProvider"})
        .as_object()
        .cloned()
        .expect("object");
    settle_provider(&mut fallback, "subtypes");
    assert_eq!(fallback["provider"], "implementationProvider");
    let mut bare = serde_json::Map::new();
    settle_provider(&mut bare, "subtypes");
    assert_eq!(bare["provider"], "typeHierarchyProvider");
}

/// A configured server that does not start names itself on the row: the
/// command, argv and source the user must fix. With no server discovered
/// there is nothing to name, and the row names nothing.
#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn a_server_that_fails_to_start_names_its_invocation() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.ts"), "const a = 1;\n").expect("file");
    std::fs::write(root.join("a.unknown"), "a\n").expect("file");
    let command = executable(&root.join("server"));
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {".ts": {
            "command": command, "args": ["--stdio"], "languageId": "plaintext"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = super::super::LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..super::super::LspExecutionConfig::default()
    };
    let hover = |file: &str| {
        serde_json::json!({
            "operation": "hover", "path": root.join(file).to_string_lossy(),
            "symbolName": "a", "line": 1
        })
    };
    let failed = run_row(hover("a.ts"), &paths, &execution).await;
    assert_eq!(failed["errorCode"], "serverUnavailable", "{failed}");
    assert_eq!(
        failed["lsp"],
        serde_json::json!({"serverAvailable": false,
            "receipt": {"command": command, "argv": ["--stdio"], "source": "user"}}),
        "{failed}"
    );
    let undiscovered = run_row(hover("a.unknown"), &paths, &execution).await;
    assert_eq!(
        undiscovered["errorCode"], "serverUnavailable",
        "{undiscovered}"
    );
    assert!(
        undiscovered["lsp"].get("receipt").is_none(),
        "{undiscovered}"
    );
}

#[test]
fn workspace_symbols_are_flat_named_and_one_based() {
    let symbol = serde_json::json!({
        "name": "run",
        "kind": 12,
        "containerName": "",
        "location": {
            "uri": "file:///repo/a.ts",
            "range": {"start": {"line": 9, "character": 2}, "end": {"line": 12, "character": 1}}
        }
    });
    assert_eq!(
        public_workspace_symbol(&symbol),
        serde_json::json!({
            "symbolName": "run",
            "kind": "function",
            "path": "/repo/a.ts",
            // A SymbolInformation range starts at the declaration, not the
            // name: no column.
            "displayRange": {"startLine": 10, "endLine": 13}
        })
    );
}

#[test]
fn workspace_symbol_accepts_directory_path() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::create_dir_all(root.join("src")).expect("src");
    let mut q = query(serde_json::json!({
        "operation": "workspaceSymbol", "symbolQuery": "f",
        "path": root.join("src").to_string_lossy()
    }));
    let resolved = resolve_paths(&mut q, &paths).expect("a directory path is the root");
    let src = root.join("src").canonicalize().expect("canonical");
    assert_eq!(resolved, src.to_string_lossy());
    assert_eq!(
        lsp_search_query::workspace_root(&q),
        Some(src.to_string_lossy().as_ref())
    );
    // Other operations still need a file.
    let mut refs = query(serde_json::json!({
        "operation": "references", "symbolName": "f", "line": 1,
        "path": root.join("src").to_string_lossy()
    }));
    assert!(resolve_paths(&mut refs, &paths).is_err());
}

#[test]
fn declared_in_paths_are_workspace_relative() {
    let root = node_at("main", Some("file:///repo/src/deep/a.ts"));
    let mut callee = node_at("map", Some("file:///repo/node_modules/x/index.d.ts"));
    callee["kind"] = serde_json::json!(12);
    let mut outside = node_at("ext", Some("file:///elsewhere/lib.d.ts"));
    outside["kind"] = serde_json::json!(12);
    let edge = |node: serde_json::Value, line: u64| HierarchyEdge {
        node,
        parent: Some(root.clone()),
        level: 1,
        sites: vec![
            serde_json::json!({"start": {"line": line, "character": 2}, "end": {"line": line, "character": 8}}),
        ],
    };
    let items = vec![
        public_edge(Expansion::OutgoingCalls, &edge(callee, 10), None),
        public_edge(Expansion::OutgoingCalls, &edge(outside, 11), None),
    ];
    let mut row = serde_json::json!({"kind": "callees", "matches": items});
    compact_calls(&mut row, &[], &repo_policy());
    let calls = row["files"][0]["matches"].as_array().expect("calls");
    let map_line = name_line("map") + 1;
    let ext_line = name_line("ext") + 1;
    assert_eq!(
        calls,
        &vec![
            serde_json::json!({"symbolName": "map", "kind": "function", "line": map_line,
                "endLine": map_line + 3, "path": "node_modules/x/index.d.ts",
                "sites": [{"line": 11, "column": 3}]}),
            serde_json::json!({"symbolName": "ext", "kind": "function", "line": ext_line,
                "endLine": ext_line + 3, "path": "/elsewhere/lib.d.ts",
                "sites": [{"line": 12, "column": 3}]}),
        ],
        "{row}"
    );
}
