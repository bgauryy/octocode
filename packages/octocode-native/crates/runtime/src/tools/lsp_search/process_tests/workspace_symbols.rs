//! Workspace-symbol provider coverage and the server result cap.

use super::*;
use crate::execution::cancel::NeverCancel;
use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::ops::Operation;
use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
use octocode_contract_types::LspSearchQuery;
use serde_json::{Value, json};

/// The `lspSearch` row `query` answers from `client` at `path` (the
/// workspace root with `root_only`) over `root`'s `*.ts` files, with its
/// continuation leads.
async fn symbol_row(
    client: &NativeLspClient,
    (root, paths): (&Path, &PathPolicy),
    query: &LspSearchQuery,
    path: &str,
    root_only: bool,
) -> Value {
    let mut sources = SourceCache::new(paths);
    let policy = snippet_policy(paths);
    let scope = crate::tools::lsp_search::scope::Scope::new(
        root.to_string_lossy().into_owned(),
        vec!["*.ts".into()],
    );
    let row = Operation {
        client,
        query,
        sources: &mut sources,
        snippet_policy: &policy,
        cancel: &NeverCancel,
        path,
        scope: &scope,
        root_only,
        line: 0,
        character: 0,
        language_id: Some("typescript"),
        budget: 16_000,
        response_window: None,
    }
    .run()
    .await
    .expect("provider answer");
    crate::tools::lsp_search::failure::with_next(query, row)
}

#[test]
fn workspace_symbol_provider_coverage_is_unknown_without_losing_semantic_rows() {
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "workspaceSymbolProvider: true, documentSymbolProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'workspace/symbol') {
    const names = msg.params.query === 'missing' ? [] : ['普通', 'Alias', 'Shared', 'Shared'];
    const uri = require('url').pathToFileURL(require('path').join(process.cwd(), 'a.ts')).href;
    const symbols = names.map((name, line) => ({name, kind: 12,
      containerName: line === 3 ? 'Shadow' : 'Module',
      location: {uri, range: {start: {line, character: line === 0 ? 3 : 0}, end: {line, character: 8}}}}));
    send({jsonrpc:'2.0', id:msg.id, result:symbols});
  } else if (msg.method === 'textDocument/documentSymbol') {
    send({jsonrpc:'2.0', id:msg.id, result:[{name:'ordinary',kind:12,
      range:{start:{line:0,character:0},end:{line:0,character:8}},
      selectionRange:{start:{line:0,character:0},end:{line:0,character:8}}}]});"#,
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        for root_only in [false, true] {
            let anchor = if root_only {
                root.to_string_lossy().into_owned()
            } else {
                path.clone()
            };
            let wire = json!({"operation":"workspaceSymbol","path":anchor,"symbolQuery":"Shared"});
            let query: LspSearchQuery = serde_json::from_value(wire.clone()).expect("query");
            let row = symbol_row(&client, (&root, &paths), &query, &anchor, root_only).await;
            assert!(
                row.get("isPartial").is_none(),
                "unknown coverage is not a proven loss: {row}"
            );
            assert!(row.get("terminalLimit").is_none());
            let found = row["matches"].as_array().expect("semantic matches");
            assert_eq!(
                found
                    .iter()
                    .map(|value| value["symbolName"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                ["Shared", "Shared", "普通", "Alias"]
            );
            assert_eq!(found[0]["containerName"], "Module");
            assert_eq!(found[1]["containerName"], "Shadow");
            let identities = found
                .iter()
                .map(|value| (value["path"].clone(), value["displayRange"].clone()))
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(identities.len(), 4);
            assert_eq!(found[2]["displayRange"], json!({"startLine":1,"endLine":1}));
            assert_row_contract(ToolId::LspSearch, &row);
        }
        // An empty workspace-symbol answer is an empty row with its notice
        // and recovery leads, not a partial one.
        for (operation, name, partial) in [
            ("workspaceSymbol", Some("missing"), false),
            ("documentSymbols", None, false),
        ] {
            let mut wire = json!({"operation":operation,"path":path});
            if let Some(name) = name {
                wire["symbolQuery"] = json!(name);
            }
            let query: LspSearchQuery = serde_json::from_value(wire.clone()).expect("query");
            let row = symbol_row(&client, (&root, &paths), &query, &path, false).await;
            assert_eq!(
                row.get("isPartial")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                partial,
                "{row}"
            );
            assert_row_contract(ToolId::LspSearch, &row);
        }
        client.stop().await.expect("fixture server closed");
    });
}

/// A file of a member crate and the directories of any member all run from
/// the Cargo workspace: one language server starts for all of them.
#[test]
fn directories_of_a_cargo_workspace_share_one_language_server() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let server = FAKE_SERVER
        .replace(
            "    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });",
            "    fs.appendFileSync(__dirname + '/roots.txt', msg.params.rootUri + '\\n');\n    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { workspaceSymbolProvider: true, textDocumentSync: 1 } } });",
        );
    assert_ne!(server, FAKE_SERVER, "the root log is injected");
    std::fs::write(&script, server).expect("fixture server");
    std::fs::write(
        root.join("lsp-servers.json"),
        json!({"languageServers": {".rs": {
            "command": script.to_string_lossy(), "args": [], "languageId": "rust"
        }}})
        .to_string(),
    )
    .expect("config");
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = ['crates/*']\n",
    )
    .expect("workspace");
    for member in ["io", "runtime"] {
        let crate_dir = root.join("crates").join(member);
        std::fs::create_dir_all(crate_dir.join("src")).expect("crate");
        std::fs::write(
            crate_dir.join("Cargo.toml"),
            format!("[package]\nname = '{member}'\n"),
        )
        .expect("manifest");
        std::fs::write(crate_dir.join("src/lib.rs"), "pub fn f() {}\n").expect("source");
    }
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        octocode_home: Some(root.clone()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    };
    let paths = crate::tools::test_support::workspace_policy(&root);
    let pool = LspClientPool::new(LspPoolOptions::default());
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    for anchor in [
        "crates/io/src/lib.rs",
        "crates/io",
        "crates/runtime/src",
        ".",
    ] {
        let wire = json!({"operation":"workspaceSymbol","symbolQuery":"f",
            "path": root.join(anchor).canonicalize().expect("anchor").to_string_lossy()});
        let row = runtime
            .block_on(crate::tools::lsp_search::execute(
                serde_json::from_value(wire).expect("query"),
                &NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
                crate::tools::lsp_search::test_answers(),
            ))
            .expect("row");
        assert!(row.get("errorCode").is_none(), "{anchor}: {row}");
    }
    runtime.block_on(pool.clear_all());
    let roots = std::fs::read_to_string(root.join("roots.txt")).expect("started");
    let expected = octocode_engine::lsp::uri::path_to_uri(&root.to_string_lossy()).expect("uri");
    assert_eq!(roots.lines().collect::<Vec<_>>(), [expected.as_str()]);
}

/// tsgo answers `workspace/symbol` with at most 256 symbols; an answer of
/// exactly that many says the server capped it, one short of it does not.
/// Only the capped answer is partial (`workspaceSymbolCoverageUnknown`).
#[test]
fn workspace_symbols_at_the_server_limit_say_capped() {
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    // Named `tsgo`, so octocode treats it as tsgo; `Cap<n>` answers n rows.
    let tsgo = root.join("tsgo");
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "workspaceSymbolProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'workspace/symbol') {
    const count = Number(msg.params.query.slice(3));
    const uri = require('url').pathToFileURL(require('path').join(process.cwd(), 'a.ts')).href;
    const symbols = Array.from({length: count}, (_, line) => ({name: msg.params.query + '_' + line, kind: 12,
      location: {uri, range: {start: {line, character: 0}, end: {line, character: 4}}}}));
    send({jsonrpc:'2.0', id:msg.id, result:symbols});"#,
        );
    std::fs::write(&tsgo, server).expect("fixture server");
    std::fs::set_permissions(
        &tsgo,
        std::fs::metadata(&script).expect("mode").permissions(),
    )
    .expect("chmod");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &tsgo, None));
        assert_eq!(client.workspace_symbol_limit(), Some(256));
        client.start().await.expect("server starts");
        for (name, capped) in [("Cap256", true), ("Cap255", false)] {
            let wire = json!({"operation":"workspaceSymbol", "path":path, "symbolQuery":name});
            let query: LspSearchQuery = serde_json::from_value(wire.clone()).expect("query");
            let row = symbol_row(&client, (&root, &paths), &query, &path, false).await;
            let reasons = if capped {
                json!(["workspaceSymbolCoverageUnknown"])
            } else {
                Value::Null
            };
            assert_eq!(row["partialReasons"], reasons, "{name}");
            let says_capped = row["warnings"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|warning| {
                    warning
                        .as_str()
                        .is_some_and(|text| text.contains("at most 256 workspace symbols"))
                });
            assert_eq!(says_capped, capped, "{name}: {row}");
            assert_row_contract(ToolId::LspSearch, &row);
        }
        client.stop().await.expect("fixture server closed");
    });
}
