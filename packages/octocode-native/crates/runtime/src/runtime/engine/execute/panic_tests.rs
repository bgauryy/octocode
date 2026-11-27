//! A panic in the inline admission and shaping code, outside every row, must
//! answer its request: the MCP server runs each request in a task that sends
//! the response only when the handler returns.
use super::*;
use octocode_test_support::json_fixture;
use std::cell::Cell;

thread_local! {
    /// Makes the next `execute_channel` on this thread panic before its rows.
    pub(super) static INJECTED: Cell<bool> = const { Cell::new(false) };
}

#[tokio::test]
async fn a_panic_outside_the_rows_is_answered_as_an_error() {
    let home = tempfile::tempdir().unwrap();
    let runtime = super::super::construction_tests::runtime(home.path(), &[]);
    INJECTED.set(true);
    let result = runtime
        .execute(
            runtime.admit("panic-outside-rows".into()).unwrap(),
            "localSearch".into(),
            json_fixture(r#"{"queries": [{"path": ".", "pattern": "x"}]}"#),
            Channel::Mcp,
        )
        .await;
    INJECTED.set(false);
    let error = result.expect_err("a panicked request is answered with an error");
    assert_eq!(error.code, ErrorCode::ExecutionFailed);
    assert!(
        !error.message.contains("injected panic"),
        "the message of a panic can quote private input: {}",
        error.message
    );
}
