use super::*;
use crate::id::ToolId;
use serde_json::json;

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

/// Recovery guidance is per-row evidence: every failed row of a batch that
/// needs the shared recovery carries it in its own `hints`, as the shaping
/// stage attaches it; a row with its own hints keeps them.
#[test]
fn every_error_row_of_a_batch_carries_its_own_shared_hint() {
    let failed = || json!({"status":"error","data":{"errorCode":"notFound","error":"gone"}});
    let mut structured = json!({"results":[failed(), failed(),
        {"status":"error","data":{"errorCode":"notFound","error":"gone","hints":["own"]}}]});
    for row in structured["results"].as_array_mut().expect("rows") {
        attach_error_hint(ToolId::GhStructure.output(), &mut row["data"], &Value::Null);
    }
    crate::response::notices::place(&mut structured, "results");
    let hints = |index: usize| structured["results"][index]["data"].get("hints").cloned();
    assert_eq!(hints(0), hints(1), "{structured}");
    assert!(hints(1).is_some(), "second row has its hint: {structured}");
    assert_eq!(hints(2), Some(json!(["own"])), "{structured}");
    assert!(!structured.to_string().contains("notices"), "{structured}");
}
