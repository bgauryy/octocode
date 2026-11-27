//! The MCP instructions for a session's enabled tools, composed from the
//! presentation's `instructions` parts (core's `MCP_INSTRUCTION_PARTS`):
//! `head`, then the `text` of every route whose `needs` are all enabled, then
//! `body`, then the `text` of each block whose `tool` is enabled, in array
//! order, then the `text` of each settings entry whose `setting` holds its
//! `value`. Plain concatenation, so the result is core's own
//! `mcpInstructions`.

use serde_json::Value;

/// The instructions for the tools `enabled` accepts. A part the presentation
/// lacks contributes nothing; the contract tests rule that out.
pub fn compose_instructions(presentation: &Value, enabled: impl Fn(&str) -> bool) -> String {
    let parts = &presentation["instructions"];
    let text = |part: &Value| part["text"].as_str().unwrap_or_default().to_owned();
    let listed = |name: &str| parts[name].as_array().into_iter().flatten();
    let mut out = parts["head"].as_str().unwrap_or_default().to_owned();
    for route in listed("routes") {
        let needs = route["needs"].as_array().into_iter().flatten();
        if needs.filter_map(Value::as_str).all(&enabled) {
            out.push_str(&text(route));
        }
    }
    out.push_str(parts["body"].as_str().unwrap_or_default());
    for block in listed("blocks") {
        if block["tool"].as_str().is_some_and(&enabled) {
            out.push_str(&text(block));
        }
    }
    out
}

/// The settings blocks for a session: the `text` of each `settings` part
/// whose `setting` (a config path) `value_of` resolves to its `value`, in
/// array order. Appended to [`compose_instructions`] by an interface whose
/// replies the setting changes.
pub fn setting_instructions(
    presentation: &Value,
    value_of: impl Fn(&str) -> Option<Value>,
) -> String {
    presentation["instructions"]["settings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["setting"]
                .as_str()
                .and_then(&value_of)
                .is_some_and(|value| value == entry["value"])
        })
        .filter_map(|entry| entry["text"].as_str())
        .collect()
}

#[cfg(test)]
mod tests;
