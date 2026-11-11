use super::*;
use octocode_io::cancellation::Cancellation;
use serde_json::json;
use std::time::{Duration, Instant};

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: Cancellation::new(),
        deadline: Some(Instant::now() + Duration::from_secs(30)),
        response_window: None,
        github_credential: None,
    }
}
fn run(value: Value) -> Result<ToolOutcome, ContractValidationError> {
    let mut structured = Sanitized::sealed(value);
    let mut failure = None;
    settle(
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

/// A warning is the call's when every row states it: said once in the
/// envelope. One some rows state, and one a single row states, stays where
/// it is, as does every warning of a one-row response.
#[test]
fn a_warning_every_row_states_is_said_once_and_the_rest_stay_on_their_rows() {
    let row = |warnings: &[&str]| json!({"path": "a.txt", "content": "exact\n", "totalLines": 1, "warnings": warnings});
    let response = crate::tools::test_support::staged_response(
        ToolId::LocalFetch,
        vec![
            row(&["shared", "two of three"]),
            row(&["two of three", "shared", "own"]),
            row(&["shared"]),
        ],
    );
    assert_eq!(response["warnings"], json!(["shared"]), "{response}");
    let kept = |index: usize| response["results"][index]["data"]["warnings"].clone();
    assert_eq!(kept(0), json!(["two of three"]), "{response}");
    assert_eq!(kept(1), json!(["two of three", "own"]), "{response}");
    assert!(
        response["results"][2]["data"].get("warnings").is_none(),
        "{response}"
    );
    let single =
        crate::tools::test_support::staged_response(ToolId::LocalFetch, vec![row(&["shared"])]);
    assert!(single.get("warnings").is_none(), "{single}");
    assert_eq!(
        single["results"][0]["data"]["warnings"],
        json!(["shared"]),
        "{single}"
    );
}

/// A measured wait is a fact of the call that measured it. The settled
/// envelope, which a page replay stores and serves again, holds none; the
/// issuing response states each once; a page cut from the stored envelope
/// states none.
#[test]
fn a_timed_notice_is_stated_by_the_issuing_call_and_on_no_replayed_page() {
    const WAITED: &str = "This call waited {} for the server to load.";
    let rows = (0..4)
        .map(|index| {
            let mut data = json!({
                "path": format!("f{index}.txt"),
                "content": format!("{}\n", "line of evidence ".repeat(40)),
                "totalLines": 1,
            });
            crate::response::paging::push_timed_notice(&mut data, WAITED, 2.5 + f64::from(index));
            json!({"index": index, "data": data})
        })
        .collect::<Vec<_>>();
    let mut structured = Sanitized::sealed(json!({"results": rows}));
    let mut failure = None;
    let queries = json!({"queries": (0..4).map(|index| json!({"path": format!("f{index}.txt")})).collect::<Vec<_>>()});
    let call_notes =
        settle(ToolId::LocalFetch, &mut structured, &queries, &mut failure).expect("settled");
    // One sentence for the call, with the longest wait any row measured.
    assert_eq!(
        call_notes,
        ["This call waited 5.5 s for the server to load."]
    );
    let saved = |replayed| {
        Some(crate::response::page_model::SavedResponse {
            key: [7; 32],
            expires_at: "2099-01-01T00:00:00Z".to_owned(),
            replayed,
        })
    };
    let input = |options| StageInput {
        tool: ToolId::LocalFetch,
        structured: Sanitized::sealed(Value::Null),
        options,
        failure: None,
        auto_page_chars: 20_000,
        allow_auto_paging: true,
    };
    let mut kept = None;
    let first = respond(
        StageInput {
            structured,
            ..input(ResponsePageOptions {
                response_chars: Some(900),
                paging: true,
                saved: saved(false),
                ..Default::default()
            })
        },
        Some(&mut kept),
        &context(),
    )
    .expect("first page");
    let seed = kept.expect("a paged response keeps its replay seed");
    assert!(!seed.to_string().contains("This call waited"), "{seed}");
    let mut issued = first.result.expect("valid first page");
    assert!(
        !issued
            .structured_content
            .to_string()
            .contains("This call waited")
    );
    state_call_notes(&mut issued, &call_notes);
    assert_eq!(
        issued.structured_content["warnings"],
        json!(call_notes),
        "{}",
        issued.structured_content
    );
    let mut options = ResponsePageOptions {
        response_chars: Some(900),
        saved: saved(true),
        ..Default::default()
    };
    let pages = super::super::pager::SavedPages::default();
    let mut next = first.next;
    let mut replayed = 0;
    while let Some(cursor) = next {
        options.open(&cursor);
        let page = respond_saved(input(options.clone()), &seed, &pages, &context())
            .expect("replayed page");
        let content = page.result.expect("valid page").structured_content;
        assert_eq!(content["responsePagination"]["replayed"], true, "{content}");
        assert!(
            !content.to_string().contains("This call waited"),
            "{content}"
        );
        replayed += 1;
        next = page.next;
    }
    assert!(replayed > 0, "the response pages");
}
