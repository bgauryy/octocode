//! When a page states `oversized: true`: it holds one value longer than the
//! page target alone that no page can split. Free text is windowed across
//! row parts (`rowPart.text`), or kept whole only when it fits a page of its
//! own row, so it never makes a page oversized (a reader's byte window, at
//! least the documented floor, is served whole and is free text too); a value the output schema
//! bounds, one every part repeats whole (an identity) and an executable call
//! (it runs only whole) cannot be split. A page wider only by its fixed
//! parts (the envelope, the `responsePagination` block, the context each
//! row part repeats) states nothing: `responseChars` is a target, and the
//! docs name those fixed parts once (`docs/TOOL_DATA_CONTRACT.md`, Paging).

use crate::contracts::output_fields::{executable_call, free_text_at, page_context_fields};
use crate::id::ToolId;
use serde_json::{Map, Value, json};

/// The envelope key of the flag. One flag for any page, single or walked;
/// never `false`.
const OVERSIZED: &str = "oversized";

/// Set `page`'s flag when it holds a value of `tool` longer than `budget`
/// that no page can split.
pub(super) fn flag(page: &mut Map<String, Value>, tool: ToolId, budget: usize) {
    if holds_value_over(page, tool, budget) {
        page.insert(OVERSIZED.into(), json!(true));
    }
}

/// Re-judge `page`'s `oversized` flag after a step that only removed content
/// from the cut page (the echo drop): the flag states the page delivered,
/// so one that no longer holds such a value states none.
pub(crate) fn unflag_fitting(page: &mut Value, tool: ToolId, budget: usize) {
    let Some(envelope) = page.as_object_mut() else {
        return;
    };
    if envelope.shift_remove(OVERSIZED).is_some() {
        flag(envelope, tool, budget);
    }
}

/// Whether `page` holds a value of `tool` longer than `budget` alone that no
/// page can split: a string the output schema bounds or every part repeats,
/// or an executable call, outside the `responsePagination` block.
pub fn holds_value_over(page: &Map<String, Value>, tool: ToolId, budget: usize) -> bool {
    page.iter().any(|(key, value)| match key.as_str() {
        "responsePagination" | OVERSIZED => false,
        "results" => value
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| value_over(row, tool, budget, "")),
        _ => value_over(value, tool, budget, &format!("/{key}")),
    })
}

/// Whether `value`, at row pointer `at`, is or holds an indivisible value
/// longer than `budget`.
fn value_over(value: &Value, tool: ToolId, budget: usize, at: &str) -> bool {
    match value {
        Value::String(text) => {
            crate::response::cut::escaped_len(text) > budget
                && !(free_text_at(tool, at) && !page_context_fields(tool).repeats_whole(at))
        }
        Value::Object(map) if executable_call(map) => {
            octocode_engine::json_size::utf16_units(value).is_some_and(|chars| chars > budget)
        }
        Value::Object(map) => map.iter().any(|(key, item)| {
            let child = format!("{at}/{}", crate::json_pointer::escape(key));
            value_over(item, tool, budget, &child)
        }),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .any(|(index, item)| value_over(item, tool, budget, &format!("{at}/{index}"))),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("an envelope")
    }

    /// Short rows and a long cursor: a page over its target by fixed parts.
    #[test]
    fn fixed_parts_never_flag_a_page() {
        let long = "x".repeat(300);
        let rows = page(json!({
            "results": [{"index":0,"data":{"files":[{"path":"a","matches":[{"line":1,"value":"short"}]}]}}],
            "responsePagination": {"hasMore":true,"next":{"kind":"continue","tool":"localSearch","input":{"cursor":long}}}
        }));
        assert!(!holds_value_over(&rows, ToolId::LocalSearch, 100));
    }

    /// Free text a page windows, kept whole or cut, is no indivisible value.
    #[test]
    fn free_text_is_split_on_purpose() {
        let long = "y".repeat(300);
        let hit = page(json!({"results": [{"index":0,
            "data": {"files":[{"path":"a","matches":[{"line":2,"value":long}]}]}}]}));
        assert!(!holds_value_over(&hit, ToolId::LocalSearch, 100));
        let read = page(json!({"results": [{"index":0,"data":{"content":long}}]}));
        assert!(!holds_value_over(&read, ToolId::LocalFetch, 100));
    }

    /// An identity every part repeats, and an executable call, run whole.
    #[test]
    fn identities_and_calls_are_indivisible() {
        let long = "z".repeat(300);
        let path = page(json!({"results": [{"index":0,
            "data": {"files":[{"path":long,"matches":[{"line":2,"value":"v"}]}]}}]}));
        assert!(holds_value_over(&path, ToolId::LocalSearch, 100));
        assert!(
            !holds_value_over(&path, ToolId::LocalSearch, 300),
            "at the target it fits"
        );
        let call = page(json!({"results": [{"index":0,"data":{
            "related":[{"kind":"related","tool":"localFetch","input":{"queries":[{"path":long}]}}]
        }}]}));
        assert!(holds_value_over(&call, ToolId::LocalSearch, 100));
    }

    #[test]
    fn unflagging_rejudges_what_the_page_still_holds() {
        let long = "p".repeat(50);
        let mut over =
            json!({"results": [{"index":0,"data":{"files":[{"path":long}]}}], "oversized": true});
        unflag_fitting(&mut over, ToolId::LocalSearch, 10);
        assert_eq!(over["oversized"], true);
        unflag_fitting(&mut over, ToolId::LocalSearch, 100);
        assert!(over.get("oversized").is_none());
    }
}
