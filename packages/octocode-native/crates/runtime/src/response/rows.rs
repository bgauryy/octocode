//! Stable indexed result rows, with explicit coverage and guidance.
mod envelope;
mod recovery;
use crate::id::ToolId;
pub use envelope::envelope;
pub(crate) use recovery::{
    CANCELLED_HINT, GITHUB_AUTH_RECOVERY_HINT, attach_diagnostics, attach_error_hint,
    error_code_hint,
};
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct RowParts {
    pub(crate) diagnostics: crate::response::result::ToolDiagnostics,
    pub(crate) remaining: Vec<super::result::Remaining>,
}

pub(crate) fn finish_row(
    tool: ToolId,
    index: usize,
    data: Value,
    status: Option<&str>,
    parts: RowParts,
) -> Value {
    let mut row = result_row(index, data, status);
    attach_diagnostics(&mut row, parts.diagnostics);
    super::pages::note_remaining(&mut row, tool, parts.remaining);
    row
}

pub(crate) fn result_row(index: usize, mut data: Value, status: Option<&str>) -> Value {
    if let Some(status) = status
        && data.get("status").and_then(Value::as_str) == Some(status)
        && let Some(data) = data.as_object_mut()
    {
        data.remove("status");
    }
    let mut row = json!({"index":index,"data":data});
    if let Some(status) = status {
        row["status"] = json!(status);
    }
    row
}

/// Row `errorCode`s that reject the caller's input (CLI exit 2): the request
/// is wrong, so broadening or retrying it cannot help.
pub fn is_invalid_input_code(code: &str) -> bool {
    crate::id::error_class(code) == crate::id::error_codes::ErrorClass::InvalidInput
}

#[cfg(test)]
mod tests;
