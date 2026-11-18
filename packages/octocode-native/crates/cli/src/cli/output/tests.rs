use super::*;
use octocode_native::runtime::error::mcp_result;
use octocode_runtime_base::security::Sanitized;
use octocode_test_support::json_fixture;

#[test]
fn a_paged_fixture_is_byte_identical_on_cli_and_mcp() {
    let fixture = json_fixture(
        r#"{
            "results": [{"index": 0, "data": {"files": []}}],
            "responsePagination": {
                "next": {
                    "tool": "localSearch",
                    "input": {"cursor": "stable"}
                },
                "expiresAt": "2026-11-02T00:00:00.000Z",
                "replayed": false
            }
        }"#,
    );
    let outcome = || ToolOutcome {
        structured_content: Sanitized::scrubbed(fixture.clone()),
        failure: Some(ErrorClass::Unservable),
        all_failed: false,
        continued: None,
    };
    let cli = Printed::finish("localSearch", Ok(outcome()), true, &Value::Null, None);
    let mcp = mcp_result("localSearch", Ok(outcome())).expect("MCP result");
    assert_eq!(cli.body.as_bytes(), mcp.text.as_bytes());
}
