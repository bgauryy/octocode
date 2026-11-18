use super::super::symbols::flatten_symbols;
use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn the_text_rule_reads_the_sites_own_line() {
    let text_call = |content: &str, line, character| {
        is_call_at(content, &LineIndex::new(content), line, character)
    };
    assert!(text_call("  return stage (1);", 0, 14));
    // UTF-16 column after a non-BMP character before the site.
    assert!(text_call("const 😀 = 1; needle(1)", 0, 20));
    assert!(!text_call("const 😀 = 1; needle(1)", 0, 19));
    assert!(!text_call("const { stage } = x;", 0, 13));
    // The rule reads the site's own line, never the next one.
    assert!(!text_call("stage\n(1);", 0, 5));
    assert!(text_call("a;\nstage(1);", 1, 5));
}

/// Issue #480: an implementing method's own name is followed by `(`, but
/// the outline names it a declaration; the call inside it stays.
#[test]
fn a_declaration_name_is_no_call_by_the_text_rule() {
    let content =
        "class Ledger {\n  read(at: number): string {\n    return this.port.read(at);\n  }\n}\n";
    let range = |line: u32, start: u32, end: u32| -> LspRange {
        serde_json::from_value(json!({
            "start": {"line": line, "character": start},
            "end": {"line": line, "character": end}
        }))
        .expect("range")
    };
    let outline = json_fixture(
        r#"[
                {
                    "name": "Ledger",
                    "kind": 5,
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 4, "character": 1}
                    },
                    "selectionRange": {
                        "start": {"line": 0, "character": 6},
                        "end": {"line": 0, "character": 12}
                    },
                    "children": [
                        {
                            "name": "read",
                            "kind": 6,
                            "range": {
                                "start": {"line": 1, "character": 2},
                                "end": {"line": 3, "character": 3}
                            },
                            "selectionRange": {
                                "start": {"line": 1, "character": 2},
                                "end": {"line": 1, "character": 6}
                            }
                        }
                    ]
                }
            ]"#,
    );
    let mut flat = Vec::new();
    flatten_symbols(&outline, &mut flat);
    let lines = LineIndex::new(content);
    let declaration = range(1, 2, 6);
    let call = range(2, 21, 25);
    assert!(
        is_call_at(content, &lines, 1, 6),
        "the bare text rule is fooled"
    );
    assert!(!is_text_call(content, &lines, &flat, &declaration));
    assert!(is_text_call(content, &lines, &flat, &call));
    // Without an outline (it failed) the text rule alone decides.
    assert!(is_text_call(content, &lines, &[], &declaration));
}
