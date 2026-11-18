//! The one projection of a rejected call. The CLI (JSON and terminal), the
//! warm server, and MCP all answer a [`RuntimeError`] with
//! [`tool_error_result`]: the typed `{error, details?, errorCode}` envelope
//! (MCP text content and CLI JSON) and its one-line terminal text.

use super::RuntimeError;
use crate::runtime::tool_output::ToolOutputOf;
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::outcome::ToolOutcome;
use octocode_runtime_base::response::wire::wire_json;
use octocode_runtime_base::security::Sanitized;
use serde_json::{Value, json};

/// A rejected call as every surface prints it.
#[derive(Clone, Debug, PartialEq)]
pub struct RejectedCall {
    /// `{error, details?, errorCode}`: serialized as MCP text content and CLI
    /// JSON stdout. Every string leaf is scrubbed.
    pub envelope: Sanitized,
    /// The error, each repair detail, then `(errorCode: code)`: CLI terminal
    /// stderr.
    pub text: String,
}

/// Project `error`, raised by a call to `tool`, for every surface. The
/// envelope is the formatted payload validation built, else the message,
/// typed with the error's `errorCode`; a timeout without details names what
/// to try next for that tool. Every string is secret-scrubbed.
#[must_use]
pub fn tool_error_result(tool: &str, error: &RuntimeError) -> RejectedCall {
    let mut envelope = match error.payload.as_deref() {
        Some(payload @ Value::Object(_)) => payload.clone(),
        _ => octocode_runtime_base::contracts::tool_error(&error.message, Vec::new()),
    };
    if envelope.get("errorCode").is_none() {
        envelope["errorCode"] = json!(error.code);
    }
    if error.code == ErrorCode::Timeout && envelope.get("details").is_none() {
        if let Some(hint) = ToolId::from_name(tool)
            .and_then(|id| {
                octocode_runtime_base::response::rows::error_code_hint(
                    id.output(),
                    error.code,
                    &Value::Null,
                )
            })
            .filter(|hint| !hint.is_empty())
        {
            envelope["details"] = json!([hint]);
        }
    } else if let Some(id) = ToolId::from_name(tool) {
        octocode_runtime_base::response::rows::attach_error_hint(
            id.output(),
            &mut envelope,
            &Value::Null,
        );
    }
    // A rejected call carries its class only: `retryable` is a row field.
    envelope["errorClass"] = json!(octocode_runtime_base::id::error_class(
        envelope["errorCode"].as_str().unwrap_or_default()
    ));
    let envelope = Sanitized::scrubbed(envelope);
    let code = envelope["errorCode"]
        .as_str()
        .unwrap_or(octocode_runtime_base::id::error_codes::as_str(error.code));
    let detail = error_line(&envelope)
        .unwrap_or_else(|| octocode_runtime_base::security::scrub_error_text(&error.message));
    // The MCP SDK's own wording for a rejected input, so a client keyed on it
    // sees native validation the same way.
    let text = if error.code == ErrorCode::InvalidInput {
        format!(
            "Input validation error: Invalid arguments for tool {tool}: {detail} (errorCode: {code})"
        )
    } else {
        format!("{detail} (errorCode: {code})")
    };
    RejectedCall { envelope, text }
}

/// The parts of an MCP `CallToolResult`: one text block carrying the shared
/// compact JSON projection, and the tool error flag. No structured copy is
/// produced, so every host reads the same evidence exactly once.
#[derive(Debug)]
pub struct McpResult {
    pub text: String,
    pub is_error: bool,
}

/// The MCP result of a finished call ([`McpResult`]). A rejected call
/// answers as a tool error; only a cancelled request or a closing runtime
/// stays an error, because no client is waiting for its answer.
pub fn mcp_result(
    tool: &str,
    result: Result<ToolOutcome, RuntimeError>,
) -> Result<McpResult, RuntimeError> {
    let (text, is_error) = match result {
        Ok(outcome) => (wire_json(&outcome.structured_content), outcome.is_error()),
        Err(error) if matches!(error.code, ErrorCode::Cancelled | ErrorCode::Closed) => {
            return Err(error);
        }
        Err(error) => {
            let projected = tool_error_result(tool, &error);
            (wire_json(&projected.envelope), true)
        }
    };
    Ok(McpResult { text, is_error })
}

/// An error envelope as one line, preserving the error, details and prose hints.
fn error_line(payload: &Value) -> Option<String> {
    let error = payload.get("error")?.as_str()?;
    let mut clauses = vec![error.to_owned()];
    clauses.extend(
        payload
            .get("details")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    clauses.extend(
        payload
            .get("hints")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned),
    );
    // Keep each detail visible to text-only clients.
    Some(clauses.join("; "))
}

#[cfg(test)]
mod tests;
