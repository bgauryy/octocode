//! A row's `related[]` never re-asks what another row of the same response
//! answers: the stage drops such a call before the workflow-lead rule
//! ([`super::workflow_lead`]) chooses what a complete row keeps.
use crate::id::ToolId;
use crate::response::result::fresh_query_row;
use serde_json::Value;

/// Drops from each row's `related[]` every call this response already
/// answers: one to this request's own tool whose every query is another
/// query of this request (as the same semantic request,
/// [`fresh_query_row`]), so the row beside it holds that answer. A call that
/// repeats the row's own query is its retry and stays. `response_query`
/// holds the admitted queries, in the order of the rows that are not
/// contract rejections.
pub(crate) fn drop_answered_leads(tool: ToolId, structured: &mut Value, response_query: &Value) {
    let queries = response_query
        .get("queries")
        .and_then(Value::as_array)
        .map(|queries| {
            queries
                .iter()
                .map(|query| fresh_query_row(tool, query))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if queries.len() < 2 {
        return;
    }
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    let mut own = 0..queries.len();
    for row in rows {
        if contract_rejection(row) {
            continue;
        }
        let Some(own) = own.next() else {
            break;
        };
        let answered = |call: &Value| {
            call["tool"] == tool.as_str()
                && call["input"]["queries"]
                    .as_array()
                    .filter(|sent| !sent.is_empty())
                    .is_some_and(|sent| {
                        sent.iter().all(|query| {
                            let query = fresh_query_row(tool, query);
                            query != queries[own]
                                && queries
                                    .iter()
                                    .enumerate()
                                    .any(|(index, asked)| index != own && *asked == query)
                        })
                    })
        };
        let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
            continue;
        };
        let Some(Value::Array(related)) = data.get_mut("related") else {
            continue;
        };
        related.retain(|call| !answered(call));
        if related.is_empty() {
            data.remove("related");
            strip_dangling_related_hints(data);
        }
    }
}

/// Removes dangling `related[]` clauses from `data["hints"]` after the row's
/// last `related[]` call has been dropped: factual prefixes stay, pure
/// `related[]` hints are removed, and an empty `hints` array is dropped.
fn strip_dangling_related_hints(data: &mut serde_json::Map<String, Value>) {
    let Some(Value::Array(hints)) = data.get_mut("hints") else {
        return;
    };
    let mut kept = Vec::with_capacity(hints.len());
    for hint in hints.drain(..) {
        match hint {
            Value::String(text) => {
                if let Some(cleaned) = strip_related_reference(&text) {
                    kept.push(Value::String(cleaned));
                }
            }
            other => kept.push(other),
        }
    }
    if kept.is_empty() {
        data.remove("hints");
    } else {
        *hints = kept;
    }
}

/// Strips every clause or sentence mentioning `related[]` from `hint`, keeping
/// any factual text before or after it. Returns `None` when nothing remains.
fn strip_related_reference(hint: &str) -> Option<String> {
    let mut text = hint.to_owned();
    while let Some((prefix, suffix)) = text.split_once("related[]") {
        let before = [". ", "; ", ": "]
            .into_iter()
            .filter_map(|sep| prefix.rsplit_once(sep).map(|(head, _)| head))
            .max_by_key(|head| head.len())
            .map_or("", str::trim_end);
        let after = suffix
            .split_once(". ")
            .map_or("", |(_, tail)| tail.trim_start());
        let mut combined = String::with_capacity(before.len() + after.len() + 2);
        if !before.is_empty() {
            combined.push_str(before);
            if !before.ends_with(['.', '!', '?']) {
                combined.push('.');
            }
        }
        if !after.is_empty() {
            if !combined.is_empty() {
                combined.push(' ');
            }
            combined.push_str(after);
        }
        text = combined;
    }
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
#[path = "answered_leads_tests.rs"]
mod tests;

/// Isolated contract failures are not admitted and therefore have no entry in
/// `response_query`. Adapter-level invalid-input failures carry a repair call
/// and remain paired with their admitted query.
fn contract_rejection(row: &Value) -> bool {
    row.get("status") == Some(&Value::String("error".into()))
        && crate::id::error_code_of(&row["data"])
            == Some(octocode_contract_types::ErrorCode::InvalidInput)
        && row.pointer("/data/next").is_none()
}
