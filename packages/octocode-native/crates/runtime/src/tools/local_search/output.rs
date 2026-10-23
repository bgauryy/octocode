//! What the shared response stages ask about localSearch output.

use crate::response::output::{PathAnchor, ToolOutput};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Broaden matchString, path, or filters."
    }
    fn error_hint(&self, code: ErrorCode) -> Option<&'static str> {
        // No file under the path could be read: access, not the search terms.
        (code == ErrorCode::FileAccessFailed)
            .then_some("Check read access to the path; a broader search reads the same files.")
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "lexical"
    }
    fn path_anchor(&self) -> PathAnchor {
        PathAnchor::Workspace
    }
    fn reads_worktree(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scope no file of which could be read is not fixed by a broader
    /// search: its tip says so.
    #[test]
    fn an_unreadable_scope_is_not_told_to_broaden() {
        let hint = Output
            .error_hint(ErrorCode::FileAccessFailed)
            .expect("a tip");
        assert!(!hint.contains("Broaden"), "{hint}");
    }
}
