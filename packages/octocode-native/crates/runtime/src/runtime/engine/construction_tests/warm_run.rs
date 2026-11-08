use super::*;
use crate::execution::ExecutionContext;
use crate::security::CheckedQuery;

/// `run_warm` run for real: the warm-up it starts panics (blocking on the
/// runtime that already runs this test), and the call neither dies nor leaves
/// a row waiting on the memo.
#[tokio::test(flavor = "multi_thread")]
async fn a_panicking_warm_up_is_contained_and_releases_the_rows() {
    let home = tempfile::tempdir().expect("home");
    let runtime = runtime(home.path(), &[("GITHUB_TOKEN", "fixture-token")]);
    let dispatcher = runtime.dispatcher();
    let queries = [CheckedQuery::checked(
        serde_json::json!({"owner": "a", "repo": "b", "path": "f.txt", "ref": "0123456789abcdef0123456789abcdef01234567"}),
    )];
    let warm = dispatcher
        .plan_warm(crate::id::ToolId::GhGetFileContent, &queries)
        .expect("a file read has a warm-up");
    let context = ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: None,
        response_window: None,
        github_credential: None,
    };
    dispatcher.run_warm(&warm, &context);
    let released = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        warm.memo().resolved_ref("a", "b", None),
    )
    .await
    .expect("the row is released");
    assert!(released.is_none());
}
