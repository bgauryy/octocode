//! What the output gate reads of a response envelope's shape: its executable
//! calls, the runtime state they carry, and the runtime's own messages.

use crate::id::ToolId;
use serde_json::Value;
use std::{collections::BTreeSet, sync::OnceLock};

/// The key a static notice rides until the response stage places it.
pub const NOTICES: &str = "notices";

/// The key a measured wait rides until the response stage states it.
pub(crate) const TIMED_NOTICES: &str = "timedNotices";

/// Explicit executable calls are never inferred from a source identity object.
#[must_use]
pub fn is_call(value: &Value) -> bool {
    matches!(
        value.get("kind").and_then(Value::as_str),
        Some("continue" | "related")
    ) && value.get("tool").is_some_and(Value::is_string)
        && value.get("input").is_some_and(Value::is_object)
}

/// A call as a response serves it: its `tool` and `input`, without the
/// `kind` the runtime drops once the response is final. Only a position the
/// output schema declares a call holds one.
#[must_use]
pub fn is_served_call(value: &Value) -> bool {
    value.get("kind").is_none()
        && value.get("tool").is_some_and(Value::is_string)
        && value.get("input").is_some_and(Value::is_object)
}

/// `tool`'s handle fields: the properties of each input form that carries no
/// `queries` (the continuation handle, `{"cursor": …}`), which only the
/// runtime issues.
fn handle_fields(tool: ToolId) -> &'static BTreeSet<String> {
    static FIELDS: [OnceLock<BTreeSet<String>>; ToolId::ALL.len()] =
        [const { OnceLock::new() }; ToolId::ALL.len()];
    FIELDS[tool.index()].get_or_init(|| {
        let Ok(contract) = super::tool_contract(tool) else {
            return BTreeSet::new();
        };
        let schema = &contract["inputSchema"];
        let defs = &schema["$defs"];
        schema["anyOf"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|form| super::resolve_ref(form, defs))
            .filter(|form| super::property_schemas(form, defs, "queries").is_empty())
            .flat_map(|form| super::property_names(form, defs))
            .map(str::to_owned)
            .collect()
    })
}

/// The runtime's own state in every executable call of `value`, by path, in
/// document order: a handle input (`{"cursor": …}`, [`handle_fields`]), and
/// in each query row of `input.queries` the fields the target tool's query
/// schema marks `x-cursor` ([`super::state_fields`]: snapshot digests,
/// offsets, pages, selection and plan tokens). The runtime writes them: the
/// input gate refuses them on a fresh request, and no file or provider text
/// fills them, so the output gate never judges them. A call's other leaves
/// (a path, a `matchString` a lead took from a hit) stay the gate's.
pub(crate) fn state_paths(value: &Value) -> Vec<Vec<String>> {
    let mut paths = Vec::new();
    state_paths_in(value, &mut Vec::new(), &mut paths);
    paths
}

fn state_paths_in(value: &Value, path: &mut Vec<String>, out: &mut Vec<Vec<String>>) {
    match value {
        Value::Object(map) => {
            if is_call(value)
                && let Some(tool) = map["tool"].as_str().and_then(ToolId::from_name)
            {
                let at = |tail: &[&str]| {
                    let mut at = path.clone();
                    at.push("input".into());
                    at.extend(tail.iter().map(|key| (*key).to_owned()));
                    at
                };
                let input = map["input"].as_object().into_iter().flatten();
                let handle = handle_fields(tool);
                if input.clone().all(|(field, _)| handle.contains(field)) {
                    out.extend(input.map(|(field, _)| at(&[field])));
                    return;
                }
                let state = super::state_fields(tool);
                for (index, row) in map["input"]["queries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    let index = index.to_string();
                    for field in row.as_object().into_iter().flat_map(|row| row.keys()) {
                        if state.contains(field) {
                            out.push(at(&["queries", &index, field]));
                        }
                    }
                }
                return;
            }
            for (key, child) in map {
                path.push(key.clone());
                state_paths_in(child, path, out);
                path.pop();
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                path.push(index.to_string());
                state_paths_in(item, path, out);
                path.pop();
            }
        }
        _ => {}
    }
}

/// The runtime's own messages in a response envelope, by path: each string
/// of the envelope's `warnings` and `hints`, of each row's `warnings`,
/// `hints` and the notices the stage has yet to place ([`NOTICES`],
/// [`TIMED_NOTICES`] texts), and of its `meta.diagnostics.hints`. Tool code
/// and the hint registry author them, so the output gate never judges them
/// as text of the row's file: only the pathless net, which still hides a
/// credential a message quotes from a provider or a file, and the values
/// the gate hid in a leaf about the same source.
pub(crate) fn message_paths(envelope: &Value) -> Vec<Vec<String>> {
    const WARNINGS: &str = "warnings";
    const HINTS: &str = "hints";
    let mut paths = Vec::new();
    let mut strings = |value: &Value, at: &[&str], tail: Option<&str>| {
        for (index, item) in value.as_array().into_iter().flatten().enumerate() {
            let text = tail.map_or(item, |key| &item[key]);
            if text.is_string() {
                let mut path: Vec<String> = at.iter().map(|key| (*key).to_owned()).collect();
                path.push(index.to_string());
                path.extend(tail.map(str::to_owned));
                paths.push(path);
            }
        }
    };
    for key in [WARNINGS, HINTS] {
        strings(&envelope[key], &[key], None);
    }
    for (index, row) in envelope["results"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let index = index.to_string();
        for key in [WARNINGS, HINTS, NOTICES] {
            strings(&row["data"][key], &["results", &index, "data", key], None);
        }
        strings(
            &row["data"][TIMED_NOTICES],
            &["results", &index, "data", TIMED_NOTICES],
            Some("text"),
        );
        strings(
            &row["meta"]["diagnostics"][HINTS],
            &["results", &index, "meta", "diagnostics", HINTS],
            None,
        );
    }
    paths
}
