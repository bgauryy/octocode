//! Coverage receipts and the one inspection notice for classified rows.
//!
//! Source acquisition can fail before a reader knows how many candidates its
//! selected view contains. The caller still needs an explicit unavailable
//! receipt, but there is no omitted body to recover and therefore no synthetic
//! unfiltered retry is added here.

use crate::id::ToolId;
use octocode_contract_types::ErrorCode;
use octocode_contract_types::{Selection, SelectionScope, SelectionStatus, SelectionUnit};
use octocode_engine::num::saturating_i64;
use serde_json::Value;

/// Add the `unavailable` receipt a classified row that failed lacks.
///
/// `response_query` contains only admitted rows. Contract-rejected siblings
/// are skipped while pairing those rows with the indexed result envelope.
pub(crate) fn decorate(tool: ToolId, structured: &mut Value, response_query: &Value) {
    let Some((scope, unit)) = tool.selection_receipt() else {
        return;
    };
    let queries: Vec<&Value> = match response_query.get("queries").and_then(Value::as_array) {
        Some(queries) => queries.iter().collect(),
        None => vec![response_query],
    };
    let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut) else {
        return;
    };
    let mut queries = queries.into_iter();
    for row in rows {
        if contract_rejection(row) {
            continue;
        }
        let Some(query) = queries.next() else {
            break;
        };
        // A refused request ran no selection: its repair, not unresolved
        // evidence, is the way on.
        if crate::id::error_code_of(&row["data"]) == Some(ErrorCode::InvalidInput) {
            continue;
        }
        if query.get("classification").is_some()
            && row.get("status") == Some(&Value::String("error".into()))
            && let Some(data) = row.get_mut("data").and_then(Value::as_object_mut)
        {
            data.entry("selection").or_insert_with(|| {
                serde_json::to_value(unavailable(scope, unit, 0, "sourceUnavailable"))
                    .unwrap_or(Value::Null)
            });
        }
    }
}

/// Isolated contract failures are not admitted and therefore have no entry in
/// `response_query`. Adapter-level invalid-input failures carry a repair call
/// and remain paired with their admitted query.
pub(super) fn contract_rejection(row: &Value) -> bool {
    row.get("status") == Some(&Value::String("error".into()))
        && crate::id::error_code_of(&row["data"]) == Some(ErrorCode::InvalidInput)
        && row.pointer("/data/next").is_none()
}

/// The receipt of a view nothing judged: all `total` items are unknown.
pub fn unavailable(
    scope: SelectionScope,
    unit: SelectionUnit,
    total: usize,
    reason: &str,
) -> Selection {
    let total = saturating_i64(total);
    Selection {
        status: SelectionStatus::Unavailable,
        scope,
        unit,
        total_items: total,
        selected_items: 0,
        excluded_items: 0,
        related_items: None,
        unknown_items: total,
        reason: Some(reason.into()),
        failures: Vec::new(),
        usage: None,
    }
}

#[cfg(test)]
mod tests;
