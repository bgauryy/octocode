//! The one projection of a rejected call. The CLI (JSON and terminal), the
//! warm server, and MCP all answer a [`RuntimeError`] with
//! [`tool_error_result`]: the typed `{error, details?, errorCode}` envelope
//! (MCP structured and text content) and its one-line terminal text.

use super::RuntimeError;
use crate::id::ToolId;
use crate::response::outcome::ToolOutcome;
use crate::security::Sanitized;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

/// A rejected call as every surface prints it.
#[derive(Clone, Debug, PartialEq)]
pub struct RejectedCall {
    /// `{error, details?, errorCode}`: MCP structuredContent (and, serialized,
    /// its text content), CLI JSON stdout. Every string leaf is scrubbed.
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
        _ => crate::contracts::tool_error(&error.message, Vec::new()),
    };
    if envelope.get("errorCode").is_none() {
        envelope["errorCode"] = json!(error.code);
    }
    if error.code == ErrorCode::Timeout && envelope.get("details").is_none() {
        if let Some(hint) = ToolId::from_name(tool)
            .and_then(|id| crate::response::rows::error_code_hint(id, error.code, &Value::Null))
            .filter(|hint| !hint.is_empty())
        {
            envelope["details"] = json!([hint]);
        }
    } else if let Some(id) = ToolId::from_name(tool) {
        crate::response::rows::attach_error_hint(id, &mut envelope, &Value::Null);
    }
    let envelope = Sanitized::scrubbed(envelope);
    let code = envelope["errorCode"]
        .as_str()
        .unwrap_or(crate::id::error_codes::as_str(error.code));
    let detail =
        error_line(&envelope).unwrap_or_else(|| crate::security::scrub_error_text(&error.message));
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

/// The MCP `CallToolResult` of a finished call. Its one text block is the
/// serialized JSON of its own `structuredContent` (MCP server/tools: a tool
/// returning structured content also returns it serialized as text), so a
/// client that reads only `content` sees the same result. A rejected call
/// answers as a tool error; only a cancelled request or a closing runtime
/// stays an error, because no client is waiting for its answer.
pub fn mcp_result(
    tool: &str,
    result: Result<ToolOutcome, RuntimeError>,
) -> Result<Value, RuntimeError> {
    let (structured, is_error) = match result {
        Ok(outcome) => (outcome.structured_content.into_value(), outcome.all_failed),
        Err(error) if matches!(error.code, ErrorCode::Cancelled | ErrorCode::Closed) => {
            return Err(error);
        }
        Err(error) => (tool_error_result(tool, &error).envelope.into_value(), true),
    };
    let text = serde_json::to_string(&structured)
        .map_err(|_| RuntimeError::new(ErrorCode::Response, "Cannot serialize response"))?;
    let mut response = serde_json::Map::with_capacity(3);
    response.insert("content".into(), json!([{"type": "text", "text": text}]));
    response.insert("structuredContent".into(), structured);
    response.insert("isError".into(), Value::Bool(is_error));
    Ok(Value::Object(response))
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
mod tests {
    use super::*;
    use crate::contracts;

    fn invalid(tool: &str, input: Value) -> RuntimeError {
        let error = contracts::prepare_many_and_validate(tool, input).expect_err("invalid input");
        RuntimeError {
            code: ErrorCode::InvalidInput,
            message: error.to_string(),
            payload: Some(Box::new(contracts::format_input_error(tool, &error))),
            validation_issues: Some(error.issues),
        }
    }

    /// The MCP tool error of `error`: its text is its envelope's JSON.
    fn mcp(tool: &str, error: RuntimeError) -> Value {
        let result = mcp_result(tool, Err(error)).expect("an MCP tool error");
        assert_eq!(
            result["content"],
            json!([{"type":"text","text":result["structuredContent"].to_string()}]),
            "{result}"
        );
        result
    }

    /// The terminal line of a rejected `input`, after checking its MCP
    /// envelope.
    fn text(tool: &str, input: Value) -> String {
        let result = mcp(tool, invalid(tool, input.clone()));
        assert_eq!(result["isError"], true);
        // The structured row is the CLI's error envelope, typed: an agent
        // reading structuredContent branches on errorCode. The caller knows
        // the tool, so there is no kind/version/tool frame.
        let envelope = &result["structuredContent"];
        for frame in ["kind", "version", "tool"] {
            assert!(
                envelope.get(frame).is_none(),
                "{frame} on {tool}: {envelope}"
            );
        }
        assert!(envelope["error"].is_string(), "{envelope}");
        assert_eq!(envelope["errorCode"], "invalidInput", "{envelope}");
        let text = tool_error_result(tool, &invalid(tool, input)).text;
        assert!(text.ends_with("(errorCode: invalidInput)"), "{text}");
        text
    }

    /// An invalidInput error with no formatted payload (a runtime-level
    /// rejection) still answers with the typed envelope.
    #[test]
    fn bare_invalid_input_errors_are_typed() {
        let result = mcp(
            "clasify",
            RuntimeError::new(ErrorCode::InvalidInput, "clasify pages by itself"),
        );
        assert_eq!(
            result["structuredContent"],
            json!({"error":"clasify pages by itself","errorCode":"invalidInput",
                "hints":[crate::response::rows::error_code_hint(ToolId::Clasify, ErrorCode::InvalidInput, &Value::Null).expect("input recovery")]})
        );
    }

    #[test]
    fn invalid_inputs_keep_structured_field_errors_without_repairing_the_request() {
        for (tool, input) in [
            (
                "localFetch",
                json!({"queries":[{"path":"a.rs","matchstring":"x"}]}),
            ),
            (
                "ghGetFileContent",
                json!({"queries":[{"owner":"a","repo":"b"}]}),
            ),
            (
                "localSearch",
                json!({"queries":[{"path":".","matchString":["x"],"regexEngine":"perl"}]}),
            ),
            (
                "localFetch",
                json!({"mainGoal":"g","queries":[{"path":"a.rs"}]}),
            ),
            ("localFetch", json!({"queries":"x"})),
            ("localFetch", json!({"path":"a.rs"})),
        ] {
            let rejected = invalid(tool, input.clone());
            let issues = rejected
                .validation_issues
                .as_ref()
                .expect("structured issues");
            assert!(!issues.is_empty(), "{tool}: {input}");
            let projected = tool_error_result(tool, &rejected);
            let response = mcp(tool, rejected);
            assert_eq!(
                response["structuredContent"],
                projected.envelope.into_value()
            );
            let envelope = &response["structuredContent"];
            assert!(
                envelope.get("next").is_none(),
                "no guessed continuation: {envelope}"
            );
            assert!(
                envelope.get("related").is_none(),
                "no rewritten request: {envelope}"
            );
            assert!(!text(tool, input).is_empty());
        }
    }

    #[test]
    fn terminal_errors_preserve_each_detail_and_prose_hint() {
        let details = [
            "The path is outside the workspace.",
            "Choose a readable path.",
        ];
        let hint = "The original request was not executed.";
        let line = error_line(&json!({"error":"bad","errorCode":"invalidInput",
            "details":details,"hints":[hint]}))
        .expect("line");
        for evidence in ["bad", details[0], details[1], hint] {
            assert_eq!(line.matches(evidence).count(), 1, "{line}");
        }
    }

    /// A response that breaks its own output contract is a tool defect the
    /// agent must see, not a generic "failed to execute": MCP returns the
    /// scrubbed cause as a typed tool error.
    #[test]
    fn output_contract_violations_surface_their_cause() {
        let error = RuntimeError::new(
            ErrorCode::OutputContractViolation,
            "ghGetHistoryItem produced a response that violates its canonical output contract: results.0.data.pullRequests.0.files: Value length 0 is below the minimum of 1",
        );
        let text = tool_error_result("ghGetHistoryItem", &error).text;
        let result = mcp("ghGetHistoryItem", error);
        assert_eq!(result["isError"], true);
        assert_eq!(
            result["structuredContent"]["errorCode"],
            "outputContractViolation"
        );
        assert!(
            text.ends_with("(errorCode: outputContractViolation)"),
            "{text}"
        );
        assert!(
            text.contains("pullRequests.0.files: Value length 0"),
            "{text}"
        );
    }

    /// Every request-level failure is a typed tool error on MCP, as on the
    /// CLI; only a cancelled request or a closing runtime stays an error.
    #[test]
    fn every_failure_but_cancellation_is_a_tool_error() {
        for code in [
            ErrorCode::ToolUnavailable,
            ErrorCode::MissingConfiguration,
            ErrorCode::Busy,
            ErrorCode::Timeout,
        ] {
            let result = mcp("localFetch", RuntimeError::new(code, "m"));
            assert_eq!(result["isError"], true, "{code}");
            assert_eq!(
                result["structuredContent"]["errorCode"],
                json!(code),
                "{code}"
            );
            let text = tool_error_result("localFetch", &RuntimeError::new(code, "m")).text;
            assert!(text.starts_with("m"), "{code}: {text}");
            assert!(text.ends_with(&format!("(errorCode: {code})")), "{text}");
        }
        for code in [ErrorCode::Cancelled, ErrorCode::Closed] {
            let error = mcp_result("localFetch", Err(RuntimeError::new(code, "m")))
                .expect_err("no client waits for it");
            assert_eq!(error.code, code);
        }
    }

    /// A timeout tells every tool's caller what to try; only lspSearch blames
    /// the language server's cold start.
    #[test]
    fn every_timeout_names_its_next_step() {
        let timeout = || RuntimeError::new(ErrorCode::Timeout, "Tool call timed out.");
        let lsp = tool_error_result("lspSearch", &timeout());
        assert!(lsp.text.starts_with("Tool call timed out."), "{}", lsp.text);
        assert!(
            lsp.text.contains("initialises the language server"),
            "{}",
            lsp.text
        );
        for other in ["localSearch", "ghSearchCode", "artifactSearch"] {
            let result = tool_error_result(other, &timeout());
            assert!(
                result.text.contains("Retry once with a smaller scope"),
                "{other}: {}",
                result.text
            );
            assert!(!result.text.contains("language server"), "{other}");
            assert_eq!(result.envelope["details"].as_array().map(Vec::len), Some(1));
        }
    }

    #[test]
    fn errors_are_secret_scrubbed_in_envelope_and_text() {
        let token = format!("ghp_{}", "a".repeat(37));
        for payload in [
            None,
            Some(json!({"error": format!("x {token}"), "detail": [token.clone()]})),
        ] {
            let error = RuntimeError {
                code: ErrorCode::ProviderError,
                message: format!("upstream rejected {token}"),
                payload: payload.map(Box::new),
                validation_issues: None,
            };
            let result = tool_error_result("localFetch", &error);
            let printed = format!("{} {}", result.envelope, result.text);
            assert!(!printed.contains("ghp_"), "{printed}");
            assert!(printed.contains("[REDACTED-"), "{printed}");
        }
    }
}
