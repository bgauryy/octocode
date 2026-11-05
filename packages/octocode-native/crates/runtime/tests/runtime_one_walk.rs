//! One loop reads everything a request asked for: run `responsePagination.next`,
//! else the `next.nextPage` a whole response carries, until neither remains.
//! Rows, split or not, batched or not, never need a second handle.
use crate::support::{Run as _, Workspace};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

const STORAGE: [(&str, &str); 1] = [("OCTOCODE_STORAGE_MODE", "persistent")];

fn persistent() -> Vec<(&'static str, String)> {
    STORAGE
        .iter()
        .map(|(key, value)| (*key, (*value).to_owned()))
        .collect()
}

/// Every issued `next.nextPage` under `value`.
fn row_walks(value: &Value, found: &mut Vec<Value>) {
    match value {
        Value::Object(object) => {
            if let Some(call) = object.get("next").and_then(|next| next.get("nextPage")) {
                found.push(call.clone());
            }
            object.values().for_each(|child| row_walks(child, found));
        }
        Value::Array(items) => items.iter().for_each(|child| row_walks(child, found)),
        _ => {}
    }
}

/// The one call a reader runs next, and the assertion that there is only one
/// way on: a page with a response-level call states no row walk beside it.
fn the_next_call(output: &Value) -> Option<Value> {
    let mut walks = Vec::new();
    row_walks(&output["results"], &mut walks);
    match output.pointer("/responsePagination/next").cloned() {
        Some(call) => {
            assert_eq!(output["responsePagination"]["hasMore"], true, "{output}");
            assert!(walks.is_empty(), "two ways on: {output}");
            Some(call["input"].clone())
        }
        None => {
            assert_ne!(
                output.pointer("/responsePagination/hasMore"),
                Some(&json!(true)),
                "hasMore with no call: {output}"
            );
            assert!(walks.len() <= 1, "several row walks: {output}");
            assert!(
                walks.is_empty() || output.get("responsePagination").is_none(),
                "a page counter beside a row walk: {output}"
            );
            walks.pop().map(|call| call["input"].clone())
        }
    }
}

/// `(index, line)` of every hit on one page, each read once.
fn collect_hits(output: &Value, seen: &mut BTreeSet<(u64, String, u64)>) {
    for row in output["results"].as_array().into_iter().flatten() {
        let index = row["index"].as_u64().expect("index");
        for file in row["data"]["files"].as_array().into_iter().flatten() {
            for hit in file["matches"].as_array().into_iter().flatten() {
                let at = (
                    index,
                    file["path"].as_str().unwrap_or_default().to_owned(),
                    hit["line"].as_u64().expect("line"),
                );
                assert!(seen.insert(at.clone()), "read twice: {at:?}");
            }
        }
    }
}

/// Walk `start` with the one loop; returns every page's output.
async fn walk(
    runtime: &octocode_native::runtime::ToolRuntime,
    tool: &str,
    start: Value,
) -> Vec<Value> {
    let mut pages = Vec::new();
    let mut input = start;
    loop {
        assert!(pages.len() < 200, "the walk terminates");
        let output: Value = (*runtime
            .run(format!("walk-{}", pages.len()), tool.into(), input)
            .await
            .unwrap_or_else(|error| panic!("page {}: {error:?}", pages.len()))
            .structured_content)
            .clone();
        octocode_native::contracts::validate_output(tool, &output).expect("output contract");
        let next = the_next_call(&output);
        pages.push(output);
        match next {
            Some(call) => input = call,
            None => return pages,
        }
    }
}

fn numbered(file: usize, lines: usize) -> String {
    (1..=lines)
        .map(|line| format!("needle file {file} line {line} {}\n", "padding ".repeat(4)))
        .collect()
}

#[tokio::test]
async fn the_final_part_of_a_split_row_carries_the_rows_next_page() {
    let workspace = Workspace::new();
    for file in 0..6 {
        workspace.write(&format!("docs/f{file}.txt"), numbered(file, 25));
    }
    let runtime = workspace.runtime(&[]);
    let pages = walk(
        &runtime,
        "localSearch",
        json!({"queries":[{"path":"docs","matchString":["needle"],"view":"content","limit":40}],"responseChars":1500}),
    )
    .await;
    let mut seen = BTreeSet::new();
    pages.iter().for_each(|page| collect_hits(page, &mut seen));
    assert_eq!(seen.len(), 150, "every hit once");
    assert!(
        pages
            .iter()
            .any(|page| page["results"][0]["rowPart"]["part"].as_u64() > Some(1)),
        "a split row"
    );
    for page in &pages {
        let block = &page["responsePagination"];
        assert!(block.get("page").is_none() && block.get("totalPages").is_none());
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_whole_read_over_the_automatic_page_says_it_reads_on() {
    let workspace = Workspace::new();
    let body = (1..=2000)
        .map(|line| format!("line {line} {}\n", "text \"quoted\" ".repeat(6)))
        .collect::<String>();
    workspace.write("long.md", &body);
    let runtime = workspace.runtime(&[]);
    let pages = walk(
        &runtime,
        "localFetch",
        json!({"queries":[{"path":"long.md"}],"responseChars":20000}),
    )
    .await;
    assert!(pages.len() > 1, "more than one page");
    assert_eq!(pages[0]["responsePagination"]["hasMore"], true);
    let read = pages
        .iter()
        .flat_map(|page| page["results"].as_array().into_iter().flatten())
        .filter_map(|row| row["data"]["content"].as_str())
        .flat_map(str::lines)
        .map(|line| line.split_once('\t').expect("numbered").1)
        .collect::<Vec<_>>();
    assert_eq!(read, body.lines().collect::<Vec<_>>());
    runtime.close().await;
}

#[tokio::test]
async fn a_batch_walks_every_row_on_one_handle_under_its_own_index() {
    let workspace = Workspace::new();
    workspace.write("a/one.txt", numbered(1, 30));
    workspace.write("b/two.txt", numbered(2, 12));
    workspace.write("c/three.txt", numbered(3, 50));
    let runtime = workspace.runtime(&[]);
    let row = |path: &str| json!({"path":path,"matchString":["needle"],"limit":5});
    let pages = walk(
        &runtime,
        "localSearch",
        json!({"queries":[row("a"),{"path":"a","bogus":true},row("b"),row("c")]}),
    )
    .await;
    assert!(pages.len() > 2, "several hops: {}", pages.len());
    let mut seen = BTreeSet::new();
    pages.iter().for_each(|page| collect_hits(page, &mut seen));
    let lines = |index: u64, file: &str| {
        seen.iter()
            .filter(|(at, path, _)| *at == index && path.ends_with(file))
            .count()
    };
    assert_eq!(lines(0, "one.txt"), 30, "row 0 whole");
    assert_eq!(lines(2, "two.txt"), 12, "row 2 keeps its index");
    assert_eq!(lines(3, "three.txt"), 50, "row 3 keeps its index");
    assert_eq!(seen.len(), 92, "no other hit");
    let rejected = pages
        .iter()
        .flat_map(|page| page["results"].as_array().into_iter().flatten())
        .filter(|row| row["status"] == "error")
        .count();
    assert_eq!(rejected, 1, "the rejected row shows once");
    runtime.close().await;
}

#[tokio::test]
async fn a_batch_that_fits_states_one_call_and_no_page_counters() {
    let workspace = Workspace::new();
    workspace.write("a/one.txt", numbered(1, 8));
    workspace.write("b/two.txt", numbered(2, 8));
    let runtime = workspace.runtime(&[]);
    let row = |path: &str| json!({"path":path,"matchString":["needle"],"limit":5});
    let first = runtime
        .run(
            "fits".into(),
            "localSearch".into(),
            json!({"queries":[row("a"),row("b")]}),
        )
        .await
        .expect("batch")
        .structured_content;
    let block = first["responsePagination"].as_object().expect("the block");
    assert_eq!(block["hasMore"], true);
    assert!(
        block.get("expiresAt").is_none(),
        "no saved response: {first}"
    );
    let mut walks = Vec::new();
    row_walks(&first["results"], &mut walks);
    assert!(walks.is_empty(), "{first}");
    for row in first["results"].as_array().expect("rows") {
        assert_eq!(row["data"]["pagination"]["hasMore"], true, "{row}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_batch_where_one_row_has_more_keeps_that_rows_own_lead() {
    let workspace = Workspace::new();
    workspace.write("a/one.txt", numbered(1, 8));
    workspace.write("b/two.txt", numbered(2, 2));
    let runtime = workspace.runtime(&[]);
    let row = |path: &str| json!({"path":path,"matchString":["needle"],"limit":5});
    let first = runtime
        .run(
            "one-lead".into(),
            "localSearch".into(),
            json!({"queries":[row("a"),row("b")]}),
        )
        .await
        .expect("batch")
        .structured_content;
    assert!(first.get("responsePagination").is_none(), "{first}");
    assert!(first["results"][0]["data"]["next"]["nextPage"].is_object());
    runtime.close().await;
}

#[tokio::test]
async fn one_handle_is_the_same_handle_on_both_reads_of_one_answer() {
    let workspace = Workspace::new();
    workspace.write("a/one.txt", numbered(1, 8));
    workspace.write("b/two.txt", numbered(2, 8));
    let runtime = workspace.runtime(&[]);
    let row = |path: &str| json!({"path":path,"matchString":["needle"],"limit":5});
    let input = json!({"queries":[row("a"),row("b")]});
    let mut handles = Vec::new();
    for call in 0..2 {
        let output = runtime
            .run(format!("same-{call}"), "localSearch".into(), input.clone())
            .await
            .expect("batch")
            .structured_content;
        handles.push(output["responsePagination"]["next"]["input"].clone());
    }
    assert_eq!(handles[0], handles[1]);
    assert!(handles[0]["cursor"].is_string());
    runtime.close().await;
}

/// The pages of a batch's hop re-execute under the rows' original indices when
/// the saved response was evicted.
#[tokio::test]
async fn an_evicted_page_of_a_batch_hop_keeps_its_rows_indices() {
    let workspace = Workspace::new();
    workspace.write("a/one.txt", numbered(1, 30));
    workspace.write("b/two.txt", numbered(2, 30));
    let row = |path: &str| json!({"path":path,"matchString":["needle"],"limit":12});
    let pages_dir = workspace.home.join("tmp/response-pages");
    let mut input =
        json!({"queries":[{"path":"a","bogus":true},row("a"),row("b")],"responseChars":900});
    let mut seen = BTreeSet::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls < 200, "the walk terminates");
        let runtime = workspace.runtime(&persistent());
        let output = runtime
            .run(format!("evict-{calls}"), "localSearch".into(), input)
            .await
            .unwrap_or_else(|error| panic!("call {calls}: {error:?}"))
            .structured_content;
        runtime.close().await;
        collect_hits(&output, &mut seen);
        evict(&pages_dir);
        match the_next_call(&output) {
            Some(call) => input = call,
            None => break,
        }
    }
    let of = |index: u64| seen.iter().filter(|(at, _, _)| *at == index).count();
    assert_eq!(
        (of(1), of(2)),
        (30, 30),
        "both rows whole under their index"
    );
}

fn evict(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        std::fs::remove_file(entry.path()).expect("evict entry");
    }
}
