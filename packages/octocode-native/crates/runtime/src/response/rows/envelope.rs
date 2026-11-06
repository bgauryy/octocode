use crate::id::ToolId;
use serde_json::{Value, json};

/// Every indexed result owns its paths and evidence.
pub fn envelope(_tool: ToolId, rows: Vec<Value>) -> Value {
    json!({"results":rows})
}

/// Redaction placeholders in each row's `data` (string leaves and keys), in
/// row order: what the output gate's count is compared against.
pub(crate) fn placeholders_per_row(envelope: &Value) -> Vec<usize> {
    envelope["results"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| crate::security::placeholders_in(&row["data"]))
                .collect()
        })
        .unwrap_or_default()
}

/// A row the output gate redacted (it holds more placeholders than `before`
/// counted) says its text is not verbatim, with the shared warning
/// ([`crate::security::redacted_content_warning`]), unless its tool already
/// said so; error rows state only their failure.
pub(crate) fn warn_redacted_rows(envelope: &mut Value, before: &[usize]) {
    let after = placeholders_per_row(envelope);
    let Some(rows) = envelope["results"].as_array_mut() else {
        return;
    };
    for ((row, before), after) in rows.iter_mut().zip(before).zip(after) {
        if after <= *before || row["status"] == "error" {
            continue;
        }
        let data = &mut row["data"];
        let stated = data["warnings"].as_array().is_some_and(|warnings| {
            warnings
                .iter()
                .filter_map(Value::as_str)
                .any(|warning| warning.starts_with("redacted"))
        });
        if !stated {
            crate::response::paging::push_warnings(
                data,
                [crate::security::redacted_content_warning(after - before)],
            );
        }
    }
}
