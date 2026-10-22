//! Original batch row positions and placement of rejected rows on page one.

use crate::id::ToolId;
use crate::response::outcome::ToolOutcome;
use crate::response::pages::Remaining;
use serde_json::{Value, json};

/// Original indices of the admitted rows: every input position not rejected.
pub(crate) fn admitted_indices(admitted: usize, rejected: &[usize]) -> Vec<usize> {
    (0..admitted + rejected.len())
        .filter(|position| !rejected.contains(position))
        .take(admitted)
        .collect()
}

/// Each input position's admitted query, `None` at a rejected position.
pub(crate) fn by_position<'a>(queries: &'a [Value], rejected: &[usize]) -> Vec<Option<&'a Value>> {
    let mut admitted = queries.iter();
    (0..queries.len() + rejected.len())
        .map(|position| {
            if rejected.contains(&position) {
                None
            } else {
                admitted.next()
            }
        })
        .collect()
}

/// A settled envelope without its rejected rows (the body a page
/// continuation reproduces), the remaining-notes of the rows it keeps, and
/// the rejected rows themselves.
pub(crate) fn split_rejected(
    mut structured: Value,
    remaining: Vec<Vec<Remaining>>,
    rejected: &[usize],
) -> (Value, Vec<Vec<Remaining>>, Vec<Value>) {
    let is_rejected = |row: &Value| {
        row["index"]
            .as_u64()
            .and_then(|index| usize::try_from(index).ok())
            .is_some_and(|index| rejected.contains(&index))
    };
    let Some(rows) = structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
    else {
        return (structured, remaining, Vec::new());
    };
    let mut notes = remaining.into_iter();
    let (mut kept, mut kept_notes, mut taken) = (Vec::new(), Vec::new(), Vec::new());
    for row in rows {
        let note = notes.next().unwrap_or_default();
        if is_rejected(&row) {
            taken.push(row);
        } else {
            kept.push(row);
            kept_notes.push(note);
        }
    }
    structured["results"] = Value::Array(kept);
    (structured, kept_notes, taken)
}

/// Whether `outcome` is one page of a longer response.
pub(crate) fn is_paged(outcome: &ToolOutcome) -> bool {
    outcome.structured_content["responsePagination"]["hasMore"] == true
}

/// Show the rejected rows on the first page: in `results` at their indices,
/// and before the page's text.
pub(crate) fn attach_rejected(
    outcome: &mut ToolOutcome,
    rejected: Vec<Value>,
    tool: ToolId,
    query: &Value,
    format: crate::response::render::TextFormat,
) {
    if rejected.is_empty() {
        return;
    }
    let structured = &mut outcome.structured_content;
    let whole_rows = structured.get("responseWindow").is_none();
    let rendered = crate::response::render::render_tool(
        tool,
        &json!({"results": rejected.clone()}),
        query,
        format,
    );
    if !structured.get("results").is_some_and(Value::is_array) {
        structured["results"] = json!([]);
    }
    if let Some(rows) = structured["results"].as_array_mut() {
        rows.extend(rejected);
        rows.sort_by_key(|row| row["index"].as_u64());
    }
    let Some(content) = outcome.content.first_mut() else {
        return;
    };
    // Row pages carry the envelope itself as text; a text window gets the
    // rejected rows' rendering ahead of its window.
    if whole_rows && content.text.starts_with('{') {
        content.text = outcome.structured_content.to_string();
    } else {
        content.text = format!("{rendered}\n{}", content.text);
    }
}
