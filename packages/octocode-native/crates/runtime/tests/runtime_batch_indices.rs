//! A batch with a rejected row keeps every row's input index across
//! response pages. The continuation carries only the admitted rows (a
//! rejected row would fail the output contract); the rejected row rides the
//! first page and the snapshot mark keeps the admitted rows' indices.

use crate::support::{Run as _, Workspace};
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
    // Text windows and row pages alike. Each read names a window wider than
    // its share of the page (a whole-file read would fit its share and page
    // by its own continuation), so the response itself pages.
    let read = |path: &std::path::Path| {
        brief(json!({"path":path,"offset":0,"length":20_000,"unit":"bytes"}))
    };
    for scope in [None, Some("rows")] {
        let mut input = json!({
            "queries":[
                read(&a),
                brief(json!({"path":a,"bogusField":1})),
                read(&c)
            ],
            "responseLength": 6000
        });
        if let Some(scope) = scope {
            input["responseScope"] = json!(scope);
        }
        let first = runtime
            .run("r7-1".into(), "localFetch".into(), input)
            .await
            .unwrap();
        let page = &first.structured_content;
        let next = page
            .pointer("/responsePagination/next/query")
            .cloned()
            .unwrap_or_else(|| panic!("a second page: {}", page["responsePagination"]));
        assert!(
            next.get("queries").is_none(),
            "saved pages do not repeat input rows: {next}"
        );
        assert!(next.to_string().find("bogusField").is_none(), "{next}");
        let first_rows = indices(page);
        assert!(
            first_rows.contains(&1),
            "the rejected row rides page 1: {page}"
        );
        let mut later = Vec::new();
        let mut next = Some(next);
        let mut pages = 1;
        while let Some(query) = next.take() {
            pages += 1;
            assert!(pages < 20, "page walk ends");
            let out = runtime
                .run(format!("r7-{pages}"), "localFetch".into(), query)
                .await
                .unwrap();
            let page = &out.structured_content;
            assert!(
                page["responsePagination"].get("restart").is_none(),
                "the continuation reproduces the paged body: {page}"
            );
            later.extend(indices(page));
            next = page.pointer("/responsePagination/next/query").cloned();
        }
        assert!(pages >= 2, "the fixture spans several pages");
        assert!(later.contains(&2), "the last row keeps index 2: {later:?}");
        assert!(
            later.iter().all(|index| [0, 2].contains(index)),
            "later pages show the admitted rows at their input indices: {later:?}"
        );
        let mut seen = [first_rows, later].concat();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, [0, 1, 2], "indices stay the input's across pages");
    }
    runtime.close().await;
}

/// A rejected row is published like every other row whether or not its
/// batch pages: its hints are the contract's strings (never the internal
/// call map), and page 1 with the rejected row passes the output contract.
#[tokio::test]
async fn a_rejected_row_on_a_paged_batch_is_published_and_validated() {
    let workspace = Workspace::new();
    let body = (0..400)
        .map(|line| format!("line {line} of the batch fixture\n"))
        .collect::<String>();
    let a = workspace.write("a.txt", &body);
    let c = workspace.write("c.txt", &body);
    let runtime = workspace.runtime(&[]);
    let row = |path: &std::path::Path, extra: Value| {
        let mut row = json!({"path": path, "mainGoal": "Rejected row publishing."});
        for (key, value) in extra.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        row
    };
    let window = json!({"offset":0,"length":20_000,"unit":"bytes"});
    for (scope, length) in [(None, None), (None, Some(6000)), (Some("rows"), Some(6000))] {
        let mut input = json!({"queries":[
            row(&a, window.clone()),
            row(&a, json!({"bogusField":1})),
            row(&c, window.clone())
        ]});
        if let Some(length) = length {
            input["responseLength"] = json!(length);
        }
        if let Some(scope) = scope {
            input["responseScope"] = json!(scope);
        }
        let outcome = runtime
            .run("r7-publish".into(), "localFetch".into(), input)
            .await
            .unwrap();
        let page = &outcome.structured_content;
        let label = format!("scope {scope:?}, length {length:?}");
        if length.is_some() {
            assert_eq!(
                page["responsePagination"]["hasMore"], true,
                "{label}: the fixture pages: {page}"
            );
        }
        let rejected = page["results"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|row| row["index"] == 1)
            .unwrap_or_else(|| panic!("{label}: the rejected row rides page 1: {page}"));
        let hints = rejected["data"]["hints"]
            .as_array()
            .unwrap_or_else(|| panic!("{label}: published string hints: {rejected}"));
        assert!(
            !hints.is_empty() && hints.iter().all(Value::is_string),
            "{label}: {rejected}"
        );
        octocode_native::contracts::validate_output("localFetch", page)
            .unwrap_or_else(|error| panic!("{label}: page 1 output contract: {error:?}\n{page}"));
    }
    runtime.close().await;
}
