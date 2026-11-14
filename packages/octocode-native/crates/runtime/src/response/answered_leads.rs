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
        if super::selection_errors::contract_rejection(row) {
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
        }
    }
}

#[cfg(test)]
#[path = "answered_leads_tests.rs"]
mod tests;
