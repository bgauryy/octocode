//! What the shared response stages ask ghGetHistoryItem about its output.
use crate::response::output::ToolOutput;
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;

impl ToolOutput for Output {
    /// A missing item's message names the repository, number, or refs it
    /// could not find: no hint repeats it.
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        (code == ErrorCode::NotFound).then_some(crate::response::output::NO_HINT)
    }
}
