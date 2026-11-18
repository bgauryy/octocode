//! Callers named at their declaration when the server's item selects a call
//! site.

use super::*;

/// jdtls selects a caller item at its first call site, not at its name: the
/// row would give the call line as the caller's line (main at 5, declared at
/// 4). The caller is named at the declaration its file's outline gives, and
/// so is the `via` of every edge below it; other callers stay as the server
/// named them.
#[test]
fn a_caller_selected_at_its_call_site_is_named_at_its_declaration() {
    let at = |line: u64, start: u64, end: u64| serde_json::json!({"start": {"line": line, "character": start}, "end": {"line": line, "character": end}});
    let span = |from: u64, to: u64| serde_json::json!({"start": {"line": from, "character": 4}, "end": {"line": to, "character": 5}});
    let uri = "file:///w/src/main/java/demo/App.java";
    // `main` (lines 3-5, name at 3:23) calls `four` at 4:27.
    let main = serde_json::json!({"name": "main(String[]) : void", "kind": 6, "detail": "demo.App",
        "uri": uri, "range": span(3, 5), "selectionRange": at(4, 27, 33)});
    // `four` (lines 6-8, name at 6:15) calls `two` at 7:15 and 7:23.
    let four = serde_json::json!({"name": "four() : int", "kind": 6, "uri": uri,
        "range": span(6, 8), "selectionRange": at(7, 15, 18)});
    // A caller its server names at its name stays.
    let named = serde_json::json!({"name": "other", "kind": 6, "uri": uri,
        "range": span(10, 12), "selectionRange": at(10, 15, 20)});
    let edges = vec![
        HierarchyEdge {
            node: four.clone(),
            parent: None,
            level: 1,
            sites: vec![at(7, 15, 18), at(7, 23, 26)],
        },
        HierarchyEdge {
            node: main.clone(),
            parent: Some(four.clone()),
            level: 2,
            sites: vec![at(4, 27, 33)],
        },
        HierarchyEdge {
            node: named.clone(),
            parent: None,
            level: 1,
            sites: vec![at(11, 8, 11)],
        },
    ];
    let outline = serde_json::json!([{"name": "App", "kind": 5, "range": span(2, 13), "selectionRange": at(2, 13, 16),
    "children": [
        {"name": "main(String[])", "kind": 6, "range": span(3, 5), "selectionRange": at(3, 23, 27)},
        {"name": "four()", "kind": 6, "range": span(6, 8), "selectionRange": at(6, 15, 19)},
        {"name": "other()", "kind": 6, "range": span(10, 12), "selectionRange": at(10, 15, 20)}
    ]}]);
    let outlines =
        std::collections::HashMap::from([(uri.to_owned(), std::sync::Arc::new(outline))]);
    let renamed = name_callers(edges, &outlines);
    assert_eq!(renamed[0].node["selectionRange"], at(6, 15, 19));
    assert_eq!(
        renamed[0].node["name"], "four() : int",
        "the server's item is kept"
    );
    assert_eq!(renamed[1].node["selectionRange"], at(3, 23, 27));
    assert_eq!(
        renamed[1]
            .parent
            .as_ref()
            .map(|parent| &parent["selectionRange"]),
        Some(&at(6, 15, 19)),
        "the via names the renamed parent"
    );
    assert_eq!(renamed[2].node, named);
    let public = public_edge(Expansion::IncomingCalls, &renamed[1], None);
    assert_eq!(public["from"]["line"], 4, "{public}");
    assert_eq!(public["via"]["line"], 7, "{public}");
    // Without an outline the server's items stay.
    let kept = name_callers(renamed, &std::collections::HashMap::new());
    assert_eq!(kept[1].node["selectionRange"], at(3, 23, 27));
}
