use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn enclosing_callables_of_an_outline() {
    let tree = json_fixture(
        r#"[
                {
                    "name": "run",
                    "kind": 12,
                    "range": {
                        "start": {"line": 1, "character": 0},
                        "end": {"line": 3, "character": 1}
                    },
                    "selectionRange": {
                        "start": {"line": 1, "character": 9},
                        "end": {"line": 1, "character": 12}
                    },
                    "children": [
                        {
                            "name": "inner",
                            "kind": 12,
                            "range": {
                                "start": {"line": 2, "character": 2},
                                "end": {"line": 2, "character": 30}
                            }
                        }
                    ]
                }
            ]"#,
    );
    let mut flat = Vec::new();
    flatten_symbols(&tree, &mut flat);
    assert_eq!(
        enclosing_callable(&flat, 2, 10).map(|s| s["name"].clone()),
        Some(json!("inner"))
    );
    assert_eq!(
        enclosing_callable(&flat, 3, 0).map(|s| s["name"].clone()),
        Some(json!("run"))
    );
    assert!(enclosing_callable(&flat, 9, 0).is_none());
}

fn range(line: u32, start: u32, end: u32) -> LspRange {
    serde_json::from_value(json!({
        "start": {"line": line, "character": start},
        "end": {"line": line, "character": end}
    }))
    .expect("range")
}

/// A flat `SymbolInformation` list has no `selectionRange`: the selection
/// is the whole `location.range`, which names only a symbol whose server
/// gave just the name there.
#[test]
fn a_flat_symbol_information_list_selects_its_location_range() {
    let outline = json_fixture(
        r#"[
                {
                    "name": "read",
                    "kind": 6,
                    "location": {
                        "uri": "file:///a.ts",
                        "range": {
                            "start": {"line": 1, "character": 2},
                            "end": {"line": 3, "character": 3}
                        }
                    }
                }
            ]"#,
    );
    let mut flat = Vec::new();
    flatten_symbols(&outline, &mut flat);
    assert_eq!(flat[0]["selectionRange"], flat[0]["range"]);
    assert!(!is_declaration_name(&flat, &range(1, 2, 6)));
    let named = json_fixture(
        r#"[
                {
                    "name": "read",
                    "kind": 6,
                    "location": {
                        "uri": "file:///a.ts",
                        "range": {
                            "start": {"line": 1, "character": 2},
                            "end": {"line": 1, "character": 6}
                        }
                    }
                }
            ]"#,
    );
    let mut flat = Vec::new();
    flatten_symbols(&named, &mut flat);
    assert!(is_declaration_name(&flat, &range(1, 2, 6)));
    assert!(!is_declaration_name(&flat, &range(2, 21, 25)));
}
