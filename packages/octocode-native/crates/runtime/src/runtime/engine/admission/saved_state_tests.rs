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
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use octocode_io::cancellation::Cancellation;
use octocode_runtime_base::execution::ExecutionContext;
use octocode_runtime_base::response::cursor::issued;
use octocode_runtime_base::security::ContentSecurity;
use std::time::{Duration, Instant};

/// A fake GitHub token: `ghp_` and 36 alphanumerics.
const TOKEN: &str = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";

/// Bytes whose URL-safe base64, from a 3-byte boundary, spells `ghp_` and
/// 40 alphanumerics, and which a JSON string holds as written (valid UTF-8,
/// no control character, quote or backslash).
const SPELLS_A_TOKEN: &str =
    "*\u{e019f}\u{79a9}8Q\u{7b5}<e\u{737}\u{125}?E\u{2eec};}\u{71e}^6Ox\u{154}";

fn context() -> ExecutionContext {
    ExecutionContext {
        cancellation: Cancellation::new(),
        deadline: Some(Instant::now() + Duration::from_secs(30)),
        response_window: None,
        github_credential: None,
    }
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut low, mut high) = (1u32, 0u32);
    for &byte in bytes {
        low = (low + u32::from(byte)) % 65_521;
        high = (high + low) % 65_521;
    }
    (high << 16) | low
}

/// `json` as a zlib stream of one stored block, so its bytes stand at a
/// known place in the stream: after the 2-byte header and the 5-byte block
/// header.
fn stored_zlib(json: &[u8]) -> Vec<u8> {
    let length = u16::try_from(json.len()).expect("one stored block");
    let mut stream = vec![0x78, 0x01, 0x01];
    stream.extend(length.to_le_bytes());
    stream.extend((!length).to_le_bytes());
    stream.extend(json);
    stream.extend(adler32(json).to_be_bytes());
    stream
}

/// A selection replay token in the form the runtime issues (a plan's JSON,
/// zlib-compressed, in URL-safe base64) whose plan places
/// [`SPELLS_A_TOKEN`] on a 3-byte boundary of the stream, so the token's
/// text holds a GitHub token's shape.
fn selection_token_spelling_a_credential() -> String {
    let plan = |binding: &str| {
        serde_json::to_vec(&json!({
            "version": octocode_tools_shared::selection::plan::VERSION,
            "binding": binding,
            "status": "partial", "scope": "selectedView", "unit": "hits",
            "selected": [], "excluded": [], "total": 1,
            "reason": "candidateBudget", "checksum": "0".repeat(64),
        }))
        .expect("plan json")
    };
    let at = plan(SPELLS_A_TOKEN)
        .windows(SPELLS_A_TOKEN.len())
        .position(|bytes| bytes == SPELLS_A_TOKEN.as_bytes())
        .expect("binding in the plan");
    let pad = (3 - (7 + at) % 3) % 3;
    let binding = format!("{}{SPELLS_A_TOKEN}", "x".repeat(pad));
    let token = URL_SAFE_NO_PAD.encode(stored_zlib(&plan(&binding)));
    assert!(
        octocode_tools_shared::selection::plan::decode(&token).is_some(),
        "the token opens as a selection plan"
    );
    token
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

/// A selection token the runtime issued is its own state: a token whose
/// encoding happens to spell a GitHub token runs from its cursor (here as
/// a replay the source no longer matches), while the same text from a
/// caller is refused.
#[tokio::test]
async fn an_issued_selection_token_that_spells_a_credential_runs_from_its_cursor() {
    let token = selection_token_spelling_a_credential();
    assert!(
        refused_as_caller_text(&token),
        "the token spells a credential"
    );
    let home = tempfile::tempdir().expect("home");
    let runtime = super::super::construction_tests::runtime(
        home.path(),
        &[("OCTOCODE_CLASSIFICATION_API", "fixture-key")],
    );
    let root = workspace(home.path());
    let cursor = issue(
        &runtime,
        ToolId::LocalSearch,
        json!({"queries":[{"path": root, "matchString": ["needle"], "selection": token}]}),
    );
    let output = resume(&runtime, "localSearch", &cursor)
        .await
        .unwrap_or_else(|error| panic!("the resumed call was refused: {error:?}"));
    assert!(
        !output.to_string().contains("credential"),
        "the resumed row was judged as a caller's text: {output}"
    );
    runtime.close().await;
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
