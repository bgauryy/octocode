//! The `responsePagination` block states more by `next` alone and the replay
//! fact only when it is true.

use crate::support::{Run as _, Workspace};
use serde_json::{Value, json};

#[tokio::test]
async fn a_walk_states_no_hasmore_and_replayed_only_when_true() {
    let workspace = Workspace::new();
    let body = (1..=60)
        .map(|line| format!("needle line {line} {}\n", "padding ".repeat(6)))
        .collect::<String>();
    let file = workspace.write("long.txt", body);
    let runtime = workspace.runtime(&[("OCTOCODE_STORAGE_MODE", "persistent".to_owned())]);
    let mut input =
        json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":1500});
    let mut pages: Vec<Value> = Vec::new();
    loop {
        let output = runtime
            .run(format!("page-{}", pages.len()), "localSearch".into(), input)
            .await
            .expect("page")
            .structured_content
            .into_value();
        octocode_runtime_base::contracts::validate_output("localSearch", &output)
            .expect("contract");
        let next = output.pointer("/responsePagination/next/input").cloned();
        pages.push(output);
        match next {
            Some(call) => input = call,
            None => break,
        }
    }
    runtime.close().await;
    assert!(pages.len() > 2, "several pages");
    assert!(
        pages[0]["responsePagination"].get("replayed").is_none(),
        "{}",
        pages[0]
    );
    for page in &pages {
        assert!(
            page["responsePagination"].get("hasMore").is_none(),
            "{page}"
        );
        let replayed = page["responsePagination"].get("replayed");
        assert!(
            replayed.is_none() || replayed == Some(&json!(true)),
            "{page}"
        );
    }
    let last = pages.last().expect("last");
    assert!(last["responsePagination"].get("next").is_none(), "{last}");
    assert_eq!(last["responsePagination"]["replayed"], true, "{last}");
}
