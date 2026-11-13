//! Shared checks for CLI and MCP presentation invariants.
use serde_json::Value;

/// The statements of composed instructions that no tool text may repeat:
/// each line split at a sentence or clause end (`. `, `; `), list and heading
/// markers dropped, four words or more (shorter pieces are headings such as
/// `## 2. Loop`). Core's `instructionsOnce.test.ts` splits the same way.
pub(super) fn instruction_sentences(text: &str) -> Vec<&str> {
    text.lines()
        .flat_map(|line| line.split_inclusive(". "))
        .flat_map(|piece| piece.split_inclusive("; "))
        .map(|piece| {
            piece
                .trim_start_matches(|c: char| {
                    c.is_whitespace() || "#-.".contains(c) || c.is_ascii_digit()
                })
                .trim()
        })
        .filter(|piece| piece.split_whitespace().count() >= 4)
        .collect()
}

/// Every string in `value`, one per line: titles, descriptions, notes, examples.
pub(super) fn strings_of(value: &Value) -> String {
    fn walk(value: &Value, out: &mut String) {
        match value {
            Value::String(text) => {
                out.push_str(text);
                out.push('\n');
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            Value::Object(map) => map.values().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out = String::new();
    walk(value, &mut out);
    out
}
