use super::*;
use octocode_test_support::json_fixture;

/// B233 (RV-W3 F4): a securityCheck row that is Ok but partial and
/// retryable (OSV did not answer) offers the same row again as
/// `next.retry`; a complete row offers none.
#[test]
fn a_retryable_partial_security_row_offers_next_retry() {
    let query = json_fixture(
        r#"{
            "operation": "package",
            "ecosystem": "npm",
            "packageName": "x"
        }"#,
    );
    let partial = registry_row(
        ToolId::SecurityCheck,
        &query,
        Ok(json_fixture(r#"{"isPartial": true, "retryable": true}"#)),
    );
    assert_eq!(partial.data["next"]["retry"]["tool"], "securityCheck");
    assert_eq!(
        partial.data["next"]["retry"]["input"]["queries"][0], query,
        "{}",
        partial.data
    );
    let complete = registry_row(
        ToolId::SecurityCheck,
        &query,
        Ok(json_fixture(r#"{"status": "ok"}"#)),
    );
    assert!(complete.data.get("next").is_none(), "{}", complete.data);
}
