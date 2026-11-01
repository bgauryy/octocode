//! A refused cursor names its own cause, true on both surfaces and under
//! either storage mode, and keeps its error code.
use super::*;
use crate::execution::ExecutionContext;
use crate::id::ToolId;
use crate::response::cursor::issued;
use crate::runtime::engine::cursors::{Pending, SavedWork};
use crate::runtime::engine::{Channel, ToolRuntime};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

const PERSISTENT: &[(&str, &str)] = &[("OCTOCODE_STORAGE_MODE", "persistent")];

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: CancellationToken::new(),
        deadline: Instant::now() + Duration::from_secs(30),
        response_window: None,
        github_credential: None,
    }
}

/// A handle `runtime` retains for `tool`, issued on the `mcp` surface.
fn issue(runtime: &ToolRuntime, tool: ToolId, mcp: bool) -> String {
    let mut pending = Pending::new(page_replay_key("", mcp, &[], &[], None), None);
    let token = pending.insert(
        tool,
        json!({"queries":[{"path":"must-not-be-read"}]}),
        SavedWork::ToolResume { index: Some(0) },
    );
    runtime.cursors.retain(pending, &context()).unwrap();
    token
}

async fn refusal(runtime: &ToolRuntime, tool: &str, cursor: &str, channel: Channel) -> String {
    let result = runtime
        .execute(
            runtime
                .admit(format!("refusal-{cursor}-{channel:?}"))
                .unwrap(),
            tool.into(),
            json!({"cursor": cursor}),
            channel,
        )
        .await;
    let Err(error) = result else {
        panic!("{tool} must refuse {cursor}");
    };
    assert_eq!(error.code, ErrorCode::ResponseSnapshotExpired);
    assert!(
        error.message.contains("No source or provider was called")
            && error.message.contains("original request without cursor"),
        "{}",
        error.message
    );
    error.message
}

#[tokio::test]
async fn a_malformed_cursor_is_named_malformed() {
    let home = tempfile::tempdir().unwrap();
    let runtime = super::super::construction_tests::runtime(home.path(), PERSISTENT);
    for cursor in ["cursor2.garbage", "not-a-cursor"] {
        let message = refusal(&runtime, "localFetch", cursor, Channel::Cli).await;
        assert!(message.starts_with("Malformed cursor"), "{message}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn an_unknown_cursor_names_expiry_and_the_actual_storage_mode() {
    let unknown = issued::placeholder();
    let home = tempfile::tempdir().unwrap();
    let persistent = super::super::construction_tests::runtime(home.path(), PERSISTENT);
    for channel in [Channel::Cli, Channel::Mcp] {
        let message = refusal(&persistent, "localFetch", &unknown, channel).await;
        assert!(
            message.starts_with("Unknown or expired cursor"),
            "{message}"
        );
        assert!(
            message.contains(&format!("{} seconds", RETENTION_IDLE.as_secs())),
            "{message}"
        );
        assert!(!message.contains("memory"), "{message}");
    }
    persistent.close().await;
    let home = tempfile::tempdir().unwrap();
    let memory = super::super::construction_tests::runtime(
        home.path(),
        &[("OCTOCODE_STORAGE_MODE", "memory")],
    );
    let message = refusal(&memory, "localFetch", &unknown, Channel::Cli).await;
    assert!(
        message.starts_with("Unknown or expired cursor"),
        "{message}"
    );
    assert!(message.contains("storage.mode is memory"), "{message}");
    memory.close().await;
}

#[tokio::test]
async fn a_cursor_from_the_other_surface_names_the_issuing_surface() {
    let home = tempfile::tempdir().unwrap();
    let runtime = super::super::construction_tests::runtime(home.path(), PERSISTENT);
    for (issued_on_mcp, channel, issuer) in [
        (false, Channel::Mcp, "issued by the CLI"),
        (true, Channel::Cli, "issued by an MCP server"),
    ] {
        let token = issue(&runtime, ToolId::LocalFetch, issued_on_mcp);
        let message = refusal(&runtime, "localFetch", &token, channel).await;
        assert!(message.contains(issuer), "{message}");
        assert!(!message.contains("memory"), "{message}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_cursor_sent_to_another_tool_names_the_tool_mismatch() {
    let home = tempfile::tempdir().unwrap();
    let runtime = super::super::construction_tests::runtime(home.path(), PERSISTENT);
    let token = issue(&runtime, ToolId::LocalSearch, false);
    let message = refusal(&runtime, "localFetch", &token, Channel::Cli).await;
    assert!(message.contains("issued to another tool"), "{message}");
    runtime.close().await;
}
