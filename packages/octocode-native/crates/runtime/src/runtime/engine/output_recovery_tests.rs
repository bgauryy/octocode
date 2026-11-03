use crate::response::outcome::ToolOutcome;

use super::*;
use crate::response::result::FailureKind;
use crate::response::stage::{isolate_output_rows, response_all_failed};

fn mcp_from_rows(mut structured: Value) -> Value {
    assert!(isolate_output_rows("ghCloneRepo", &mut structured).expect("row isolation"));
    let all_failed = response_all_failed(&structured);
    let result = super::super::error::mcp_result(
        "ghCloneRepo",
        Ok(ToolOutcome {
            structured_content: crate::security::Sanitized::sealed(structured),
            failure: Some(FailureKind::Execution),
            all_failed,
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
    let result = mcp_from_rows(json!({"results":[
        {"index":0,"data":{"owner":"a"}}
    ]}));
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
    let result = mcp_from_rows(json!({"results":[
        {"index":0,"data":{"owner":"a"}},
        {"index":1,"data":{
            "owner":"a","repo":"b","bytes":0,
            "location":{
                "kind":"repo","localPath":"/tmp/repo",
                "commitSha":"abc",
                "resolvedRef":"main"
            }
        }}
    ]}));
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
