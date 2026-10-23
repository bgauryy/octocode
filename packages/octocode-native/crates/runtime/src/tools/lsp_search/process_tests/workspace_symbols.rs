//! Workspace-symbol provider coverage and the server result cap.

use super::*;

#[test]
fn workspace_symbol_provider_coverage_is_unknown_without_losing_semantic_rows() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::{Value, json};
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
        for (page_size, root_only) in [(2, false), (1000, false), (1000, true)] {
            let anchor = if root_only {
                root.to_string_lossy().into_owned()
            } else {
                path.clone()
            };
            let start = json!({
                "operation":"workspaceSymbol", "path":anchor, "symbolName":"Shared",
                "pageSize":page_size
            });
            let mut found = Vec::new();
            let mut page_count = 0;
            let mut walker = Walker::new(ToolId::LspSearch.as_str(), json!({"queries":[start]}));
            while let Some(call) = walker.next_query() {
                let query: crate::tools::lsp_search::LspSearchQuery =
                    serde_json::from_value(call["queries"][0].clone()).expect("query");
                let mut sources = SourceCache::new(&paths);
                let policy = snippet_policy(&paths);
                let scope = crate::tools::lsp_search::scope::Scope::new(
                    root.to_string_lossy().into_owned(),
                    vec!["*.ts".into()],
                );
                let row = Operation {
                    client: &client,
                    query: &query,
                    sources: &mut sources,
                    snippet_policy: &policy,
                    cancel: &NeverCancel,
                    path: &anchor,
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
                .expect("workspace symbols");
                let row = crate::tools::lsp_search::failure::with_next(&query, row);
                assert_eq!(row["isPartial"], true, "{row}");
                assert_eq!(
                    row["partialReasons"],
                    json!(["workspaceSymbolCoverageUnknown"])
                );
                assert!(
                    row.get("terminalLimit").is_none(),
                    "unknown is not a proven cap"
                );
                assert_eq!(row["pagination"]["totalItems"], 4);
                let published = crate::tools::test_support::published(
                    ToolId::LspSearch,
                    &lsp_search_query::to_row(&query),
                    &row,
                );
                assert!(
                    published
                        .pointer("/results/0/data/next/inspectDocumentSymbols")
                        .is_none(),
                    "optional lead is not pagination: {published}"
                );
                found.extend(row["matches"].as_array().expect("matches").iter().cloned());
                page_count += 1;
                if row.pointer("/next/nextPage").is_none() {
                    let hint = crate::tools::test_support::checks::hint(
                        &published["results"][0]["data"],
                        "inspectDocumentSymbols",
                    );
                    assert_eq!(
                        hint["query"]["queries"][0],
                        json!({"operation":"documentSymbols", "path":path})
                    );
                } else {
                    assert!(page_count < 3, "continuation must terminate");
                }
                walker.accept(Page {
                    output: published,
                    rows: match_rows(&row),
                });
            }
            walker.finish();
            assert_eq!(page_count, if page_size == 2 { 2 } else { 1 });
            assert_eq!(
                found
                    .iter()
                    .map(|v| v["symbolName"].as_str().expect("name"))
                    .collect::<Vec<_>>(),
                ["Shared", "Shared", "普通", "Alias"]
            );
            assert_eq!(found[0]["containerName"], "Module");
            assert_eq!(found[1]["containerName"], "Shadow");
            let identities = found
                .iter()
                .map(|v| (v["path"].clone(), v["displayRange"].clone()))
                .collect::<Vec<_>>();
            assert_eq!(
                identities
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                4
            );
            assert_eq!(
                found[2]["displayRange"],
                json!({"startLine":1,"startCharacter":4,"endLine":1})
            );
        }
        for (operation, name, partial) in [
            ("workspaceSymbol", Some("missing"), true),
            ("documentSymbols", None, false),
        ] {
            let mut wire = json!({"operation":operation,"path":path});
            if let Some(name) = name {
                wire["symbolName"] = json!(name);
            }
            let query: crate::tools::lsp_search::LspSearchQuery =
                serde_json::from_value(wire.clone()).expect("query");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let scope = crate::tools::lsp_search::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                vec!["*.ts".into()],
            );
            let row = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &scope,
                root_only: false,
                line: 0,
                character: 0,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await
            .expect("provider answer");
            let row = crate::tools::lsp_search::failure::with_next(&query, row);
            assert_eq!(
                row.get("isPartial")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                partial,
                "{row}"
            );
            assert_row_contract(ToolId::LspSearch, &wire, &row);
        }
        client.stop().await.expect("fixture server closed");
    });
}

/// tsgo answers `workspace/symbol` with at most 256 symbols; an answer of
/// exactly that many says the server capped it, one short of it does not.
/// The partial reason stays `workspaceSymbolCoverageUnknown`.
#[test]
fn workspace_symbols_at_the_server_limit_say_capped() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
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
            let wire = json!({"operation":"workspaceSymbol", "path":path, "symbolName":name});
            let query: crate::tools::lsp_search::LspSearchQuery =
                serde_json::from_value(wire.clone()).expect("query");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let scope = crate::tools::lsp_search::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                vec!["*.ts".into()],
            );
            let row = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &scope,
                root_only: false,
                line: 0,
                character: 0,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await
            .expect("workspace symbols");
            let row = crate::tools::lsp_search::failure::with_next(&query, row);
            assert_eq!(
                row["partialReasons"],
                json!(["workspaceSymbolCoverageUnknown"]),
                "{name}"
            );
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
            assert_row_contract(ToolId::LspSearch, &wire, &row);
        }
        client.stop().await.expect("fixture server closed");
    });
}
