//! Guidance follows the row's type, in one rule every tool's rows pass
//! before paging.
//!
//! An evidence row (it returns what was asked) carries its data, the `next`
//! calls that finish what was asked and the `warnings` that limit trust in
//! it: never `hints`, and no `related` of its own. What to call next from
//! an answer is the tool description's to say, and every identity such a
//! call needs is a field of the row. A `related` call stays on an evidence
//! row when its producer marked it as one the row needs
//! ([`Continuation::required`]: it reaches evidence the row asked for and
//! does not state), or when the tool's contract promises the link
//! ([`ToolId::promised_links`]). No sentence and no diagnostic code decides
//! it: a warning that names `related` keeps nothing.
//!
//! [`Continuation::required`]: super::result::Continuation::required
//!
//! An empty or failed row (`status`) carries its recovery: `hints` and
//! `related`, which its producer states.
use crate::id::ToolId;
use serde_json::Value;

/// Applies the rule to each result row. `request` holds the admitted
/// queries: a `related` call another row of the same response answers is
/// dropped first ([`super::answered_leads`]).
pub fn retain_guidance(tool: ToolId, structured: &mut Value, request: &Value) {
    super::answered_leads::drop_answered_leads(tool, structured, request);
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    for row in rows {
        let evidence = is_evidence(row);
        let Some(data) = row.get_mut("data").and_then(Value::as_object_mut) else {
            continue;
        };
        // The producer's mark is read here and never served.
        let Some(Value::Array(related)) = data.remove("related") else {
            if evidence {
                data.remove("hints");
            }
            continue;
        };
        let kept = related
            .into_iter()
            .filter_map(|mut call| {
                let required = call
                    .as_object_mut()
                    .and_then(|call| call.remove(super::result::REQUIRED_KEY))
                    == Some(Value::Bool(true));
                (required || !evidence || promised(tool, &call)).then_some(call)
            })
            .collect::<Vec<_>>();
        if !kept.is_empty() {
            data.insert("related".to_owned(), Value::Array(kept));
        }
        if evidence {
            data.remove("hints");
        }
    }
}

/// Removes the producers' marks ([`Continuation::required`]) and keeps
/// every call: a row as its producer built it, for a check that validates
/// it without the rule.
///
/// [`Continuation::required`]: super::result::Continuation::required
pub fn drop_marks(structured: &mut Value) {
    for row in structured
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        for call in row
            .pointer_mut("/data/related")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .filter_map(Value::as_object_mut)
        {
            call.remove(super::result::REQUIRED_KEY);
        }
    }
}

/// Whether `row` answers what was asked: the dispatch boundary states every
/// other outcome (`empty`, `error`, `outOfRange`) in `status`.
pub fn is_evidence(row: &Value) -> bool {
    row.get("status").is_none() && row["data"].get("errorCode").is_none()
}

/// Whether `tool`'s contract promises `call` on an answer: a cross-tool
/// link its description states.
fn promised(tool: ToolId, call: &Value) -> bool {
    call["tool"]
        .as_str()
        .is_some_and(|target| tool.promised_links().iter().any(|to| to.as_str() == target))
}

#[cfg(test)]
#[path = "workflow_lead_tests.rs"]
mod tests;
