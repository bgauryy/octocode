use super::*;

#[test]
fn shared_failures_have_recovery_across_every_tool() {
    for tool in ToolId::ALL {
        for code in [
            ErrorCode::Authentication,
            ErrorCode::RateLimited,
            ErrorCode::ExecutionFailed,
            ErrorCode::OutputContractViolation,
            ErrorCode::ToolUnavailable,
            ErrorCode::SourceMismatch,
        ] {
            let mut data = json!({"errorCode":code,"error":"original cause"});
            attach_error_hint(tool, &mut data, &Value::Null);
            assert_eq!(data["error"], "original cause");
            assert!(
                data["hints"][0]
                    .as_str()
                    .is_some_and(|hint| !hint.is_empty()),
                "{tool}: {data}"
            );
            let once = data.clone();
            attach_error_hint(tool, &mut data, &Value::Null);
            assert_eq!(data, once, "recovery is not repeated");
        }
    }
}

#[test]
fn producer_guidance_and_explicit_no_hint_are_preserved() {
    let mut data = json!({"errorCode":"server","hints":["provider-specific cause and remedy"]});
    let original = data.clone();
    attach_error_hint(ToolId::GhSearchCode, &mut data, &Value::Null);
    assert_eq!(data, original);
    let mut data =
        json!({"errorCode":"languageMismatch","error":"the message already names both repairs"});
    let original = data.clone();
    attach_error_hint(ToolId::AstSearch, &mut data, &Value::Null);
    assert_eq!(data, original);
}

#[test]
fn transient_guidance_respects_final_failures() {
    let mut data = json!({"errorCode":"transport","error":"connection failed","retryable":false});
    attach_error_hint(ToolId::ArtifactSearch, &mut data, &Value::Null);
    assert_eq!(data["retryable"], false);
    assert!(
        data["hints"][0]
            .as_str()
            .expect("guidance")
            .contains("only if retryable is true")
    );
}
