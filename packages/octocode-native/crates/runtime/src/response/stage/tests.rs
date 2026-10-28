use super::*;
use serde_json::json;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: CancellationToken::new(),
        deadline: Instant::now() + Duration::from_secs(30),
        walk_threads: None,
        response_window: None,
        github_credential: None,
    }
}
fn run(value: Value) -> Result<ToolOutcome, ContractValidationError> {
    let mut structured = Sanitized::sealed(value);
    let mut failure = None;
    let remaining = settle(
        ToolId::LocalFetch,
        &mut structured,
        &json!({"queries":[{"path":"a.txt"}]}),
        &mut failure,
    )?;
    respond(
        StageInput {
            tool: ToolId::LocalFetch,
            structured,
            options: ResponsePageOptions::default(),
            failure,
            auto_page_chars: 20_000,
            allow_auto_paging: true,
        },
        remaining,
        None,
        &context(),
    )
    .expect("stage execution")
    .result
}

#[test]
fn invalid_rows_are_explicit_failures_and_do_not_remove_other_rows() {
    let outcome=run(json!({"results":[{"index":0,"data":{"path":"a.txt","content":"exact\n","totalLines":1}},{"index":1,"data":{"path":7}}]})).expect("isolated error");
    assert_eq!(
        outcome.structured_content["results"]
            .as_array()
            .expect("rows")
            .len(),
        2
    );
    assert_eq!(outcome.structured_content["results"][1]["status"], "error");
    let failed = &outcome.structured_content["results"][1]["data"];
    assert_eq!(failed["errorCode"], "outputContractViolation");
    assert!(
        failed["hints"][0]
            .as_str()
            .expect("contract recovery")
            .contains("tool failure")
    );
    assert!(outcome.failure.is_some());
    assert!(!outcome.all_failed);
    assert!(
        outcome.structured_content["results"][0]["data"]["content"]
            .as_str()
            .expect("content")
            .contains("exact")
    );
}

#[test]
fn malformed_envelopes_are_rejected_before_snapshotting() {
    assert!(run(json!({"results":"not an array"})).is_err());
    assert!(run(json!({"results":[],"responseWindow":"retired"})).is_err());
}

#[test]
fn every_failed_row_is_required_for_all_failed() {
    assert!(response_all_failed(
        &json!({"results":[{"index":0,"status":"error","data":{}},{"index":1,"status":"error","data":{}}]})
    ));
    assert!(!response_all_failed(
        &json!({"results":[{"index":0,"status":"error","data":{}},{"index":1,"data":{}}]})
    ));
    assert!(!response_all_failed(&json!({"results":[]})));
}

#[test]
fn cancellation_preserves_its_failure_class() {
    assert_eq!(
        response_failure(ResponseError::Cancelled),
        ExecutionError::Cancelled
    );
}
