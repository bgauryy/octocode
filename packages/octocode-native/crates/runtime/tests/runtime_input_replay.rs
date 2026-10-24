use crate::support::{Run as _, Workspace, row_data};
use octocode_contract_types::ErrorCode;
use serde_json::json;

#[tokio::test]
async fn both_surfaces_reject_invalid_requests_and_execute_the_same_typed_range() {
    let workspace = Workspace::new();
    let file = workspace.write("source.txt", "first\nselected\nlast\n");
    let runtime = workspace.runtime(&[]);
    for input in [
        json!({"path":file}),
        json!({"queries":[{"path":file,"ranges":["2-2"]}]}),
        json!({"queries":[{"path":file,"ranges":[{"startLine":2,"endLine":1}]}]}),
    ] {
        let mcp = runtime
            .run_mcp("invalid".into(), "localFetch".into(), input.clone())
            .await
            .unwrap();
        assert_eq!(mcp["isError"], true, "{mcp}");
        assert_eq!(
            mcp["structuredContent"]["errorCode"], "invalidInput",
            "{mcp}"
        );
        let cli = runtime
            .run("invalid".into(), "localFetch".into(), input)
            .await
            .unwrap_err();
        assert_eq!(cli.code, ErrorCode::InvalidInput);
    }
    let input = json!({"queries":[{"path":file,"ranges":[{"startLine":2,"endLine":2}]}]});
    let cli = runtime
        .run("valid".into(), "localFetch".into(), input.clone())
        .await
        .unwrap();
    let mcp = runtime
        .run_mcp("valid".into(), "localFetch".into(), input)
        .await
        .unwrap();
    let content = row_data(&cli)["content"].as_str().unwrap();
    assert!(content.contains("selected"));
    assert!(!content.contains("first") && !content.contains("last"));
    assert_eq!(
        mcp["structuredContent"]["results"][0]["data"]["content"],
        content
    );
    runtime.close().await;
}
