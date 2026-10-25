//! Original batch row positions and placement of rejected rows on page one.

use crate::response::outcome::ToolOutcome;
use crate::response::result::Remaining;
use serde_json::{Value, json};

/// Original indices of the admitted rows: every input position not rejected.
pub(crate) fn admitted_indices(admitted: usize, rejected: &[usize]) -> Vec<usize> {
    (0..admitted + rejected.len())
        .filter(|position| !rejected.contains(position))
        .take(admitted)
        .collect()
}

/// Saved tool work continues the originating query, including fan-out reads.
pub(crate) fn inherit(structured: &mut Value, origin: Option<usize>) {
    if let Some(index) = origin {
        for row in structured
            .get_mut("results")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            row["index"] = json!(index);
        }
    }
}

/// Take the rejected rows out of a settled envelope, leaving the body a page
/// continuation reproduces; returns the remaining-notes of the rows it keeps
/// and the rejected rows themselves. Positions refer to the settled array,
/// since fan-out continuations can share an originating query index.
pub(crate) fn split_rejected(
    structured: &mut Value,
    remaining: Vec<Vec<Remaining>>,
    rejected: &[usize],
) -> (Vec<Vec<Remaining>>, Vec<Value>) {
    let Some(rows) = structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
    else {
        return (remaining, Vec::new());
    };
    let mut notes = remaining.into_iter();
    let (mut kept, mut kept_notes, mut taken) = (Vec::new(), Vec::new(), Vec::new());
    for (position, row) in rows.into_iter().enumerate() {
        let note = notes.next().unwrap_or_default();
        if rejected.contains(&position) {
            taken.push(row);
        } else {
            kept.push(row);
            kept_notes.push(note);
        }
    }
    structured["results"] = Value::Array(kept);
    (kept_notes, taken)
}

/// Show the rejected rows on the first page, in `results` at their indices.
pub(crate) fn attach_rejected(outcome: &mut ToolOutcome, rejected: Vec<Value>) {
    if rejected.is_empty() {
        return;
    }
    outcome.structured_content.edit(|structured| {
        if !structured.get("results").is_some_and(Value::is_array) {
            structured["results"] = json!([]);
        }
        if let Some(rows) = structured["results"].as_array_mut() {
            rows.extend(rejected);
            rows.sort_by_key(|row| row["index"].as_u64());
        }
    });
}
