//! Stable indexed result rows, with explicit coverage and guidance.
mod envelope;
mod recovery;
mod restart;
pub use envelope::envelope;
pub(crate) use envelope::{placeholders_per_row, warn_redacted_rows};
pub(crate) use recovery::{
    CANCELLED_HINT, GITHUB_AUTH_RECOVERY_HINT, LARGE_SOURCE_HINT, attach_diagnostics,
    attach_error_hint, error_code_hint, note_error_hint,
};
pub(crate) use restart::require_restart;
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct RowParts {
    pub(crate) diagnostics: crate::response::result::ToolDiagnostics,
}

pub(crate) fn finish_row(
    index: usize,
    data: Value,
    status: Option<&str>,
    parts: RowParts,
) -> Value {
    let mut row = result_row(index, data, status);
    attach_diagnostics(&mut row, parts.diagnostics);
    row
}

pub(crate) fn result_row(index: usize, mut data: Value, status: Option<&str>) -> Value {
    if let Some(status) = status
        && data.get("status").and_then(Value::as_str) == Some(status)
        && let Some(data) = data.as_object_mut()
    {
        data.remove("status");
    }
    lead_with_error_identity(&mut data);
    let mut row = json!({"index":index,"data":data});
    if let Some(status) = status {
        row["status"] = json!(status);
    }
    row
}

/// Keys an error row states first, in this order: the shape of every error
/// row, whichever tool or layer built it.
const ERROR_LEAD: [&str; 2] = ["errorCode", "error"];

/// Reorders an error row's `data` so [`ERROR_LEAD`] come first and the rest
/// keep their order.
fn lead_with_error_identity(data: &mut Value) {
    let Some(map) = data.as_object_mut() else {
        return;
    };
    if !map.contains_key(ERROR_LEAD[0]) {
        return;
    }
    let mut ordered = serde_json::Map::with_capacity(map.len());
    for key in ERROR_LEAD {
        if let Some(value) = map.remove(key) {
            ordered.insert(key.to_owned(), value);
        }
    }
    ordered.extend(std::mem::take(map));
    *map = ordered;
}

/// Row `errorCode`s that reject the caller's input (CLI exit 2): the request
/// is wrong, so broadening or retrying it cannot help.
pub fn is_invalid_input_code(code: &str) -> bool {
    crate::id::error_class(code) == crate::id::error_codes::ErrorClass::InvalidInput
}

#[cfg(test)]
mod tests;
