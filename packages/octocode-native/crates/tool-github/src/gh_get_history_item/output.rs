//! What the shared response stages ask ghGetHistoryItem about its output:
//! its own recovery for a missing item; the shared table answers the rest.
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::response::output::ToolOutput;
use serde_json::Value;

pub struct Output;

impl ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        (code == ErrorCode::NotFound).then_some(NOT_FOUND_HINT)
    }
}

/// A history item is named by its repository and its number or ref, never
/// by a path: the shared hint's parent-directory listing does not apply.
/// A row with a related[] listing (the repository's recent items, its refs,
/// or a repository search) carries no hint, so the hint never names it.
const NOT_FOUND_HINT: &str = "Check owner, repo and the number or ref as GitHub writes them.";

#[cfg(test)]
mod tests;
