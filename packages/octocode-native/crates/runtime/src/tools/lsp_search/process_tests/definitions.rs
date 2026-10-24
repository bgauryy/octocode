//! Definition answers from a real provider: partial follow-ups and locations
//! outside the read roots.

use super::*;

/// A real provider answers the first definition with an alias location, then
/// fails the follow-up. Useful locations survive, but identity stays partial.
#[test]
fn nested_definition_failure_keeps_location_with_partial_provenance() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    for fail in [true, false] {
        let (_dir, root, script) = fixture();
        std::fs::write(root.join("b.ts"), "const b = 1;\n").expect("alias source");
        let server = FAKE_SERVER
            .replace("hoverProvider: true", "definitionProvider: true")
            .replace(
                "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
                &format!(r#"  }} else if (msg.method === 'textDocument/definition') {{
    if (msg.params.textDocument.uri.endsWith('/a.ts')) {{
      send({{jsonrpc:'2.0',id:msg.id,result:{{uri:msg.params.textDocument.uri.replace('/a.ts','/b.ts'),range:{{start:{{line:0,character:6}},end:{{line:0,character:7}}}}}}}});
    }} else if ({fail}) {{
      send({{jsonrpc:'2.0',id:msg.id,error:{{code:-32603,message:'injected follow-up failure'}}}});
    }} else {{
      send({{jsonrpc:'2.0',id:msg.id,result:[]}});
    }}"#),
            );
        std::fs::write(&script, server).expect("definition fixture server");
        let paths = crate::tools::test_support::workspace_policy(&root);
        let path = root.join("a.ts").to_string_lossy().into_owned();
        let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
        let query = serde_json::from_value(json!({
            "operation":"definition", "path":uri, "symbolName":"x", "line":1
        }))
        .expect("query");
        tokio::runtime::Runtime::new().expect("rt").block_on(async {
            let client = NativeLspClient::new(config(&root, &script, None));
            client.start().await.expect("server starts");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let row = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &crate::tools::lsp_search::scope::Scope::new(
                    root.to_string_lossy().into_owned(),
                    Vec::new(),
                ),
                root_only: false,
                line: 0,
                character: 6,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await
            .expect("definition row retains earlier evidence");
            assert!(lists_path(&row, "b.ts"), "{row}");
            if fail {
                assert_eq!(row["isPartial"], true, "{row}");
                assert!(
                    row["partialReasons"]
                        .as_array()
                        .expect("reasons")
                        .contains(&json!("definitionHopFailed")),
                    "{row}"
                );
                // The server's failure reaches the caller in a warning.
                assert!(
                    row["warnings"].as_array().is_some_and(|warnings| warnings
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .any(|warning| warning.contains("injected follow-up failure"))),
                    "{row}"
                );
                assert_eq!(row["next"]["retry"]["tool"], "lspSearch");
            } else {
                assert!(
                    !row["partialReasons"]
                        .as_array()
                        .is_some_and(|reasons| reasons.contains(&json!("definitionHopFailed"))),
                    "{row}"
                );
                assert_absent(&row, "injected follow-up failure");
            }
            client.stop().await.expect("fixture server closed");
        });
    }
}

/// A definition the server places outside the allowed read roots is
/// withheld, and the row says so instead of "no locations".
#[test]
fn a_definition_outside_the_read_roots_is_reported_withheld() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::{SourceCache, snippet_policy};
    use serde_json::json;
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("dep.ts");
    std::fs::write(&target, "export const a = 1;\n").expect("target");
    let target_uri =
        octocode_engine::lsp::uri::path_to_uri(&target.to_string_lossy()).expect("uri");
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "definitionProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            &format!(
                "  }} else if (msg.method === 'textDocument/definition') {{\n    send({{ jsonrpc: '2.0', id: msg.id, result: [{{ uri: '{target_uri}', range: {{ start: {{ line: 0, character: 13 }}, end: {{ line: 0, character: 14 }} }} }}] }});"
            ),
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(async {
            let client = NativeLspClient::new(config(&root, &script, None));
            client.start().await.expect("server starts");
            let query = serde_json::from_value(json!({
                "operation": "definition", "path": path, "symbolName": "a", "line": 1
            }))
            .expect("query");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let outcome = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &crate::tools::lsp_search::scope::Scope::new(
                    root.to_string_lossy().into_owned(),
                    vec!["*.ts".into()],
                ),
                root_only: false,
                line: 0,
                character: 6,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await;
            client.stop().await.expect("fixture server closed");
            outcome
        })
        .unwrap_or_else(|error| panic!("the server answers: {error:?}"));
    assert_eq!(row["category"], "locationsWithheld", "{row}");
    assert!(
        row["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("1 location")),
        "{row}"
    );
}
