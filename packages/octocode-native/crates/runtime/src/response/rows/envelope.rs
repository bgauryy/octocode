use crate::id::ToolId;
use serde_json::{Value, json};

/// Every indexed result owns its paths and evidence.
pub fn envelope(_tool: ToolId, rows: Vec<Value>) -> Value {
    json!({"results":rows})
}

/// Redaction placeholders in each row's `data` (string leaves and keys)
/// outside the row's own messages, in row order: what the output gate's
/// count is compared against. A message only repeats a value hidden in the
/// row's text ([`crate::contracts::envelope::message_paths`]), so a
/// placeholder there says nothing the row's text does not.
pub(crate) fn placeholders_per_row(envelope: &Value) -> Vec<usize> {
    let messages = crate::contracts::envelope::message_paths(envelope);
    envelope["results"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .enumerate()
                .map(|(index, row)| {
                    let index = index.to_string();
                    let said: usize = messages
                        .iter()
                        .filter(|path| {
                            matches!(path.as_slice(), [rows, at, data, ..] if rows == "results" && *at == index && data == "data")
                        })
                        .filter_map(|path| envelope.pointer(&crate::json_pointer::of(path))?.as_str())
                        .map(crate::security::placeholders)
                        .sum();
                    crate::security::placeholders_in(&row["data"]).saturating_sub(said)
                })
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
                .any(crate::security::states_a_redaction)
        });
        if !stated {
            crate::response::paging::push_warnings(
                data,
                [crate::security::redacted_content_warning(after - before)],
            );
        }
    }
}
