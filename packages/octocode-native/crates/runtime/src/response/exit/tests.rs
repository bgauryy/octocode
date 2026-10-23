use super::*;
use crate::runtime::RuntimeError;
use serde_json::json;
mod outcomes;
mod rows;
mod spelling;

fn all_rows_invalid_input(value: &Value) -> bool {
    value["results"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(is_invalid_input_row))
}

fn outcome(
    structured_content: Value,
    failure: Option<FailureKind>,
    all_failed: bool,
) -> ToolOutcome {
    ToolOutcome {
        structured_content: crate::security::Sanitized::sealed(structured_content),
        failure,
        all_failed,
    }
}
