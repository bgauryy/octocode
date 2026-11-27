use octocode_runtime_base::response::outcome::ToolOutcome;

use super::*;
use octocode_runtime_base::response::result::ErrorClass;
use octocode_runtime_base::response::stage::{isolate_output_rows, response_all_failed};
use octocode_test_support::json_fixture;

fn mcp_from_rows(tool: &str, mut structured: Value) -> Value {
    let isolated = isolate_output_rows(tool, &mut structured).expect("row isolation");
    let all_failed = response_all_failed(&structured);
    let result = super::super::error::mcp_result(
        tool,
        Ok(ToolOutcome {
            structured_content: octocode_runtime_base::security::Sanitized::sealed(structured),
            failure: isolated.then_some(ErrorClass::Unservable),
            all_failed,
            continued: None,
        }),
    )
    .expect("MCP response");
    json!({"content":[{"type":"text","text":result.text}],"isError":result.is_error})
}

fn payload(result: &Value) -> Value {
    serde_json::from_str(result["content"][0]["text"].as_str().expect("text")).expect("JSON")
}

#[test]
fn malformed_single_row_is_an_error_in_both_mcp_channels() {
    let result = mcp_from_rows(
        "ghCloneRepo",
        json_fixture(r#"{"results": [{"index": 0, "data": {"owner": "a"}}]}"#),
    );
    assert_eq!(result["isError"], true);
    assert_eq!(
        payload(&result)["results"][0]["data"]["errorCode"],
        "outputContractViolation"
    );
    assert!(
        result["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("outputContractViolation")),
        "{result}"
    );
}

#[test]
fn malformed_row_in_mixed_batch_keeps_healthy_row_and_mcp_success_flag() {
    let result = mcp_from_rows(
        "ghCloneRepo",
        json_fixture(
            r#"{
            "results": [
                {"index": 0, "data": {"owner": "a"}},
                {
                    "index": 1,
                    "data": {
                        "owner": "a",
                        "repo": "b",
                        "bytes": 0,
                        "location": {"path": "/tmp/repo", "commitSha": "abc", "ref": "main"}
                    }
                }
            ]
        }"#,
        ),
    );
    assert_eq!(result["isError"], false);
    let payload = payload(&result);
    assert_eq!(
        payload["results"][0]["data"]["errorCode"],
        "outputContractViolation"
    );
    assert_eq!(
        payload["results"][1]["data"]["location"]["commitSha"],
        "abc"
    );
    assert!(
        result["content"][0]["text"]
            .as_str()
            .is_some_and(|text| text.contains("outputContractViolation")),
        "{result}"
    );
}

#[test]
fn projected_and_compact_repository_archive_states_survive_mcp_output_validation() {
    let active = json!({"repo": "active", "archived": false});
    let archived = json!({"repo": "archived", "archived": true});
    let compact = json!({"repo": "active", "defaultBranch": "main"});
    let output = json!({"results": [
        {"index": 0, "data": {"repositories": [active]}},
        {"index": 1, "data": {"repositories": [archived]}},
        {"index": 2, "data": {"repositories": [active, archived]}},
        {"index": 3, "data": {"repositories": [compact]}},
    ]});

    let result = mcp_from_rows("ghSearchRepo", output.clone());

    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(payload(&result), output);
}
