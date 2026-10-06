use super::RuntimeError;
use serde_json::{Value, json};

/// Project a typed contract failure into the CallToolResult shape MCP
/// returns: the same actionable projection the CLI prints (`error` plus its
/// repair `details`), pointed at the tool's inputSchema; an output contract
/// violation keeps its cause. Runtime and transport faults remain errors at
/// the NAPI boundary.
pub fn mcp_input_error(tool: &str, error: &RuntimeError) -> Option<Value> {
    // A response that breaks its own output contract names the broken field
    // (contract paths, no request data): the agent sees the cause instead of
    // the interface's generic failure.
    if error.code == "outputContractViolation" {
        let message = crate::security::scrub_error_text(&error.message);
        return Some(json!({
            "content": [{"type":"text","text":format!("{message} (errorCode: outputContractViolation)")}],
            "isError": true
        }));
    }
    (error.code == "invalidInput").then(|| {
        let detail = error
            .payload
            .as_deref()
            .and_then(tool_error_text)
            .unwrap_or_else(|| error.message.clone());
        json!({
            "content": [{"type":"text","text":format!(
                "Input validation error: Invalid arguments for tool {tool}: {detail}"
            )}],
            "isError": true
        })
    })
}

/// `octocode.toolError` → one line: the error, then each repair detail.
fn tool_error_text(payload: &Value) -> Option<String> {
    let error = payload.get("error")?.as_str()?;
    let details = payload
        .get("details")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    Some(if details.is_empty() {
        error.to_owned()
    } else {
        format!("{error} {}", details.join("; "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{self, PrepareOptions};

    fn invalid(tool: &str, input: Value) -> RuntimeError {
        let error = contracts::prepare_many_and_validate(tool, input, PrepareOptions::default())
            .expect_err("invalid input");
        RuntimeError {
            code: "invalidInput".into(),
            message: error.to_string(),
            payload: Some(Box::new(contracts::format_input_error(tool, &error, true))),
            validation_issues: Some(error.issues),
        }
    }

    fn text(tool: &str, input: Value) -> String {
        let result = mcp_input_error(tool, &invalid(tool, input)).expect("an MCP error");
        assert_eq!(result["isError"], true);
        assert!(result.get("structuredContent").is_none());
        result["content"][0]["text"]
            .as_str()
            .expect("text")
            .to_owned()
    }

    #[test]
    fn mcp_errors_carry_the_actionable_repair_details() {
        let typo = text(
            "localFetch",
            json!({"queries":[{"path":"a.rs","matchstring":"x"}]}),
        );
        assert!(
            typo.starts_with("Input validation error: Invalid arguments for tool localFetch: "),
            "{typo}"
        );
        assert!(typo.contains("did you mean 'matchString'?"), "{typo}");
        assert!(typo.contains("See the localFetch inputSchema"), "{typo}");
        assert!(!typo.contains("octocode schema"), "{typo}");

        let missing = text(
            "ghGetFileContent",
            json!({"queries":[{"owner":"a","repo":"b"}]}),
        );
        assert!(
            missing.contains("Set path to a repository-relative file."),
            "{missing}"
        );

        let choice = text(
            "localSearch",
            json!({"queries":[{"path":".","matchString":"x","regex":"perl"}]}),
        );
        assert!(choice.contains("literal, rust, pcre2"), "{choice}");

        let brief = text(
            "localFetch",
            json!({"mainGoal":"g","queries":[{"path":"a.rs"}]}),
        );
        assert!(
            brief.contains("Move 'mainGoal' into each queries[] row"),
            "{brief}"
        );
    }

    #[test]
    fn malformed_envelopes_get_the_same_projection() {
        assert!(text("localFetch", json!({"queries":"x"})).contains("queries must be an array"));
        assert!(text("localFetch", json!({"path":"a.rs"})).contains("{\"queries\":[...]}"));
    }

    /// A response that breaks its own output contract is a tool defect the
    /// agent must see, not a generic "failed to execute": MCP returns the
    /// scrubbed cause as a tool error.
    #[test]
    fn output_contract_violations_surface_their_cause() {
        let error = RuntimeError {
            code: "outputContractViolation".into(),
            message: "ghGetHistoryItem produced a response that violates its canonical output contract: results.0.data.pullRequests.0.files: Value length 0 is below the minimum of 1".into(),
            payload: None,
            validation_issues: None,
        };
        let result = mcp_input_error("ghGetHistoryItem", &error).expect("an MCP error");
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().expect("text");
        assert!(text.contains("outputContractViolation"), "{text}");
        assert!(
            text.contains("pullRequests.0.files: Value length 0"),
            "{text}"
        );
    }

    #[test]
    fn other_failures_stay_runtime_errors() {
        assert!(
            mcp_input_error("localFetch", &RuntimeError::new("toolUnavailable", "x")).is_none()
        );
    }
}
