//! What the shared response stages ask about localSearch output.

use octocode_contract_types::ErrorCode;
use octocode_runtime_base::response::output::ToolOutput;
use serde_json::Value;

pub struct Output;
impl ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, _query: &Value) -> Option<&'static str> {
        Some(match code {
            // No file under the path could be read: access, not the search terms.
            ErrorCode::FileAccessFailed => {
                "Check read access to the path; a broader search reads the same files."
            }
            // A pattern's fix depends on its cause, which the check that
            // found it states (`pattern::check_expression`).
            ErrorCode::InvalidPattern => return None,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests;
