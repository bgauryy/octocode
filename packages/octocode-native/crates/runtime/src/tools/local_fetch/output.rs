//! What the shared response stages ask about localFetch output.

use crate::response::output::{PathAnchor, TextShape, ToolOutput};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn fallback_hint(&self, query: &Value) -> &'static str {
        if query.get("matchString").is_some() {
            "Verify path/range, or remove matchString."
        } else {
            "Verify path/range."
        }
    }
    fn error_hint(&self, code: ErrorCode) -> Option<&'static str> {
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
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "exact"
    }
    fn path_anchor(&self) -> PathAnchor {
        PathAnchor::Workspace
    }
    fn reads_worktree(&self) -> bool {
        true
    }
    fn text_shape(&self) -> TextShape {
        TextShape::FileText
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Tips follow the read: only a match read is told about matchString,
    /// and a malformed page cursor's message is its whole answer.
    #[test]
    fn tips_follow_the_read() {
        let plain = Output.fallback_hint(&json!({"path":"a.rs","ranges":["1-3"]}));
        assert!(!plain.contains("matchString"), "{plain}");
        let matched = Output.fallback_hint(&json!({"path":"a.rs","matchString":"x"}));
        assert!(matched.contains("matchString"), "{matched}");
        assert_eq!(
            Output.error_hint(ErrorCode::InvalidPagination),
            Some(crate::response::output::NO_HINT)
        );
    }
}
