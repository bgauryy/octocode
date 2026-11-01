use super::*;
use crate::id::ToolId;

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
            attach_error_hint(tool.output(), &mut data, &Value::Null);
            assert_eq!(data["error"], "original cause");
            assert!(
                data["hints"][0]
                    .as_str()
                    .is_some_and(|hint| !hint.is_empty()),
                "{tool}: {data}"
            );
            let once = data.clone();
            attach_error_hint(tool.output(), &mut data, &Value::Null);
            assert_eq!(data, once, "recovery is not repeated");
        }
    }
}

#[test]
fn producer_guidance_and_explicit_no_hint_are_preserved() {
    let mut data = json!({"errorCode":"server","hints":["provider-specific cause and remedy"]});
    let original = data.clone();
    attach_error_hint(ToolId::GhSearchCode.output(), &mut data, &Value::Null);
    assert_eq!(data, original);
    let mut data =
        json!({"errorCode":"languageMismatch","error":"the message already names both repairs"});
    let original = data.clone();
    attach_error_hint(ToolId::AstSearch.output(), &mut data, &Value::Null);
    assert_eq!(data, original);
}

#[test]
fn transient_guidance_respects_final_failures() {
    let mut data = json!({"errorCode":"transport","error":"connection failed","retryable":false});
    attach_error_hint(ToolId::ArtifactSearch.output(), &mut data, &Value::Null);
    assert_eq!(data["retryable"], false);
    assert!(
        data["hints"][0]
            .as_str()
            .expect("guidance")
            .contains("only if retryable is true")
    );
}

/// A batch's error rows that share a recovery state it once per response:
/// the first failed row carries it in `hints`, later ones do not repeat it,
/// and a row with its own hints keeps them.
#[test]
fn a_shared_error_hint_is_stated_once_per_response() {
    let failed = || json!({"status":"error","data":{"errorCode":"transport","error":"down"}});
    let mut structured = json!({"results":[failed(), failed(),
        {"status":"error","data":{"errorCode":"transport","error":"down","hints":["own"]}}]});
    for row in structured["results"].as_array_mut().expect("rows") {
        attach_error_notice(
            ToolId::GhSearchCode.output(),
            &mut row["data"],
            &Value::Null,
        );
    }
    crate::response::notices::place(&mut structured, "results");
    let hints = |index: usize| structured["results"][index]["data"].get("hints").cloned();
    assert_eq!(hints(0), Some(json!([TRANSIENT_HINT])), "{structured}");
    assert_eq!(hints(1), None, "said once: {structured}");
    assert_eq!(hints(2), Some(json!(["own"])), "{structured}");
    assert!(!structured.to_string().contains("notices"), "{structured}");
}
