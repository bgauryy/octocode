//! Long source lines retain their evidence through executable reads.

use super::*;
use crate::test_support::huge_line;

/// A contextLines window whose anchor sits on a later line than the first
/// context line starts there: its row says so (the window, which the
/// numbering and the line judging read) and numbers its text from that line.
#[tokio::test(flavor = "current_thread")]
async fn a_window_on_a_later_line_is_numbered_from_that_line() {
    let (_dir, root, paths) = temp_workspace();
    let file = root.join("bundle.js");
    let (line, column) = huge_line();
    std::fs::write(&file, format!("target(1);\n// before-line\n{line}\n")).expect("file");
    let q = query(serde_json::json!({
        "operation": "definition", "path": file.to_string_lossy(), "symbolName": "target",
        "line": 1, "contextLines": {"before": 1, "after": 0}
    }));
    let mut sources = SourceCache::new(&paths);
    let row = super::super::super::locations::locations(
        &q,
        &mut sources,
        "definition",
        "definitionProvider",
        vec![serde_json::json!({
            "uri": format!("file://{}", file.to_string_lossy()),
            "range": {"start": {"line": 2, "character": column + 9}, "end": {"line": 2, "character": column + 15}},
            "value": line,
            "displayRange": {"startLine": 3, "endLine": 3, "startCharacter": column + 1}
        })],
        20_000,
        &octocode_runtime_base::security::ContentSecurity::new(),
    )
    .await;
    let rows = location_rows(&row);
    let location = rows.first().expect("one location");
    let content = location["value"].as_str().expect("value");
    assert!(content.contains("before the window on line 3"), "{content}");
    let numbered = content
        .split('\n')
        .filter(|record| !record.starts_with('…'))
        .collect::<Vec<_>>();
    assert_eq!(numbered.len(), 1, "{content}");
    assert!(numbered[0].starts_with("3\ttarget(a)"), "{content}");
    assert_eq!(location["line"], 3, "{location}");
    // A one-line range states no end: `endLine` is absent.
    assert!(location.get("endLine").is_none(), "{location}");
}
