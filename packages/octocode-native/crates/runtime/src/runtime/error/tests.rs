use super::*;
use octocode_runtime_base::contracts;
use octocode_test_support::json_fixture;

fn invalid(tool: &str, input: Value) -> RuntimeError {
    let error = contracts::prepare_rows(tool, input).expect_err("invalid input");
    RuntimeError {
        code: ErrorCode::InvalidInput,
        message: error.to_string(),
        payload: Some(Box::new(contracts::format_input_error(
            tool,
            &error,
            &serde_json::Value::Null,
        ))),
        validation_issues: Some(error.issues),
    }
}

/// The MCP tool error of `error`: its text is its envelope's JSON.
fn mcp(tool: &str, error: RuntimeError) -> Value {
    let result = mcp_result(tool, Err(error)).expect("an MCP tool error");
    json!({"content":[{"type":"text","text":result.text}],"isError":result.is_error})
}

fn payload(result: &Value) -> Value {
    assert!(result.get("structuredContent").is_none(), "{result}");
    serde_json::from_str(result["content"][0]["text"].as_str().expect("text")).expect("JSON text")
}

/// The terminal line of a rejected `input`, after checking its MCP
/// envelope.
fn text(tool: &str, input: Value) -> String {
    let result = mcp(tool, invalid(tool, input.clone()));
    assert_eq!(result["isError"], true);
    // The text row is the CLI's error envelope, typed: an agent branches
    // on errorCode. The caller knows the tool, so there is no frame.
    let envelope = payload(&result);
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
        payload(&result),
        json_fixture(
            r#"{
                "error": "clasify pages by itself",
                "errorCode": "invalidInput",
                "errorClass": "correctInput"
            }"#
        )
    );
}

#[test]
fn invalid_inputs_keep_structured_field_errors_without_repairing_the_request() {
    for (tool, input) in [
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": "a.rs", "matchstring": "x"}]}"#),
        ),
        (
            "ghGetFileContent",
            json_fixture(r#"{"queries": [{"owner": "a", "repo": "b"}]}"#),
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [{"path": ".", "matchString": ["x"], "regexEngine": "perl"}]
                }"#,
            ),
        ),
        (
            "localFetch",
            json_fixture(r#"{"mainGoal": "g", "queries": [{"path": "a.rs"}]}"#),
        ),
        ("localFetch", json_fixture(r#"{"queries": "x"}"#)),
        ("localFetch", json_fixture(r#"{"path": "a.rs"}"#)),
    ] {
        let rejected = invalid(tool, input.clone());
        let issues = rejected
            .validation_issues
            .as_ref()
            .expect("structured issues");
        assert!(!issues.is_empty(), "{tool}: {input}");
        let projected = tool_error_result(tool, &rejected);
        let response = mcp(tool, rejected);
        let envelope = payload(&response);
        assert_eq!(envelope, projected.envelope.into_value());
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
    assert_eq!(payload(&result)["errorCode"], "outputContractViolation");
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
        assert_eq!(payload(&result)["errorCode"], json!(code), "{code}");
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

/// A timeout tells every tool's caller what to try; lspSearch says whether
/// the server was still working or idle and names the exact fallback.
#[test]
fn every_timeout_names_its_next_step() {
    let timeout = || RuntimeError::new(ErrorCode::Timeout, "Tool call timed out.");
    let lsp = tool_error_result("lspSearch", &timeout());
    assert!(lsp.text.starts_with("Tool call timed out."), "{}", lsp.text);
    assert!(
        lsp.text.contains("still working") && lsp.text.contains("localSearch + localFetch"),
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
