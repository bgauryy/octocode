//! Resume queries: re-anchoring a capped node by name, line and occurrence.

use super::*;

fn node(uri: &str, line: u32, character: u32) -> Value {
    json!({
        "name": "foo", "uri": uri, "kind": 12,
        "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 20}},
        "selectionRange": {"start": {"line": line, "character": character}, "end": {"line": line, "character": character + 3}}
    })
}

/// The occurrence counts on the line the server numbered: a lone `\r`
/// breaks a line there as it does for the server, and every resume point
/// of one file reads it once.
#[test]
fn resume_points_count_occurrences_on_the_servers_lines() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("cr.ts");
    std::fs::write(&file, "let a = 1;\rfoo(); foo();\r").expect("source");
    let path = file.to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
    let query: LspSearchQuery = serde_json::from_value(json!({
        "operation": "callers", "path": path, "symbolName": "foo", "line": 2,
        "depth": 2, "importerPage": 2
    }))
    .expect("query");
    let mut sources = HashMap::new();
    let second = resume_query(
        &query,
        &Resume {
            node: node(&uri, 1, 7),
            depth: 1,
        },
        &mut sources,
    )
    .expect("resumable");
    assert_eq!(second["line"], 2, "{second}");
    assert_eq!(
        second["occurrence"], 1,
        "the second foo on that line: {second}"
    );
    assert!(second.get("importerPage").is_none(), "{second}");
    // A later resume point in the same file reuses the read: its source
    // is the one kept, not the file as it is now.
    std::fs::write(&file, "").expect("emptied");
    let again = resume_query(
        &query,
        &Resume {
            node: node(&uri, 1, 7),
            depth: 2,
        },
        &mut sources,
    )
    .expect("resumable");
    assert_eq!(again["occurrence"], 1, "{again}");
    assert_eq!(sources.len(), 1);
}
