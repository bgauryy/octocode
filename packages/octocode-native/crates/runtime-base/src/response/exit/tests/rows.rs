use super::*;
use octocode_test_support::json_fixture;

#[test]
fn a_missing_target_is_correct_input_a_file_failure_is_unservable() {
    use crate::id::error_codes::class;
    assert_eq!(class(ErrorCode::NotFound), ErrorClass::CorrectInput);
    assert_eq!(class(ErrorCode::FileAccessFailed), ErrorClass::Unservable);
    assert_eq!(class(ErrorCode::PermissionDenied), ErrorClass::Refused);
    assert_eq!(class(ErrorCode::StaleSnapshot), ErrorClass::Restart);
    assert_eq!(class(ErrorCode::Timeout), ErrorClass::Transient);
    assert_eq!(
        class(ErrorCode::LanguageUnsupported),
        ErrorClass::Unservable
    );
}

/// A row failed after execution (classification of a provider page)
/// carries no execution failure kind: its code decides, as on a row
/// that failed while executing.
#[test]
fn a_row_failed_after_execution_exits_by_its_code() {
    let row = json_fixture(
        r#"{
            "results": [
                {
                    "status": "error",
                    "data": {"errorCode": "classificationProviderError"}
                }
            ]
        }"#,
    );
    assert_eq!(
        tool_outcome(row, None, true).exit_class(),
        ExitClass::Failed(ErrorClass::Transient)
    );
    let refused = json_fixture(
        r#"{
            "results": [{"status": "error", "data": {"errorCode": "invalidInput"}}]
        }"#,
    );
    assert_eq!(
        tool_outcome(refused, None, true).exit_class(),
        ExitClass::Failed(ErrorClass::CorrectInput)
    );
    assert_eq!(
        tool_outcome(json!({}), None, true).exit_class(),
        ExitClass::Failed(ErrorClass::CorrectInput)
    );
}

/// Every row failed and one rejected its input: rejected input (2), as
/// in a batch where other rows answered.
#[test]
fn a_failed_batch_with_a_rejected_row_is_invalid_input() {
    let rows = json_fixture(
        r#"{
            "results": [
                {"status": "error", "data": {"errorCode": "invalidPattern"}},
                {"status": "error", "data": {"errorCode": "notFound"}}
            ]
        }"#,
    );
    for failure in [
        Some(ErrorClass::Unservable),
        Some(ErrorClass::CorrectInput),
        None,
    ] {
        assert_eq!(
            tool_outcome(rows.clone(), failure, true).exit_class(),
            ExitClass::Failed(ErrorClass::CorrectInput),
            "{failure:?}"
        );
    }
}

/// A directory given to a file read (or a file to a listing) is
/// the caller's mistake: exit 2, not an execution failure.
#[test]
fn not_a_file_and_not_a_directory_are_invalid_input() {
    for code in ["notAFile", "notADirectory"] {
        let row = json!({"results":[{"status":"error","data":{"errorCode":code}}]});
        assert!(all_rows_invalid_input(&row), "{code}");
        assert_eq!(
            tool_outcome(row, Some(ErrorClass::Unservable), true).exit_class(),
            ExitClass::Failed(ErrorClass::CorrectInput),
            "{code}"
        );
    }
}

#[test]
fn rows_rejecting_caller_input_are_invalid_input() {
    let rows = json_fixture(
        r#"{
            "results": [
                {"status": "error", "data": {"errorCode": "invalidPattern"}},
                {"status": "error", "data": {"errorCode": "invalidInput"}}
            ]
        }"#,
    );
    assert!(all_rows_invalid_input(&rows));
    let mixed = json_fixture(
        r#"{
            "results": [
                {"status": "error", "data": {"errorCode": "invalidPattern"}},
                {
                    "status": "error",
                    "data": {"errorCode": "fileAccessFailed"}
                }
            ]
        }"#,
    );
    assert!(!all_rows_invalid_input(&mixed));
    assert!(!all_rows_invalid_input(&json_fixture(r#"{"results": []}"#)));
}

#[test]
fn mixed_clasify_rows_follow_worst_row_precedence() {
    let good = json_fixture(
        r#"{
            "index": 0,
            "data": {"answers": {"ok": {"type": "noul", "noul": 0.8}}}
        }"#,
    );
    let bad = |code| json!({"index":1,"status":"error","data":{"errorCode":code,"error":"failed"}});
    for (code, expected) in [
        ("notFound", ExitClass::Failed(ErrorClass::CorrectInput)),
        ("rateLimited", ExitClass::Failed(ErrorClass::Transient)),
        ("authentication", ExitClass::Failed(ErrorClass::Refused)),
        ("invalidInput", ExitClass::Failed(ErrorClass::CorrectInput)),
    ] {
        for rows in [vec![good.clone(), bad(code)], vec![bad(code), good.clone()]] {
            assert_eq!(
                tool_outcome(json!({"results":rows}), None, false).exit_class(),
                expected
            );
        }
    }
    assert_eq!(
        tool_outcome(
            json!({"results":[good,bad("notFound"),bad("invalidInput")]}),
            None,
            false
        )
        .exit_class(),
        ExitClass::Failed(ErrorClass::CorrectInput)
    );
}

/// Batch precedence: any correctInput row exits 2, else refused 4, else
/// restart 6, else unservable 5, else all-transient 7; an answering row
/// never hides a failed one.
#[test]
fn a_batch_exits_by_its_strictest_failed_row() {
    let row =
        |code: &str| json!({"index":0,"status":"error","data":{"errorCode":code,"error":"x"}});
    let ok = json_fixture(r#"{"index": 0, "data": {"files": []}}"#);
    let exit = |codes: &[&str]| {
        let rows: Vec<Value> = codes.iter().map(|code| row(code)).collect();
        tool_outcome(json!({"results":rows}), None, false).exit_class()
    };
    let failed = ExitClass::Failed;
    assert_eq!(
        exit(&[
            "timeout",
            "executionFailed",
            "authentication",
            "staleSnapshot",
            "notFound"
        ]),
        failed(ErrorClass::CorrectInput)
    );
    assert_eq!(
        exit(&[
            "timeout",
            "executionFailed",
            "authentication",
            "staleSnapshot"
        ]),
        failed(ErrorClass::Refused)
    );
    assert_eq!(
        exit(&["timeout", "executionFailed", "staleSnapshot"]),
        failed(ErrorClass::Restart)
    );
    assert_eq!(
        exit(&["timeout", "executionFailed"]),
        failed(ErrorClass::Unservable)
    );
    assert_eq!(
        exit(&["timeout", "rateLimited"]),
        failed(ErrorClass::Transient)
    );
    let mixed = json!({"results":[ok, row("rateLimited")]});
    assert_eq!(
        tool_outcome(mixed, None, false).exit_class(),
        failed(ErrorClass::Transient)
    );
}
