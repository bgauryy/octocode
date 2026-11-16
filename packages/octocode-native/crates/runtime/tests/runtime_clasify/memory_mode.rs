//! A command that exits after one call keeps no saved work. A classified
//! result still arrives, and its leads that need saved work stay out with a
//! warning, instead of the call failing after the provider billed it.

use super::hits::{DENSE, PREDICATE, kept_hit_lines, provider};
use crate::support::{self, Workspace};
use octocode_native::runtime::{Channel, ToolRuntime};
use serde_json::{Value, json};

/// Storage that keeps nothing for a later call.
fn memory() -> [(&'static str, String); 1] {
    [("OCTOCODE_STORAGE_MODE", "memory".to_owned())]
}

/// One call of a command that exits afterwards.
async fn exiting(runtime: &ToolRuntime, tool: &str, row: Value) -> Result<Value, String> {
    let admission = runtime
        .admit("exiting-call".into())
        .expect("admission")
        .requiring_persistent_continuations();
    runtime
        .execute(
            admission,
            tool.into(),
            json!({"queries":[row]}),
            Channel::Cli,
        )
        .await
        .map(|outcome| outcome.structured_content.into_value())
        .map_err(|error| format!("{error:?}"))
}

#[tokio::test]
async fn a_classified_search_that_left_hits_out_is_delivered_in_memory_mode() {
    let server = provider().await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let runtime = support::clasify_runtime(&workspace, &server, &memory());
    let output = exiting(
        &runtime,
        "localSearch",
        json!({"path":file,"matchString":["needle"],
            "classification":{"predicates":[PREDICATE]}}),
    )
    .await
    .expect("delivered, not failed after billing");
    support::checks::assert_output_contract("localSearch", &output);
    let row = &output["results"][0];
    assert_ne!(row["status"], "error", "{output:#}");
    let data = &row["data"];
    assert_eq!(kept_hit_lines(data), [1, 5, 9], "{data}");
    assert_eq!(data["selection"]["excludedItems"], 3, "{data}");
    // One provider request judged the page.
    let sent = crate::runtime_clasify::sent(&server).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    // The whole read goes on as a fresh request beside the row: no cursor,
    // no saved work, and no classification to bill.
    assert!(data["next"].get("unfiltered").is_none(), "{data}");
    let read = data["related"]
        .as_array()
        .and_then(|related| related.iter().find(|call| call["tool"] == "localSearch"))
        .unwrap_or_else(|| panic!("a related whole read: {data}"));
    assert_eq!(read["kind"], "related", "{data}");
    assert!(data["warnings"].to_string().contains("related[]"), "{data}");
    assert!(
        read["input"]["queries"][0].get("classification").is_none(),
        "{read}"
    );
    assert!(!output.to_string().contains("\"cursor\""), "{output:#}");
    runtime.close().await;
}

/// The same call with a runtime that keeps saved work answers as before.
#[tokio::test]
async fn the_same_search_in_a_live_runtime_keeps_a_saved_unfiltered_lead() {
    let server = provider().await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    let row = support::run(
        &runtime,
        "localSearch",
        json!({"path":file,"matchString":["needle"],
            "classification":{"predicates":[PREDICATE]}}),
    )
    .await;
    let unfiltered = &row.data()["next"]["unfiltered"];
    assert_eq!(unfiltered["kind"], "continue", "{}", row.data());
    assert!(unfiltered["input"]["cursor"].is_string(), "{unfiltered}");
    runtime.close().await;
}

/// A fresh query identical to a judged one reads the judgments saved for it:
/// no second provider request (docs/OCTOCODE_CLASIFY.md states this).
#[tokio::test]
async fn an_identical_fresh_inline_query_replays_its_judgments_for_free() {
    let server = provider().await;
    let workspace = Workspace::new();
    let file = workspace.write("dense.txt", DENSE);
    let runtime = support::clasify_runtime(&workspace, &server, &[]);
    let query = json!({"path":file,"matchString":["needle"],
        "classification":{"predicates":[PREDICATE]}});
    let first = support::run(&runtime, "localSearch", query.clone()).await;
    let second = support::run(&runtime, "localSearch", query).await;
    assert_eq!(first.data()["selection"]["usage"]["calls"], 1);
    assert_eq!(
        second.data()["selection"]["usage"]["calls"],
        0,
        "{}",
        second.data()
    );
    assert_eq!(second.data()["selection"]["usage"]["replayed"], true);
    assert_eq!(kept_hit_lines(first.data()), kept_hit_lines(second.data()));
    let sent = crate::runtime_clasify::sent(&server).await;
    assert_eq!(sent.len(), 1, "{sent:?}");
    runtime.close().await;
}

/// Memory mode still refuses what it cannot deliver: an ordinary page of a
/// read needs its cursor. No classification, so no provider request.
#[tokio::test]
async fn an_ordinary_paged_read_in_memory_mode_still_refuses() {
    let server = provider().await;
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
