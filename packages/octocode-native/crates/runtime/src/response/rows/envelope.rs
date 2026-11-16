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

/// The one statement that a row's text was redacted: its data gets
/// `redacted: true` when a tool marked it ([`crate::security::REDACTED_MARK`]
/// in its warnings, taken out here) or the output gate replaced text in it (it
/// holds more placeholders than `before` counted: a literal placeholder in a
/// source is no replacement). Error rows state only their failure.
pub(crate) fn warn_redacted_rows(envelope: &mut Value, before: &[usize]) {
    let after = placeholders_per_row(envelope);
    let Some(rows) = envelope["results"].as_array_mut() else {
        return;
    };
    for (index, row) in rows.iter_mut().enumerate() {
        let marked = take_marks(&mut row["data"]);
        let gate = after
            .get(index)
            .zip(before.get(index))
            .is_some_and(|(after, before)| after > before);
        if (marked || gate) && row["status"] != "error" {
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
mod tests {
    use super::*;
    use serde_json::json;

    /// A tool's mark, or text the gate replaced, flags the row; a source that
    /// only spells a placeholder does not, and an error row is never flagged.
    #[test]
    fn only_a_replacement_flags_a_row() {
        let mark = crate::security::redacted_mark();
        let mut envelope = json!({"results": [
            {"status": "hasResults", "data": {"content": "a", "warnings": [mark, "kept"]}},
            {"status": "hasResults", "data": {"content": "x = [REDACTED-FOO]"}},
            {"status": "hasResults", "data": {"content": "gate [REDACTED-GITHUBTOKENS]"}},
            {"status": "error", "data": {"warnings": [mark]}},
        ]});
        let before = vec![0, 1, 0, 0];
        // Rows 0, 1 and 3 hold the placeholders they started with; row 2's
        // text gained one at the gate.
        warn_redacted_rows(&mut envelope, &before);
        let rows = envelope["results"].as_array().expect("rows");
        assert_eq!(rows[0]["data"]["redacted"], json!(true));
        assert_eq!(rows[0]["data"]["warnings"], json!(["kept"]));
        assert!(rows[1]["data"].get("redacted").is_none(), "{}", rows[1]);
        assert_eq!(rows[2]["data"]["redacted"], json!(true));
        assert!(rows[3]["data"].get("redacted").is_none(), "{}", rows[3]);
    }
}
