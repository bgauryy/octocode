//! Answers from the editor: level `ide`, shaped and paged like a server's.
use super::{Bridge, Editor, item, location, result};
use serde_json::{Value, json};

fn assert_ide(row: &Value) {
    assert_eq!(row["lsp"]["level"], "ide", "{row}");
    assert_eq!(row["lsp"]["ide"]["name"], "Visual Studio Code", "{row}");
    assert_eq!(row["lsp"]["serverAvailable"], true, "{row}");
    assert!(row["lsp"]["fellBackFrom"].is_null(), "{row}");
}

/// Definition and references come from the bridge with no server spawned;
/// every request carried the right `Host` and bearer token.
#[tokio::test(flavor = "current_thread")]
async fn definition_and_references_answer_at_level_ide() {
    let editor = Editor::new();
    let uri = editor.uri();
    let bridge = Bridge::serve(Box::new(move |body| match body["method"].as_str() {
        Some("definition") => result(json!([location(&uri, 0, 16, 22)])),
        Some("references") => result(json!([
            location(&uri, 0, 16, 22),
            location(&uri, 1, 29, 35)
        ])),
        _ => (404, json!({"error": "unknown", "reason": "method"})),
    }));
    editor.open(&bridge);
    let definition = editor
        .ask(json!({"operation": "definition", "symbolName": "target", "line": 1}))
        .await;
    assert_ide(&definition);
    assert!(definition.to_string().contains("target"), "{definition}");
    let references = editor
        .ask(json!({"operation": "references", "symbolName": "target", "line": 1}))
        .await;
    assert_ide(&references);
    assert!(references.to_string().contains("target();"), "{references}");
    let asked = &bridge.asked("definition")[0];
    assert_eq!(asked["path"], editor.file.to_string_lossy().as_ref());
    assert_eq!(
        (asked["line"].as_u64(), asked["character"].as_u64()),
        (Some(0), Some(16))
    );
    bridge.assert_authentic();
}

/// An outline cut into tiny pages reaches every symbol exactly once. The
/// outline lists symbols in source order, so each one sits on its own line.
#[tokio::test(flavor = "current_thread")]
async fn document_symbols_page_every_row_once() {
    let editor = Editor::new();
    let symbols = (0..40)
        .map(|index| {
            let range = json!({"start": {"line": index, "character": 0},
                "end": {"line": index, "character": 20}});
            json!({"name": format!("symbol_{index}"), "kind": 12,
                "range": range, "selectionRange": range})
        })
        .collect::<Vec<_>>();
    let bridge = Bridge::serve(Box::new(move |_| result(json!(symbols))));
    editor.open(&bridge);
    let query = json!({"operation": "documentSymbols", "path": editor.file.to_string_lossy()});
    let pages = editor.walk(query, Some(600)).await;
    assert!(pages.len() > 1, "{pages:?}");
    let mut names = Vec::new();
    for page in &pages {
        assert_ide(page);
        for symbol in page["symbols"].as_array().into_iter().flatten() {
            names.push(symbol["symbolName"].as_str().unwrap_or_default().to_owned());
        }
    }
    let expected = (0..40)
        .map(|index| format!("symbol_{index}"))
        .collect::<Vec<_>>();
    assert_eq!(names, expected);
    bridge.assert_authentic();
}

/// `callers` at depth 2 walks incoming calls one level at a time, handing
/// each item back to the bridge exactly as it was returned.
#[tokio::test(flavor = "current_thread")]
async fn callers_walk_two_levels() {
    let editor = Editor::new();
    let uri = editor.uri();
    let bridge = Bridge::serve(Box::new(move |body| match body["method"].as_str() {
        Some("prepareCallHierarchy") => result(json!([item(&uri, "target", 0)])),
        Some("incomingCalls") => {
            let from = match body["item"]["name"].as_str() {
                Some("target") => vec![item(&uri, "caller", 1)],
                Some("caller") => vec![item(&uri, "top", 2)],
                _ => vec![],
            };
            result(json!(from
                .into_iter()
                .map(|from| json!({"from": from, "fromRanges": [location(&uri, 1, 29, 35)["range"].clone()]}))
                .collect::<Vec<_>>()))
        }
        _ => (404, json!({"error": "unknown", "reason": "method"})),
    }));
    editor.open(&bridge);
    let row = editor
        .ask(json!({"operation": "callers", "symbolName": "target", "line": 1, "depth": 2}))
        .await;
    assert_ide(&row);
    let text = row.to_string();
    assert!(text.contains("caller") && text.contains("top"), "{row}");
    let asked = bridge.asked("incomingCalls");
    assert_eq!(asked.len(), 2, "{:?}", bridge.methods());
    assert_eq!(asked[0]["item"]["data"], "handle-target");
    bridge.assert_authentic();
}

/// Hover, diagnostics and workspace symbols are shaped as a server's rows.
#[tokio::test(flavor = "current_thread")]
async fn hover_diagnostics_and_workspace_symbols_answer_at_level_ide() {
    let editor = Editor::new();
    let uri = editor.uri();
    let range = location(&uri, 0, 16, 22)["range"].clone();
    let bridge = Bridge::serve(Box::new(move |body| match body["method"].as_str() {
        Some("hover") => {
            result(json!({"contents": "```ts\nfunction target(): void\n```", "range": range}))
        }
        Some("diagnostics") => result(json!([
            {"range": range, "severity": 2, "message": "target is unused", "source": "ts"}
        ])),
        Some("workspaceSymbol") => result(json!([
            {"name": "target", "kind": 12, "location": location(&uri, 0, 16, 22)}
        ])),
        _ => (404, json!({"error": "unknown", "reason": "method"})),
    }));
    editor.open(&bridge);
    let hover = editor
        .ask(json!({"operation": "hover", "symbolName": "target", "line": 1}))
        .await;
    assert_ide(&hover);
    assert!(
        hover.to_string().contains("function target(): void"),
        "{hover}"
    );
    let diagnostics = editor.ask(json!({"operation": "diagnostic"})).await;
    assert_ide(&diagnostics);
    let text = diagnostics.to_string();
    assert!(
        text.contains("target is unused") && text.contains("warning"),
        "{text}"
    );
    let symbols = editor
        .ask(json!({"operation": "workspaceSymbol", "symbolQuery": "target"}))
        .await;
    assert_ide(&symbols);
    assert!(symbols.to_string().contains("target"), "{symbols}");
    assert_eq!(bridge.asked("workspaceSymbol")[0]["query"], "target");
    bridge.assert_authentic();
}

/// A handle the editor forgot (404) is prepared again once, at the item's
/// own position, and the expansion is asked again with the fresh item.
#[tokio::test(flavor = "current_thread")]
async fn a_stale_handle_is_prepared_again_once() {
    let editor = Editor::new();
    let uri = editor.uri();
    let prepared = std::sync::atomic::AtomicUsize::new(0);
    let bridge = Bridge::serve(Box::new(move |body| match body["method"].as_str() {
        Some("prepareCallHierarchy") => {
            let mut root = item(&uri, "target", 0);
            // The first handle is already stale; the one prepared again is fresh.
            let first = prepared.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0;
            root["data"] = json!(if first { "stale" } else { "fresh" });
            result(json!([root]))
        }
        Some("incomingCalls") if body["item"]["data"] == "fresh" => result(json!([
            {"from": item(&uri, "caller", 1), "fromRanges": [location(&uri, 1, 29, 35)["range"].clone()]}
        ])),
        Some("incomingCalls") => (404, json!({"error": "not found", "reason": "stale handle"})),
        _ => (404, json!({"error": "unknown", "reason": "method"})),
    }));
    editor.open(&bridge);
    let row = editor
        .ask(json!({"operation": "callers", "symbolName": "target", "line": 1}))
        .await;
    assert_ide(&row);
    assert!(row.to_string().contains("caller"), "{row}");
    assert_eq!(
        bridge.asked("prepareCallHierarchy").len(),
        2,
        "{:?}",
        bridge.methods()
    );
    bridge.assert_authentic();
}

/// A workspace-root search names no document: the bridge gets the query and
/// no `path`, so the editor opens nothing.
#[tokio::test(flavor = "current_thread")]
async fn a_workspace_root_search_sends_no_path() {
    let editor = Editor::new();
    let uri = editor.uri();
    let bridge = Bridge::serve(Box::new(move |_| {
        result(json!([{
            "name": "target", "kind": 12, "location": location(&uri, 0, 16, 22)
        }]))
    }));
    editor.open(&bridge);
    let pages = editor
        .walk(
            json!({
                "operation": "workspaceSymbol", "symbolQuery": "targ",
                "workspaceRoot": editor.root.to_string_lossy()
            }),
            None,
        )
        .await;
    assert_ide(&pages[0]);
    assert!(pages[0].to_string().contains("target"), "{}", pages[0]);
    let asked = bridge.asked("workspaceSymbol");
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(asked[0].get("path").is_none(), "{}", asked[0]);
    assert_eq!(asked[0]["query"], "targ");
}

/// A workspace search anchored at a file still tells the editor which one.
#[tokio::test(flavor = "current_thread")]
async fn a_workspace_search_from_a_file_sends_its_path() {
    let editor = Editor::new();
    let bridge = Bridge::serve(Box::new(|_| result(json!([]))));
    editor.open(&bridge);
    editor
        .ask(json!({"operation": "workspaceSymbol", "symbolQuery": "targ"}))
        .await;
    let asked = bridge.asked("workspaceSymbol");
    assert_eq!(asked[0]["path"], editor.file.to_string_lossy().as_ref());
}
