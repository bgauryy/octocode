//! What the shared response stages ask about localFetch output.

use crate::tools::output::{PathAnchor, TextShape, ToolOutput};
use serde_json::Value;

pub(crate) struct Output;
impl ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Verify path/range, or remove matchString."
    }
    fn error_hint(&self, code: &str) -> Option<&'static str> {
        match code {
            "fileAccessFailed" => Some(crate::tools::output::LIST_FILES_HINT),
            "notAFile" => Some("Read a file inside it; hints.viewTree lists its entries."),
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
