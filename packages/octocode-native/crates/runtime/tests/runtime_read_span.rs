//! Source reads never elide inside a requested span, and a batch of reads
//! shares the response window so every row returns content.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use crate::support;

use serde_json::{Value, json};
use std::collections::BTreeMap;
use support::Workspace;

const WINDOW: usize = 20_000;

fn numbered_file(tag: &str, lines: usize) -> String {
    (1..=lines)
        .map(|line| format!("{tag} line {line:04} {}\n", "x".repeat(40)))
        .collect()
}

/// `line -> text` of numbered content (`N\t<text>`); markers are skipped.
fn numbered_lines(content: &str, into: &mut BTreeMap<usize, String>) {
    for record in content.lines() {
        let Some((number, text)) = record.split_once('\t') else {
            assert!(record.starts_with("... ["), "unnumbered line {record:?}");
            continue;
        };
        let number: usize = number.parse().expect("line number");
        assert!(
            into.insert(number, text.to_owned()).is_none(),
            "line {number} shown twice"
        );
    }
}

fn row_file(row: &Value) -> &Value {
    &row["data"]
}

/// Follow a row's `next.continue` chain to the end, collecting every line.
async fn walk(
    runtime: &octocode_native::runtime::ToolRuntime,
    tool: &str,
    first: &Value,
) -> BTreeMap<usize, String> {
    let mut lines = BTreeMap::new();
    numbered_lines(row_file(first)["content"].as_str().unwrap(), &mut lines);
    let mut next = row_file(first)["next"]["continue"]["query"].clone();
    let mut hops = 0;
    while next.is_object() {
        hops += 1;
        assert!(hops < 50, "walk terminates");
        let page = runtime
            .execute(
                format!("walk-{hops}"),
                tool.into(),
                json!({"queries":[next.clone()]}),
            )
            .await
            .expect("continuation runs unchanged");
        let row = &page.structured_content["results"][0];
        numbered_lines(row_file(row)["content"].as_str().unwrap(), &mut lines);
        next = row_file(row)["next"]["continue"]["query"].clone();
    }
    lines
}

#[tokio::test]
async fn a_batch_of_reads_shares_the_window_and_every_row_returns_content() {
    let workspace = Workspace::new();
    let files: Vec<String> = (0..3)
        .map(|index| {
            workspace
                .write(
                    &format!("f{index}.txt"),
                    numbered_file(&format!("f{index}"), 400),
                )
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let small = workspace.write("small.txt", "one\ntwo\n");
    let runtime = workspace.runtime(&[("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", WINDOW.to_string())]);
    let mut queries: Vec<Value> = vec![json!({"path": small})];
    queries.extend(
        files
            .iter()
            .map(|path| json!({"path": path, "fullContent": true})),
    );
    let mcp = runtime
        .execute_mcp(
            "share".into(),
            "localFetch".into(),
            json!({"queries": queries}),
        )
        .await
        .expect("batch");
    let envelope = &mcp["structuredContent"];
    assert!(
        envelope.get("responsePagination").is_none(),
        "rows stay on one page: {}",
        envelope["responsePagination"]
    );
    assert!(envelope.to_string().encode_utf16().count() <= WINDOW + 1_000);
    let rows = envelope["results"].as_array().expect("rows");
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0]["data"]["content"], "1\tone\n2\ttwo\n");
    // The incomplete banner leads the envelope and names the partial rows.
    assert_eq!(
        envelope.as_object().unwrap().keys().next().unwrap(),
        "warnings"
    );
    let banner = envelope["warnings"][0].as_str().unwrap();
    assert!(
        banner.starts_with("incomplete — 3 of 4 rows partial (index 1, 2, 3)"),
        "{banner}"
    );
    assert!(banner.contains("next.continue"), "{banner}");
    let text = mcp["content"][0]["text"].as_str().unwrap();
    assert!(
        text.starts_with("warnings:\n- incomplete — 3 of 4"),
        "{text:.200}"
    );
    for (row, path) in rows[1..].iter().zip(&files) {
        let next = &row["data"]["next"]["continue"]["query"]["queries"][0];
        assert!(
            next.get("length").is_none(),
            "continues at the default page: {next}"
        );
        let lines = walk(&runtime, "localFetch", row).await;
        let expected = std::fs::read_to_string(path).unwrap();
        assert_eq!(
            lines.keys().copied().collect::<Vec<_>>(),
            (1..=400).collect::<Vec<_>>()
        );
        assert_eq!(
            lines
                .values()
                .map(|line| format!("{line}\n"))
                .collect::<String>(),
            expected
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_batch_that_fits_is_unchanged_and_carries_no_banner() {
    let workspace = Workspace::new();
    let a = workspace.write("a.txt", numbered_file("a", 20));
    let b = workspace.write("b.txt", numbered_file("b", 20));
    let runtime = workspace.runtime(&[("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", WINDOW.to_string())]);
    let mcp = runtime
        .execute_mcp(
            "fits".into(),
            "localFetch".into(),
            json!({"queries":[{"path":a},{"path":b}]}),
        )
        .await
        .expect("batch");
    let envelope = &mcp["structuredContent"];
    assert!(envelope.get("warnings").is_none(), "{envelope}");
    for row in envelope["results"].as_array().unwrap() {
        assert!(row["data"].get("next").is_none(), "{row}");
        assert_eq!(row["data"]["content"].as_str().unwrap().lines().count(), 20);
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_budget_cut_range_continues_exactly_the_rest_of_the_requested_spans() {
    let workspace = Workspace::new();
    let path = workspace.write("big.txt", numbered_file("big", 2_000));
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .execute(
            "cut".into(),
            "localFetch".into(),
            json!({"queries":[{"path": path, "ranges": ["10-20", "100-1900"]}]}),
        )
        .await
        .expect("read");
    let row = &outcome.structured_content["results"][0];
    let content = row["data"]["content"].as_str().unwrap();
    // One gap marker between the spans; none inside a span.
    assert_eq!(content.matches("... [").count(), 1, "{content:.400}");
    assert!(
        row["data"]["next"]["continue"].is_object(),
        "the page is cut"
    );
    let lines = walk(&runtime, "localFetch", row).await;
    let expected: Vec<usize> = (10..=20).chain(100..=1900).collect();
    assert_eq!(lines.keys().copied().collect::<Vec<_>>(), expected);
    runtime.close().await;
}

const SOURCE: &str = "\
pub fn first(value: usize) -> usize {
    let mut total = 0;
    for step in 0..value {
        total += step;
    }
    total += 1;
    total += 2;
    total += 3;
    total += 4;
    total += 5;
    total += 6;
    let marker = \"needle here\";
    total += 7;
    total += 8;
    total += 9;
    total += 10;
    total += 11;
    total += 12;
    total += 13;
    total
}

pub fn second() -> usize {
    let short = \"other needle\";
    short.len()
}
";

#[tokio::test]
async fn a_match_window_that_cuts_its_declaration_offers_the_rest_as_a_lead() {
    let workspace = Workspace::new();
    let path = workspace.write("src/lib.rs", SOURCE);
    let runtime = workspace.runtime(&[]);
    let outcome = runtime
        .execute(
            "lead".into(),
            "localFetch".into(),
            json!({"queries":[{"path": path, "matchString": "needle here", "contextLines": 2}]}),
        )
        .await
        .expect("read");
    let data = &outcome.structured_content["results"][0]["data"];
    let lead = &data["hints"]["readBlock"];
    assert_eq!(lead["tool"], "localFetch", "{data}");
    // The lead reads only the lines of `first` the window left out.
    let query = &lead["query"]["queries"][0];
    assert_eq!(query["ranges"], json!(["1-9", "15-21"]), "{query}");
    let rest = runtime
        .execute(
            "lead-run".into(),
            "localFetch".into(),
            json!({"queries":[query.clone()]}),
        )
        .await
        .expect("lead runs unchanged");
    let mut lines = BTreeMap::new();
    numbered_lines(data["content"].as_str().unwrap(), &mut lines);
    numbered_lines(
        rest.structured_content["results"][0]["data"]["content"]
            .as_str()
            .unwrap(),
        &mut lines,
    );
    assert_eq!(
        lines.keys().copied().collect::<Vec<_>>(),
        (1..=21).collect::<Vec<_>>()
    );

    // Exact-line reads and windows that already cover the block offer none.
    for query in [
        json!({"path": path, "matchString": "needle here", "contextLines": 0}),
        json!({"path": path, "matchString": "other needle"}),
        json!({"path": path, "matchString": "needle here", "block": true}),
    ] {
        let outcome = runtime
            .execute(
                "no-lead".into(),
                "localFetch".into(),
                json!({"queries":[query.clone()]}),
            )
            .await
            .expect("read");
        let data = &outcome.structured_content["results"][0]["data"];
        assert!(data["hints"].get("readBlock").is_none(), "{query}: {data}");
    }
    runtime.close().await;
}
