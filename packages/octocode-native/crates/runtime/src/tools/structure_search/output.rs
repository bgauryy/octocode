//! What the shared response stages ask about structureSearch output.

use crate::tools::output::{PathAnchor, ToolOutput};
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Broaden path, depth, or file filters."
    }
    fn error_hint(&self, code: &str) -> Option<&'static str> {
        (code == crate::policy::PATH_POLICY_DENIED)
            .then_some(crate::policy::discovery::WITHHELD_HINT)
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "exact"
    }
    fn path_anchor(&self) -> PathAnchor {
        PathAnchor::QueryParent
    }
    fn reads_worktree(&self) -> bool {
        true
    }
}
