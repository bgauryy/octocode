use super::remaining::REMAINING;
use super::{PageFacts, SpanFacts, note_remaining, redisclose, take_remaining_notes};
use crate::id::ToolId;
use crate::response::result::Remaining;
use serde_json::{Value, json};
mod facts;
mod remaining;

fn page(tool: &str) -> Value {
    json!({"tool": tool, "query": {"queries":[{"page": 2}]}})
}

/// The row as the engine notes it and the stage takes it: no warning is
/// added (the visible pagination and `next` carry the counts), and the
/// returned data leads with one `warnings` entry that words each note as
/// `<left> (next.<page>)`, so the counting rules stay asserted.
fn disclosed(data: Value) -> Value {
    let mut row = json!({"index": 0, "data": data});
    note_remaining(&mut row, ToolId::LocalSearch, Vec::new());
    let before = row["data"].clone();
    let mut structured = json!({"results": [row]});
    let notes = take_remaining_notes(&mut structured);
    let row = &structured["results"][0];
    assert!(row.get(REMAINING).is_none(), "{row}");
    assert_eq!(row["data"], before, "a whole row words no remaining note");
    let mut data = structured["results"][0]["data"].clone();
    if !notes[0].is_empty() {
        let entry = notes[0]
            .iter()
            .map(|note| format!("{} (next.{})", note.left, note.page))
            .collect::<Vec<_>>()
            .join("; ");
        let mut warnings = data["warnings"].as_array().cloned().unwrap_or_default();
        warnings.push(json!(entry));
        data["warnings"] = json!(warnings);
    }
    data
}
