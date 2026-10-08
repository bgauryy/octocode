//! Stable row indices across response pages of a batch with rejected rows.
//!
//! A row rejected at admission cannot ride `responsePagination.next`: the
//! continuation is a complete input that the contract validates, so it carries
//! only the admitted rows. Every page still shows each row at its original
//! input `index`:
//!
//! - the paged body holds the admitted rows only, so a continuation that
//!   re-executes them reproduces the body (and its snapshot) exactly;
//! - the rejected rows ride the first page beside that body;
//! - the snapshot the continuation carries names each admitted row's original
//!   index (`<snapshot>@0,2`), so the re-executed rows keep them.

use crate::runtime::ToolOutcome;
use crate::tools::id::ToolId;
use serde_json::{Value, json};

const MARK: char = '@';

/// Split `<snapshot>@<index>,…` into the pager's snapshot and the original
/// indices of the continuation's `rows` rows. A malformed or mismatched mark
/// is left in place, so the snapshot mismatches and the pager restarts.
pub(crate) fn split_snapshot(snapshot: &str, rows: usize) -> Option<(String, Vec<usize>)> {
    let (base, mark) = snapshot.rsplit_once(MARK)?;
    let indices = mark
        .split(',')
        .map(|index| index.parse::<usize>().ok())
        .collect::<Option<Vec<_>>>()?;
    let increasing = indices.windows(2).all(|pair| pair[0] < pair[1]);
    (indices.len() == rows && increasing).then(|| (base.to_owned(), indices))
}

/// Original indices of the admitted rows: every input position not rejected.
pub(crate) fn admitted_indices(admitted: usize, rejected: &[usize]) -> Vec<usize> {
    (0..admitted + rejected.len())
        .filter(|position| !rejected.contains(position))
        .take(admitted)
        .collect()
}

/// The mark a snapshot carries when the admitted rows are not `0..n`.
pub(crate) fn mark(indices: &[usize]) -> Option<String> {
    let shifted = indices
        .iter()
        .enumerate()
        .any(|(position, index)| position != *index);
    shifted.then(|| {
        let list = indices
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        format!("{MARK}{list}")
    })
}

/// Whether `outcome` is one page of a longer response.
pub(crate) fn is_paged(outcome: &ToolOutcome) -> bool {
    outcome.structured_content["responsePagination"]["hasMore"] == true
}

/// `structured` without the rows at `rejected` indices.
pub(crate) fn admitted_body(structured: &Value, rejected: &[usize]) -> Value {
    let mut body = structured.clone();
    if let Some(rows) = body.get_mut("results").and_then(Value::as_array_mut) {
        rows.retain(|row| {
            row["index"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .is_none_or(|index| !rejected.contains(&index))
        });
    }
    body
}

/// Append `mark` to the page's snapshot and to its continuation's.
pub(crate) fn mark_snapshots(outcome: &mut ToolOutcome, mark: &str) {
    let Some(pagination) = outcome
        .structured_content
        .get_mut("responsePagination")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if let Some(Value::String(snapshot)) = pagination.get_mut("snapshot") {
        snapshot.push_str(mark);
    }
    if let Some(Value::String(snapshot)) = pagination
        .get_mut("next")
        .and_then(|next| next.get_mut("query"))
        .and_then(|query| query.get_mut("responseSnapshot"))
    {
        snapshot.push_str(mark);
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_round_trip_and_reject_mismatches() {
        let indices = admitted_indices(2, &[1]);
        assert_eq!(indices, [0, 2]);
        let marked = mark(&indices).expect("shifted rows are marked");
        assert_eq!(marked, "@0,2");
        let snapshot = format!("response-v1:abc{marked}");
        assert_eq!(
            split_snapshot(&snapshot, 2),
            Some(("response-v1:abc".to_owned(), vec![0, 2]))
        );
        assert_eq!(split_snapshot(&snapshot, 3), None, "row count must match");
        assert_eq!(split_snapshot("response-v1:abc@2,0", 2), None);
        assert_eq!(split_snapshot("response-v1:abc", 1), None);
        assert_eq!(mark(&admitted_indices(3, &[])), None, "0..n needs no mark");
    }
}
