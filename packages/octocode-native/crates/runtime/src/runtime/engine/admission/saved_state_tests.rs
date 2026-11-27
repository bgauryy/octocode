//! A cursor restores the runtime's own private progress (a selection plan,
//! the files a walk listed, a provider's page cursor) into the row it
//! resumes. That state is no caller's text: whatever its encoding happens to
//! spell, the resumed call runs. The caller's own fields of the same row, a
//! private field a fresh call sends, and the handle itself keep the gate
//! every caller's text passes.
use super::*;
use crate::runtime::engine::Channel;
use crate::runtime::engine::cursors::SavedWork;
use crate::runtime::engine::page_replay::page_replay_key;
use octocode_io::cancellation::Cancellation;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::response::cursor::issued;
use octocode_runtime_base::security::ContentSecurity;
use std::time::{Duration, Instant};

/// A fake GitHub token: `ghp_` and 36 alphanumerics.
const TOKEN: &str = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: Cancellation::new(),
        deadline: Some(Instant::now() + Duration::from_secs(30)),
        response_window: None,
        github_credential: None,
    }
}

/// The fresh-call gate refuses `value` as a caller's search term.
fn refused_as_caller_text(value: &str) -> bool {
    ContentSecurity::new()
        .check_input(json!({"matchString": [value]}), InputPolicy::Query)
        .is_err_and(|issues| {
            issues
                .iter()
                .any(|issue| issue.rule_id == "security.credential")
        })
}

/// A cursor `runtime` retains for `tool`, resuming `input`.
fn issue(runtime: &ToolRuntime, tool: ToolId, input: Value) -> String {
    let mut pending = runtime
        .cursors
        .pending(page_replay_key("", &[], &[], None), None);
    let token = pending.insert(tool, input, SavedWork::ToolResume { index: Some(0) });
    runtime
        .cursors
        .retain(pending, &context())
        .expect("retained");
    token
}

/// A fresh ghSearchCode call of one `row`.
async fn fresh(runtime: &ToolRuntime, id: &str, row: Value) -> Result<Value, RuntimeError> {
    runtime
        .execute(
            runtime.admit(id.into()).expect("admitted"),
            "ghSearchCode".into(),
            json!({"queries": [row]}),
            Channel::Mcp,
        )
        .await
        .map(|outcome| outcome.structured_content.into_value())
}

async fn resume(runtime: &ToolRuntime, tool: &str, cursor: &str) -> Result<Value, RuntimeError> {
    runtime
        .execute(
            runtime.admit(format!("saved-{cursor}")).expect("admitted"),
            tool.into(),
            json!({"cursor": cursor}),
            Channel::Mcp,
        )
        .await
        .map(|outcome| outcome.structured_content.into_value())
}

fn workspace(home: &std::path::Path) -> String {
    let root = home.join("src");
    std::fs::create_dir_all(&root).expect("workspace");
    std::fs::write(root.join("a.txt"), "needle\n").expect("file");
    root.to_string_lossy().into_owned()
}

/// The caller's own fields of a resumed row pass the gate every caller's
/// text passes.
#[tokio::test]
async fn a_resumed_rows_caller_fields_are_still_refused() {
    let home = tempfile::tempdir().expect("home");
    let runtime = super::super::construction_tests::runtime(home.path(), &[]);
    let root = workspace(home.path());
    let cursor = issue(
        &runtime,
        ToolId::LocalSearch,
        json!({"queries":[{"path": root, "matchString": [TOKEN]}]}),
    );
    let error = resume(&runtime, "localSearch", &cursor)
        .await
        .expect_err("a credential in a caller's field is refused");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert!(error.message.contains("credential"), "{}", error.message);
    runtime.close().await;
}

/// Private progress a fresh call sends is the caller's: a forged walk state
/// is refused, however it is spelled, and so is the same text in a
/// caller's field when it is longer than a caller's string may be.
#[tokio::test]
async fn private_progress_from_a_caller_is_refused() {
    let home = tempfile::tempdir().expect("home");
    let runtime = super::super::construction_tests::runtime(home.path(), &[]);
    let listed = "0123456789abcdef".repeat(700);
    let forged = fresh(
        &runtime,
        "forged",
        json!({"owner":"o","repo":"r","keywords":["x"],"listed": listed}),
    )
    .await
    .expect_err("a forged walk state is refused");
    assert_eq!(forged.code, ErrorCode::InvalidInput);
    assert!(forged.message.contains("listed"), "{}", forged.message);
    let oversized = fresh(
        &runtime,
        "oversized",
        json!({"owner":"o","repo":"r","keywords":[listed]}),
    )
    .await
    .expect_err("an oversized caller string is refused");
    assert_eq!(oversized.code, ErrorCode::InvalidInput);
    assert!(
        oversized.message.contains("maximum length"),
        "{}",
        oversized.message
    );
    runtime.close().await;
}

/// The handle itself is never judged as a caller's text: one that is well
/// formed and spells a GitHub token is a cursor this runtime did not issue,
/// not a credential.
#[tokio::test]
async fn a_handle_that_spells_a_credential_is_an_unknown_cursor() {
    // Each id ends in a character whose unused low bits are zero, as a
    // canonical 16-byte id's encoding does.
    let spelled = issued::format("ghp_a1B2c3D4e5F6g7H8iA", "J0k1L2m3N4o5P6q7R8s9tA");
    assert!(issued::parts(&spelled).is_some(), "{spelled}");
    assert!(refused_as_caller_text(&spelled), "{spelled}");
    let home = tempfile::tempdir().expect("home");
    let runtime = super::super::construction_tests::runtime(home.path(), &[]);
    let error = resume(&runtime, "localSearch", &spelled)
        .await
        .expect_err("no work is saved under it");
    assert_eq!(
        error.code,
        ErrorCode::ResponseSnapshotExpired,
        "{}",
        error.message
    );
    assert!(!error.message.contains("credential"), "{}", error.message);
    runtime.close().await;
}
