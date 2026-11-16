//! A row echoes the operation the query asked for, whatever payload shape
//! carries its answer.
use super::*;
use serde_json::json;

#[test]
fn rows_echo_the_requested_operation() {
    let symbol = json!([{"name":"run","kind":12,"location":{"uri":"file:///repo/a.ts","range":{"start":{"line":0,"character":0},"end":{"line":0,"character":3}}}}]);
    for (input, kind, operation) in [
        (
            json!({"operation":"workspaceSymbol","workspaceRoot":"/repo","symbolFuzzy":"run"}),
            "symbols",
            "workspaceSymbol",
        ),
        (
            json!({"operation":"diagnostic","path":"/repo/a.ts"}),
            "diagnostics",
            "diagnostic",
        ),
        (
            json!({"operation":"documentSymbols","path":"/repo/a.ts"}),
            "documentSymbols",
            "documentSymbols",
        ),
    ] {
        let q = query(input);
        for items in [symbol.clone(), json!([])] {
            let row = items_payload(&q, kind, items.clone(), TEST_BUDGET);
            if let Some(echo) = row.get("operation") {
                assert_eq!(echo, operation, "{row}");
            }
            assert!(
                kind == operation || !row.to_string().contains(&format!("{kind} returned")),
                "the reason names the requested operation: {row}"
            );
        }
    }
}
