//! A host that keeps no saved work serves display paging by returning the
//! whole result, and never fails a call that asked for no continuation.

use crate::support::Workspace;
use octocode_native::runtime::{Channel, ToolRuntime};
use octocode_test_support::json_fixture;
use serde_json::Value;

/// One call of a command that exits afterwards.
async fn exiting(runtime: &ToolRuntime, input: Value) -> Result<Value, String> {
    let admission = runtime
        .admit("exiting-call".into())
        .expect("admission")
        .requiring_persistent_continuations();
    runtime
        .execute(admission, "localSearch".into(), input, Channel::Cli)
        .await
        .map(|outcome| outcome.structured_content.into_value())
        .map_err(|error| format!("{error:?}"))
}

fn hits(output: &Value) -> usize {
    output["results"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row["data"]["files"].as_array().into_iter().flatten())
        .map(|file| file["matches"].as_array().map_or(0, Vec::len))
        .sum()
}

fn fixture(workspace: &Workspace) {
    for file in 0..8 {
        let body = (1..=10)
            .map(|line| format!("pub fn needle_{file}_{line}() {{}}\n"))
            .collect::<String>();
        workspace.write(&format!("src/f{file}.rs"), body);
    }
}

async fn served_whole(extra: &[(&str, String)]) {
    let workspace = Workspace::new();
    fixture(&workspace);
    let runtime = workspace.runtime(extra);
    let output = exiting(
        &runtime,
        json_fixture(
            r#"{
                "queries": [{"path": "src", "matchString": ["pub fn"], "limit": 80}],
                "responseChars": 1500
            }"#,
        ),
    )
    .await
    .expect("the call is served, not refused for a continuation");
    runtime.close().await;
    assert_eq!(hits(&output), 80, "every hit is in the one result");
    assert!(output.pointer("/next").is_none(), "{output}");
    assert!(
        !output.to_string().contains("\"cursor\""),
        "no cursor is offered: {output}"
    );
    assert!(
        output["warnings"].as_array().is_some_and(|w| !w.is_empty()),
        "the whole result is disclosed: {output}"
    );
}

#[tokio::test]
async fn memory_mode_serves_display_paging_as_one_result() {
    served_whole(&[("OCTOCODE_STORAGE_MODE", "memory".to_owned())]).await;
}

#[tokio::test]
async fn persistent_storage_with_credential_files_allowed_serves_one_result() {
    served_whole(&[
        ("OCTOCODE_STORAGE_MODE", "persistent".to_owned()),
        ("OCTOCODE_CREDENTIAL_FILES", "allow".to_owned()),
    ])
    .await;
}
