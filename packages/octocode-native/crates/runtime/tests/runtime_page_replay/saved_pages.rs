//! Saved pages bind the complete request and security boundary; lost storage never silently reacquires evidence.

use super::lines;
use crate::support::{self, Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

async fn saved_fixture(
    workspace: &Workspace,
    runtime: &octocode_native::runtime::ToolRuntime,
) -> (Value, Value) {
    let file = workspace.write("saved-partitions.txt", lines("captured"));
    let query = json!({"path":file,"matchString":["needle"]});
    let first = runtime
        .run(
            "save".into(),
            "localSearch".into(),
            json!({"queries":[query],"responseChars":800}),
        )
        .await
        .expect("save");
    let next = first.structured_content["responsePagination"]["next"]["input"].clone();
    assert!(next.is_object(), "{}", first.structured_content);
    support::assert_cursor_only(&next);
    octocode_native::contracts::validate("localSearch", next.clone())
        .expect("canonical continuation");
    (query, next)
}

#[tokio::test]
async fn saved_pages_reject_mixed_input_tool_credential_config_and_workspace_changes() {
    use octocode_native::runtime::{Channel, ToolRuntime};
    let workspace = Workspace::new();
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let runtime = workspace.runtime(&env);
    let (query, next) = saved_fixture(&workspace, &runtime).await;
    for changed in [
        query.clone(),
        json!({"path":"/changed","matchString":["new question"]}),
    ] {
        let mut mixed = next.clone();
        mixed["queries"] = json!([changed]);
        let error = runtime
            .run("mixed".into(), "localSearch".into(), mixed)
            .await
            .expect_err("mixed input");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
    let error = runtime
        .run("tool".into(), "localFetch".into(), next.clone())
        .await
        .expect_err("wrong tool");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    let admission = runtime
        .admit_with(
            "credential".into(),
            Some(json!({"githubToken":"different-request-token"})),
        )
        .expect("admit");
    let error = runtime
        .execute(admission, "localSearch".into(), next.clone(), Channel::Cli)
        .await
        .expect_err("credential boundary");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    let admission = runtime.admit("surface".into()).expect("admit");
    let error = runtime
        .execute(admission, "localSearch".into(), next.clone(), Channel::Mcp)
        .await
        .expect_err("surface boundary");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    for changed in ["cwd", "config"] {
        let mut input = workspace.config(&env);
        if changed == "cwd" {
            input.cwd = workspace.workspace.join("subdirectory");
            std::fs::create_dir_all(&input.cwd).expect("cwd");
        } else {
            input
                .env
                .insert("OCTOCODE_ENABLE_STATS".into(), "true".into());
        }
        let other = ToolRuntime::new(input).expect("other runtime");
        let error = other
            .run(changed.into(), "localSearch".into(), next.clone())
            .await
            .expect_err("partition");
        assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
        other.close().await;
    }
    runtime.close().await;
}

#[tokio::test]
async fn expired_disk_snapshot_does_not_reexecute() {
    let workspace = Workspace::new();
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let runtime = workspace.runtime(&env);
    let (_, next) = saved_fixture(&workspace, &runtime).await;
    runtime.close().await;
    let dir = workspace.home.join("tmp/response-pages");
    // An entry file is its header line, then the value.
    for file in std::fs::read_dir(dir).expect("snapshot store").flatten() {
        if file.path().extension().is_some_and(|ext| ext == "entry") {
            let bytes = std::fs::read(file.path()).expect("entry");
            let end = bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .expect("header");
            let mut header: Value = serde_json::from_slice(&bytes[..end]).expect("header json");
            header["storedAtUnix"] = json!(1);
            let mut aged = header.to_string().into_bytes();
            aged.extend_from_slice(&bytes[end..]);
            std::fs::write(file.path(), aged).expect("age entry");
        }
    }
    let runtime = workspace.runtime(&env);
    let error = runtime
        .run("expired".into(), "localSearch".into(), next)
        .await
        .expect_err("expired snapshot");
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    runtime.close().await;
}

#[tokio::test]
async fn failed_snapshot_retention_returns_complete_result_without_display_next() {
    let workspace = Workspace::new();
    std::fs::create_dir_all(workspace.home.join("tmp")).expect("cache parent");
    std::fs::write(workspace.home.join("tmp/response-pages"), "block directory")
        .expect("blocked cache");
    let runtime = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", "persistent".into())]);
    let file = workspace.write("saved.txt", lines("all-evidence"));
    let response = runtime
        .run(
            "fallback".into(),
            "localSearch".into(),
            json!({
                "queries":[{"path":file,"matchString":["needle"]}],
                "responseChars":800
            }),
        )
        .await
        .expect("complete fallback");
    let output = response.structured_content;
    assert!(output.get("responsePagination").is_none(), "{output}");
    assert!(
        output["warnings"].to_string().contains("complete result"),
        "{output}"
    );
    assert_eq!(
        output["results"][0]["data"]["files"][0]["matches"]
            .as_array()
            .expect("complete match array")
            .len(),
        200
    );
    runtime.close().await;
}

#[tokio::test]
async fn tampered_and_unissued_handles_never_execute() {
    use base64::Engine as _;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let (_, next) = saved_fixture(&workspace, &runtime).await;
    let mut token = next["cursor"].as_str().unwrap().to_owned();
    let last = token.pop().unwrap();
    token.push(if last == 'A' { 'Q' } else { 'A' });
    let encoded = token.strip_prefix("cursor2.").expect("current format");
    assert_eq!(encoded.len(), 44);
    let (ids, remainder) = encoded.as_bytes().as_chunks::<22>();
    assert!(remainder.is_empty());
    for id in ids {
        assert_eq!(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(id)
                .expect("tampered ID remains canonical")
                .len(),
            16
        );
    }
    for input in [
        json!({"cursor":token}),
        json!({"cursor":format!("cursor2.{}","A".repeat(44))}),
        json!({"cursor":format!("cursor1.{}","0".repeat(64))}),
    ] {
        let error = runtime
            .run("unissued".into(), "localSearch".into(), input)
            .await
            .expect_err("registry lookup");
        assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    }
    runtime.close().await;
}
