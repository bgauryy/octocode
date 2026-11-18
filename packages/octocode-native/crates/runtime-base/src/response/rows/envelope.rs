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
pub fn placeholders_per_row(envelope: &Value) -> Vec<usize> {
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

/// The one statement that a row's text was redacted: its data gets
/// `redacted: true` when a tool marked it ([`crate::security::REDACTED_MARK`]
/// in its warnings, taken out here) and the text the row serves still shows a
/// placeholder (a page cut can leave the replaced text on another page), or
/// when the output gate replaced text in it (it holds more placeholders than
/// `before` counted: a literal placeholder in a source is no replacement).
/// An error row is flagged when its text holds a placeholder (a tool's
/// mark alone never flags it), and drops any lead that holds a placeholder.
pub fn warn_redacted_rows(envelope: &mut Value, before: &[usize]) {
    let after = placeholders_per_row(envelope);
    let Some(rows) = envelope["results"].as_array_mut() else {
        return;
    };
    for (index, row) in rows.iter_mut().enumerate() {
        let marked = take_marks(&mut row["data"]);
        let shows = after.get(index).is_some_and(|after| *after > 0);
        let gate = after
            .get(index)
            .zip(before.get(index))
            .is_some_and(|(after, before)| after > before);
        let error = row["status"] == "error";
        // An error row's own message counts too: its text repeats the
        // caller's value, and a placeholder there is a replaced value.
        let replaced_echo = error && crate::security::placeholders_in(&row["data"]) > 0;
        if (marked && shows && !error) || gate || replaced_echo {
            // An error row that echoes a caller's credential-shaped value
            // (a path) shows it replaced; a lead built from the raw value
            // would hold the placeholder and can match nothing.
            if error
                && let Some(leads) = row["data"].get_mut("related").and_then(Value::as_array_mut)
            {
                leads.retain(|lead| {
                    !lead
                        .to_string()
                        .contains(octocode_engine::security::REDACTION_PREFIX)
                });
                if leads.is_empty()
                    && let Some(fields) = row["data"].as_object_mut()
                {
                    fields.remove("related");
                }
            }
            crate::security::mark_redacted(&mut row["data"]);
        }
    }
}

/// Takes the tools' [`crate::security::REDACTED_MARK`] out of `data`'s
/// warnings; whether there was one.
fn take_marks(data: &mut Value) -> bool {
    let Some(warnings) = data.get_mut("warnings").and_then(Value::as_array_mut) else {
        return false;
    };
    let had = warnings.len();
    warnings.retain(|warning| warning.as_str() != Some(crate::security::REDACTED_MARK));
    let marked = warnings.len() != had;
    if warnings.is_empty()
        && let Some(fields) = data.as_object_mut()
    {
        fields.remove("warnings");
    }
    marked
}

#[cfg(test)]
mod tests;
