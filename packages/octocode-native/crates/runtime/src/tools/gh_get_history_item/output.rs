//! What the shared response stages ask ghGetHistoryItem about its output.
use crate::tools::output::{TextShape, ToolOutput};
use serde_json::Value;

pub(crate) struct Output;

impl ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Verify owner/repo and the number, ref, or compare refs."
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "provider"
    }
    fn text_shape(&self) -> TextShape {
        TextShape::Diff
    }
}
