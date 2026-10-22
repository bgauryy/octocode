//! What the shared response stages ask ghGetHistoryItem about its output.
use crate::response::output::{TextShape, ToolOutput};
use serde_json::Value;

pub(crate) struct Output;

impl ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Verify owner/repo and the number, ref, or compare refs."
    }
    /// A missing item's message names the repository, number, or refs it
    /// could not find: no hint repeats it.
    fn error_hint(&self, code: &str) -> Option<&'static str> {
        (code == "notFound").then_some(crate::response::output::NO_HINT)
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "provider"
    }
    fn text_shape(&self) -> TextShape {
        TextShape::Diff
    }
}
