//! A shown (redacted) name that several entries share is named by localFetch
//! with the parent listing as its next call, as the other tools name it.
use crate::support::{Workspace, call, query_path};
use serde_json::json;

const TOKEN: &str = "ghp_aB3dE5gH7jK9mN1pQ3sT5vW7yZ9bD1fH3jK5";
const OTHER: &str = "ghp_zY9xW7vU5tS3rQ1pN9mL7kJ5hG3fD1bA9zX7";

#[tokio::test]
async fn the_ambiguity_answer_carries_the_parent_listing() {
    let workspace = Workspace::new();
    workspace.write(&format!("deploy-{}.txt", TOKEN), "one\n");
    workspace.write(&format!("deploy-{}.txt", OTHER), "two\n");
    let shown = workspace
        .workspace
        .join("deploy-[REDACTED-GITHUBTOKENS].txt");
    let runtime = workspace.runtime(&[]);
    let read = call(&runtime, "localFetch", query_path(&shown, json!({})))
        .await
        .expect("localFetch");
    let text = read.structured_content.to_string();
    assert!(text.contains("several entries show that name"), "{text}");
    assert!(
        text.contains("structureSearch"),
        "the parent listing is offered: {text}"
    );
    assert!(!text.contains(TOKEN) && !text.contains(OTHER), "{text}");
    runtime.close().await;
}
