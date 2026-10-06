//! R7: a batch with a rejected row keeps every row's input index across
//! response pages: the response continuation replays the whole input, so
//! the rejected row is rejected again at its index instead of dropping out
//! and shifting the rows after it.
#![allow(clippy::panic, clippy::unwrap_used)]

use crate::support::Workspace;
use serde_json::{Value, json};

/// Row indices a page shows: structured rows, or `result: N` headers of a
/// text-paged window.
fn indices(page: &Value) -> Vec<u64> {
    let rows = page["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row["index"].as_u64());
    let text = page["responseWindow"]
        .as_str()
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.strip_prefix("result: ")?.trim().parse().ok());
    rows.chain(text).collect()
}

#[tokio::test]
#[ignore = "R7: blocked by the output contract (a rejected row cannot ride responsePagination.next); needs a core decision"]
async fn a_rejected_row_keeps_later_row_indices_on_the_next_page() {
    let workspace = Workspace::new();
    let body = (0..400)
        .map(|line| format!("line {line} of the batch fixture\n"))
        .collect::<String>();
    let a = workspace.write("a.txt", &body);
    let c = workspace.write("c.txt", &body);
    let runtime = workspace.runtime(&[]);
    let brief = |mut row: Value| {
        row["mainGoal"] = json!("Batch index replay.");
        row
    };
    let input = json!({
        "queries":[
            brief(json!({"path":a,"fullContent":true})),
            brief(json!({"path":a,"bogusField":1})),
            brief(json!({"path":c,"fullContent":true}))
        ],
        "responseLength": 6000
    });
    let first = runtime
        .execute("r7-1".into(), "localFetch".into(), input)
        .await
        .unwrap();
    let page = &first.structured_content;
    let next = page
        .pointer("/responsePagination/next/query")
        .cloned()
        .unwrap_or_else(|| panic!("a second page: {}", page["responsePagination"]));
    assert_eq!(
        next["queries"].as_array().map(Vec::len),
        Some(3),
        "the continuation replays every input row: {next}"
    );
    assert_eq!(next["queries"][1]["bogusField"], 1, "{next}");
    let mut seen = indices(page);
    let mut next = Some(next);
    let mut pages = 1;
    while let Some(query) = next.take() {
        pages += 1;
        assert!(pages < 20, "page walk ends");
        let out = runtime
            .execute(format!("r7-{pages}"), "localFetch".into(), query)
            .await
            .unwrap();
        let page = &out.structured_content;
        seen.extend(indices(page));
        next = page.pointer("/responsePagination/next/query").cloned();
    }
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, [0, 1, 2], "indices stay the input's across pages");
    runtime.close().await;
}
