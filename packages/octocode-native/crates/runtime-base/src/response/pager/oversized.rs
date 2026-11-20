//! When a page states `oversized: true`: it holds one value longer than the
//! page target alone that the response cannot split. Paged responses window
//! free text across row parts (`rowPart.text`), so free text does not make
//! them oversized. Unpaged responses keep all strings whole. Schema-bounded
//! values, repeated identities, executable calls and the members of a declared
//! collection that declares nothing below them are always indivisible.
//! A page wider only by its fixed
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
/// that this response cannot split. Only a paged response can window free text.
pub(super) fn flag(page: &mut Map<String, Value>, tool: ToolId, budget: usize, paging: bool) {
    if holds_value_over_with_mode(page, tool, budget, paging) {
        page.insert(OVERSIZED.into(), json!(true));
    }
}

/// Re-judge `page`'s `oversized` flag after a step that only removed content
/// from the cut page (the echo drop): the flag states the page delivered,
/// so one that no longer holds such a value states none.
pub fn unflag_fitting(page: &mut Value, tool: ToolId, budget: usize, paging: bool) {
    let Some(envelope) = page.as_object_mut() else {
        return;
    };
    if envelope.shift_remove(OVERSIZED).is_some() {
        flag(envelope, tool, budget, paging);
    }
}

/// Whether `page` holds a value of `tool` longer than `budget` alone that no
/// page can split: a string the output schema bounds or every part repeats,
/// or an executable call, outside the `responsePagination` block and per-call
/// row metadata.
pub fn holds_value_over(page: &Map<String, Value>, tool: ToolId, budget: usize) -> bool {
    holds_value_over_with_mode(page, tool, budget, true)
}

fn holds_value_over_with_mode(
    page: &Map<String, Value>,
    tool: ToolId,
    budget: usize,
    paging: bool,
) -> bool {
    page.iter().any(|(key, value)| match key.as_str() {
        "responsePagination" | OVERSIZED => false,
        "results" => value
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| value_over(row, tool, budget, "", paging)),
        _ => value_over(value, tool, budget, &format!("/{key}"), paging),
    })
}

/// Whether `value`, at row pointer `at`, is or holds an indivisible value
/// longer than `budget`.
fn value_over(value: &Value, tool: ToolId, budget: usize, at: &str, paging: bool) -> bool {
    match value {
        Value::String(text) => {
            crate::response::cut::escaped_len(text) > budget
                && (!paging
                    || !free_text_at(tool, at)
                    || page_context_fields(tool).repeats_whole(at))
        }
        Value::Object(map) if executable_call(map) => {
            octocode_engine::json_size::utf16_units(value).is_some_and(|chars| chars > budget)
        }
        Value::Object(_) | Value::Array(_)
            if page_context_fields(tool).collection_routes.is_whole_member(at)
                && octocode_engine::json_size::utf16_units(&without_free_text(
                    value, tool, at, paging,
                ))
                .is_some_and(|chars| chars > budget) =>
        {
            true
        }
        Value::Object(map) => map.iter().any(|(key, item)| {
            if at == "/data" && super::telemetry::is_volatile_data_field(key) {
                return false;
            }
            let child = format!("{at}/{}", crate::json_pointer::escape(key));
            value_over(item, tool, budget, &child, paging)
        }),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .any(|(index, item)| value_over(item, tool, budget, &format!("{at}/{index}"), paging)),
        _ => false,
    }
}

/// `value` at row pointer `at` with the free text a paged response windows
/// emptied: what the value weighs however that text is cut.
fn without_free_text(value: &Value, tool: ToolId, at: &str, paging: bool) -> Value {
    match value {
        Value::String(_) if paging && free_text_at(tool, at) => Value::String(String::new()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| {
                    let child = format!("{at}/{}", crate::json_pointer::escape(key));
                    (key.clone(), without_free_text(item, tool, &child, paging))
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| without_free_text(item, tool, &format!("{at}/{index}"), paging))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests;
