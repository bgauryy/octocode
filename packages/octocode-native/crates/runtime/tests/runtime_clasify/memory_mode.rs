//! A command that exits after one call keeps no saved work. A classified
//! result still arrives, and its leads that need saved work stay out with a
//! warning, instead of the call failing after the provider billed it.

use crate::support::{self, Workspace};
use octocode_native::runtime::Channel;
use serde_json::json;

/// Storage that keeps nothing for a later call.
fn memory() -> [(&'static str, String); 1] {
    [("OCTOCODE_STORAGE_MODE", "memory".to_owned())]
}

/// Memory mode still refuses what it cannot deliver: an ordinary page of a
/// read needs its cursor. No classification, so no provider request.
#[tokio::test]
async fn an_ordinary_paged_read_in_memory_mode_still_refuses() {
    let server = super::provider(0).await;
    let workspace = Workspace::new();
    let long = "needle line\n".repeat(4000);
    let file = workspace.write("long.txt", long);
    let runtime = support::clasify_runtime(&workspace, &server, &memory());
    let admission = runtime
        .admit("exiting-call".into())
        .expect("admission")
        .requiring_persistent_continuations();
    let result = runtime
        .execute(
            admission,
            "localSearch".into(),
            json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":2000}),
            Channel::Cli,
        )
        .await;
    assert!(result.is_err(), "a paged read needs saved work");
    assert!(crate::runtime_clasify::sent(&server).await.is_empty());
    runtime.close().await;
}
