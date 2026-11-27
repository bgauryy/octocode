//! The text of an MCP `tools/call` reply in the session's
//! `output.mcpResult` format. `json` (the default) is the runtime's one
//! compact JSON projection, unchanged. `text` is the text view the CLI
//! prints, from the same renderer in its MCP form ([`Form::MCP`]): a result
//! without a text view stays JSON, a rejected call prints as a failed row
//! does, and the error flag is the same in both formats.
use crate::cli::lines::{self, Form};
use octocode_native::runtime::RuntimeError;
use octocode_native::runtime::error::{McpResult, mcp_result, tool_error_result};
use octocode_runtime_base::config::OutputMcpResult;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::outcome::ToolOutcome;
use serde_json::Value;

/// The MCP result of a finished call to `tool` in `format`. `query` is the
/// call's input, which the text view reads as the CLI's does (the request a
/// cursor continued when the runtime names it). The view names its root
/// always: a client does not share the server's directory.
pub(in crate::cli) fn reply(
    tool: &str,
    result: Result<ToolOutcome, RuntimeError>,
    format: OutputMcpResult,
    query: &Value,
) -> Result<McpResult, RuntimeError> {
    match (format, result) {
        (OutputMcpResult::Json, result) => mcp_result(tool, result),
        (OutputMcpResult::Text, Ok(outcome)) => {
            let query = outcome.continued.as_ref().unwrap_or(query);
            let view = ToolId::from_name(tool).and_then(|id| {
                lines::render_as(id, &outcome.structured_content, query, None, Form::MCP)
            });
            match view {
                Some(view) => Ok(McpResult {
                    text: view.out,
                    is_error: outcome.is_error(),
                }),
                None => mcp_result(tool, Ok(outcome)),
            }
        }
        (OutputMcpResult::Text, Err(error)) => {
            let text = lines::rejected(&tool_error_result(tool, &error).envelope);
            // The one rule of which rejections are protocol errors, and the
            // error flag of the rest.
            let McpResult { is_error, .. } = mcp_result(tool, Err(error))?;
            Ok(McpResult { text, is_error })
        }
    }
}

#[cfg(test)]
mod tests;
