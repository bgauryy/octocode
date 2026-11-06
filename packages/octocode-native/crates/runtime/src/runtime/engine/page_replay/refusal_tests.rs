//! A refused cursor names its own cause, true on both surfaces and under
//! either storage mode, with the error code of that cause: a handle that is
//! not issued, or sent to the wrong tool, is the caller's input; any other
//! cause is saved work this runtime cannot answer.
use super::*;
use crate::execution::ExecutionContext;
use crate::id::ToolId;
use crate::response::cursor::issued;
use crate::runtime::engine::cursors::SavedWork;
use crate::runtime::engine::{Channel, ToolRuntime};
use octocode_io::cancellation::Cancellation;
use std::time::{Duration, Instant};

const PERSISTENT: &[(&str, &str)] = &[("OCTOCODE_STORAGE_MODE", "persistent")];

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: Cancellation::new(),
        deadline: Instant::now() + Duration::from_secs(30),
        response_window: None,
        github_credential: None,
    }
}

/// A handle `runtime` retains for `tool`, issued on the `mcp` surface.
fn issue(runtime: &ToolRuntime, tool: ToolId, mcp: bool) -> String {
    let mut pending = runtime
        .cursors
        .pending(page_replay_key("", mcp, &[], &[], None), None);
    let token = pending.insert(
        tool,
        json!({"queries":[{"path":"must-not-be-read"}]}),
        SavedWork::ToolResume { index: Some(0) },
    );
    runtime.cursors.retain(pending, &context()).unwrap();
    token
}

async fn refusal(
    runtime: &ToolRuntime,
    tool: &str,
    cursor: &str,
    channel: Channel,
    code: ErrorCode,
) -> String {
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
    assert_eq!(error.code, code, "{}", error.message);
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
    for cursor in [
        "cursor2.garbage",
        "not-a-cursor",
        "cursor2.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        let message = refusal(
            &runtime,
            "localFetch",
            cursor,
            Channel::Cli,
            ErrorCode::InvalidInput,
        )
        .await;
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
        let message = refusal(
            &persistent,
            "localFetch",
            &unknown,
            channel,
            ErrorCode::ResponseSnapshotExpired,
        )
        .await;
        assert!(
            message.starts_with("Unknown or expired cursor"),
            "{message}"
        );
        assert!(
            message.contains(&format!("{} seconds", CONTINUATION_IDLE.as_secs())),
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
    let message = refusal(
        &memory,
        "localFetch",
        &unknown,
        Channel::Cli,
        ErrorCode::ResponseSnapshotExpired,
    )
    .await;
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
        let message = refusal(
            &runtime,
            "localFetch",
            &token,
            channel,
            ErrorCode::ResponseSnapshotExpired,
        )
        .await;
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
    let message = refusal(
        &runtime,
        "localFetch",
        &token,
        Channel::Cli,
        ErrorCode::InvalidInput,
    )
    .await;
    assert!(message.contains("issued to another tool"), "{message}");
    runtime.close().await;
}
