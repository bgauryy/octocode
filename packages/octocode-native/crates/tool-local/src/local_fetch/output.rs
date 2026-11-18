//! What the shared response stages ask about localFetch output.

use octocode_contract_types::ErrorCode;
use octocode_runtime_base::response::output::ToolOutput;
use serde_json::Value;

pub struct Output;
impl ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        match code {
            ErrorCode::FileAccessFailed => {
                Some(octocode_runtime_base::response::output::LIST_FILES_HINT)
            }
            // The message names the item; `repair` is the corrected read.
            // A malformed page cursor: the message names the bad offset.
            // `notAFile`: a directory's tree is its lead; a FIFO or device
            // has nothing to retry.
            ErrorCode::InvalidPattern | ErrorCode::InvalidPagination | ErrorCode::NotAFile => {
                Some(octocode_runtime_base::response::output::NO_HINT)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
