use super::*;
use crate::id::ToolId;
use octocode_test_support::json_fixture;
use serde_json::json;

#[test]
fn every_tool_keeps_complete_row_data_without_hoisting_or_pruning() {
    let evidence = json_fixture(
        r#"{
            "path": "/workspace/a",
            "content": "exact\n😀",
            "facts": [1, 2, 3],
            "nested": {
                "kind": "related",
                "tool": "provider-text",
                "input": {"evidence": true}
            }
        }"#,
    );
    for tool in ToolId::ALL {
        let row = result_row(3, evidence.clone(), None);
        assert_eq!(row, json!({"index":3,"data":evidence}));
        assert_eq!(envelope(tool, vec![row.clone()]), json!({"results":[row]}));
    }
}

#[test]
fn partial_diagnostics_keep_their_reasons_and_full_guidance() {
    let mut row = json_fixture(
        r#"{
            "data": {"hints": ["existing"], "partialReasons": ["filesSkipped"]}
        }"#,
    );
    let diagnostics = crate::response::result::ToolDiagnostics {
        partial_reasons: vec![crate::response::paging::PartialReason::ProviderResultCap],
        hints: vec!["complete guidance ".repeat(100)],
        ..Default::default()
    };
    attach_diagnostics(&mut row, diagnostics);
    let data = &row["data"];
    assert_eq!(data["isPartial"], true);
    assert_eq!(
        data["partialReasons"],
        json_fixture(r#"["filesSkipped", "providerResultCap"]"#)
    );
    assert_eq!(data["hints"].as_array().expect("hints").len(), 2);
    assert_eq!(data["hints"][1], "complete guidance ".repeat(100));
}

#[test]
fn explicit_common_status_consumes_only_its_equal_producer_copy() {
    let evidence = json_fixture(r#"{"status": "empty", "provider": {"status": "empty"}}"#);
    let row = result_row(0, evidence.clone(), Some("empty"));
    assert_eq!(
        row,
        json_fixture(
            r#"{
                "index": 0,
                "status": "empty",
                "data": {"provider": {"status": "empty"}}
            }"#
        )
    );
    for status in [None, Some("complete")] {
        let row = result_row(0, evidence.clone(), status);
        assert_eq!(row["data"], evidence);
    }
}

/// One error shape: whichever order a tool built `error` and `errorCode` in,
/// the row states them first and in the same order, the rest unchanged.
#[test]
fn every_error_row_leads_with_its_code_then_its_message() {
    let rest = json_fixture(r#"{"httpStatus": 404, "hints": ["check the name"]}"#);
    for built in [
        json_fixture(
            r#"{
                "error": "gone",
                "errorCode": "notFound",
                "httpStatus": 404,
                "hints": ["check the name"]
            }"#,
        ),
        json_fixture(
            r#"{
                "httpStatus": 404,
                "errorCode": "notFound",
                "hints": ["check the name"],
                "error": "gone"
            }"#,
        ),
    ] {
        let row = result_row(0, built, Some("error"));
        let keys = row["data"]
            .as_object()
            .expect("data")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_eq!(keys[..3], ERROR_LEAD);
        for (name, value) in rest.as_object().expect("rest") {
            assert_eq!(row["data"][name], *value);
        }
        assert_eq!(keys.len(), 5);
    }
}

/// Every declared code gets its class on the row; `retryable` is a boolean
/// exactly when the class is transient (false only for a cancelled call).
#[test]
fn the_row_finaliser_states_the_class_and_retryable_only_when_transient() {
    use crate::id::error_codes::{self, ErrorClass};
    for &code in error_codes::ALL {
        for stale in [None, Some(json!(true)), Some(json!(false))] {
            let mut data = json!({"errorCode": code, "error": "x"});
            if let Some(stale) = stale {
                data["retryable"] = stale;
            }
            let row = result_row(0, data, Some("error"));
            let class = error_codes::class(code);
            assert_eq!(row["data"]["errorClass"], json!(class), "{code}");
            if class == ErrorClass::Transient {
                assert!(row["data"]["retryable"].is_boolean(), "{code}");
            } else {
                assert!(row["data"].get("retryable").is_none(), "{code}");
            }
        }
    }
    let cancelled = result_row(
        0,
        json_fixture(r#"{"errorCode": "cancelled", "error": "x"}"#),
        Some("error"),
    );
    assert_eq!(cancelled["data"]["retryable"], json!(false));
    let timeout = result_row(
        0,
        json_fixture(r#"{"errorCode": "timeout", "error": "x"}"#),
        Some("error"),
    );
    assert_eq!(timeout["data"]["retryable"], json!(true));
}

/// An error row that offers a `related` call states no shared hint beside
/// it, whatever its code: the error says what failed and the call is what to
/// run. The same row without a call gets the code's shared recovery.
#[test]
fn an_error_row_with_a_related_call_gets_no_shared_hint() {
    let tool = &crate::response::output::SharedOutput;
    let query = json_fixture(r#"{"owner": "o", "repo": "r", "path": "src/missing.rs"}"#);
    let call = json_fixture(
        r#"{"tool": "ghStructure", "input": {"queries": [{"operation": "tree", "owner": "o", "repo": "r", "path": "src"}]}}"#,
    );
    for code in [
        "notFound",
        "fileAccessFailed",
        "serverUnavailable",
        "timeout",
    ] {
        let mut led = json!({"errorCode": code, "error": "x", "related": [call]});
        attach_error_hint(tool, &mut led, &query);
        assert_eq!(led.get("hints"), None, "{led}");
        let mut bare = json!({"errorCode": code, "error": "x"});
        attach_error_hint(tool, &mut bare, &query);
        assert_eq!(
            bare["hints"].as_array().map(Vec::len),
            Some(1),
            "one shared hint: {bare}"
        );
    }
}

/// A rate-limited row states the wait only when nothing on it says when the
/// limit resets or what to run instead.
#[test]
fn a_rate_limit_hint_appears_only_when_the_row_says_nothing_else() {
    let tool = &crate::response::output::SharedOutput;
    let query = json!({});
    for (row, hinted) in [
        (json!({"errorCode": "rateLimited", "error": "x"}), true),
        (
            json!({"errorCode": "rateLimited", "error": "x", "rateLimit": {"retryAfterSeconds": 30}}),
            false,
        ),
        (
            json!({"errorCode": "rateLimited", "error": "x", "rateLimit": {"resetEpochSeconds": 1}}),
            false,
        ),
        (
            json!({"errorCode": "rateLimited", "error": "x", "next": {"readFile": {"tool": "ghGetFileContent", "input": {}}}}),
            false,
        ),
    ] {
        let mut data = row.clone();
        attach_error_hint(tool, &mut data, &query);
        assert_eq!(data.get("hints").is_some(), hinted, "{row} -> {data}");
    }
}
