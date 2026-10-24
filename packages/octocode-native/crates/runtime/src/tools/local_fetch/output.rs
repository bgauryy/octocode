//! What the shared response stages ask about localFetch output.

use crate::response::output::ToolOutput;
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        match code {
            ErrorCode::FileAccessFailed => Some(crate::response::output::LIST_FILES_HINT),
            // The message names the item; `repair` is the corrected read.
            // A malformed page cursor: the message names the bad offset.
            // `notAFile`: a directory's tree is its lead; a FIFO or device
            // has nothing to retry.
            ErrorCode::InvalidPattern | ErrorCode::InvalidPagination | ErrorCode::NotAFile => {
                Some(crate::response::output::NO_HINT)
            }
            _ => None,
        }
    }

    fn reads_source(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_progress_uses_the_error_without_a_redundant_hint() {
        assert_eq!(
            Output.error_hint(ErrorCode::InvalidPagination, &Value::Null),
            Some(crate::response::output::NO_HINT)
        );
    }
}
