//! A documentSymbols outline: declarations and their members, and on request
//! the symbols inside function and method bodies.
use super::items_payload;
use octocode_contract_types::LspSearchQuery;
use serde_json::{Value, json};

fn query(locals: bool) -> LspSearchQuery {
    serde_json::from_value(json!({
        "operation": "documentSymbols", "path": "src/a.ts", "locals": locals
    }))
    .expect("query")
}

fn range(start: u64, end: u64) -> Value {
    json!({"start": {"line": start, "character": 0}, "end": {"line": end, "character": 1}})
}

fn symbol(name: &str, kind: u64, line: u64, children: Value) -> Value {
    json!({"name": name, "kind": kind, "range": range(line, line + 1),
        "selectionRange": range(line, line), "children": children})
}

/// An interface with its member, and a function whose body holds a local
/// constant and an object literal with one property (3 body symbols).
fn tree() -> Value {
    json!([
        symbol("Options", 11, 0, json!([symbol("home", 7, 1, json!([]))])),
        symbol(
            "run",
            12,
            5,
            json!([
                symbol("envLocal", 14, 6, json!([])),
                symbol(
                    "result",
                    13,
                    7,
                    json!([symbol("resultKey", 7, 7, json!([]))])
                ),
            ])
        ),
    ])
}

#[test]
fn an_outline_lists_declarations_and_members_and_counts_body_symbols() {
    let outline = items_payload(&query(false), "documentSymbols", tree(), 100_000);
    let rows = outline["symbols"].to_string();
    for kept in ["Options", "home", "run"] {
        assert!(rows.contains(kept), "{kept}: {outline}");
    }
    for left in ["envLocal", "result", "resultKey"] {
        assert!(!rows.contains(left), "{left}: {outline}");
    }
    let hint = outline["warnings"][0]
        .as_str()
        .expect("a warning names what was left out");
    assert!(
        hint.contains('3') && hint.contains("locals: true"),
        "{hint}"
    );

    let whole = items_payload(&query(true), "documentSymbols", tree(), 100_000);
    let rows = whole["symbols"].to_string();
    for listed in ["envLocal", "result", "resultKey"] {
        assert!(rows.contains(listed), "{listed}: {whole}");
    }
    assert!(whole.get("warnings").is_none(), "{whole}");
}
