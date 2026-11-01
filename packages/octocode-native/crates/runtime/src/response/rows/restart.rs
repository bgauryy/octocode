//! The one restart of a stale page: required work, on every tool.
use crate::id::ToolId;
use crate::response::result::{Continuation, fresh_query_row};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// A `staleSnapshot` row: its pages describe a source that changed, so the
/// walk starts again. `next.restart` is the row's own query without its
/// private progress, a required continuation; a related call restating it
/// is dropped, so the restart never reads as optional work.
pub(crate) fn require_restart(tool: ToolId, data: &mut Value, query: &Value) {
    if crate::id::error_code_of(data) != Some(ErrorCode::StaleSnapshot) || !query.is_object() {
        return;
    }
    let restart = Continuation::continued(tool, fresh_query_row(tool, query)).build();
    if let Some(related) = data.get_mut("related").and_then(Value::as_array_mut) {
        related.retain(|call| call["tool"] != restart["tool"] || call["input"] != restart["input"]);
        if related.is_empty()
            && let Some(fields) = data.as_object_mut()
        {
            fields.remove("related");
        }
    }
    if data.is_object() {
        data["next"]["restart"] = restart;
    }
}

#[cfg(test)]
mod tests;
