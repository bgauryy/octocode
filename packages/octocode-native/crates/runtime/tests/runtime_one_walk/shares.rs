//! A batch of source reads shares one response window: each row's page, as it
//! renders (the line gutter and JSON escapes included), fits the row's share,
//! so the response fits the window without a row split or an oversized page,
//! and walking the batch reads every row exactly as a walk of that row alone.
use super::walk;
use crate::support::Workspace;
use serde_json::{Value, json};

/// The response window a call without `responseChars` pages under.
const WINDOW: usize = octocode_native::id::DEFAULT_RESPONSE_CHARS;
/// The response chars around the `data` of `rows` result rows: the envelope
/// and each row's `{"index":…,"data":…}` wrapper
/// (`response::read_share::row_window`).
fn frame(rows: usize) -> usize {
    let rows = (0..rows)
        .map(|index| json!({"index": index, "data": {}}))
        .collect::<Vec<_>>();
    utf16(&json!({ "results": rows })) - 2 * rows.len()
}

fn utf16(value: &Value) -> usize {
    serde_json::to_string(value)
        .expect("serializes")
        .encode_utf16()
        .count()
}

/// A source whose lines cost more as JSON than as bytes: quotes, backslashes
/// and tabs escape to two characters, and the gutter adds its own.
fn escaped(file: usize, lines: usize) -> String {
    (1..=lines)
        .map(|line| {
            format!(
                "f{file} l{line}\t\"say\": \"a \\\"b\\\" c\"\t{}é\n",
                "\\x".repeat(line % 7)
            )
        })
        .collect()
}

/// The source text of one row's pages, gutters stripped, in walk order.
fn read(pages: &[Value], index: u64) -> String {
    pages
        .iter()
        .flat_map(|page| page["results"].as_array().into_iter().flatten())
        .filter(|row| row["index"] == index)
        .filter_map(|row| row["data"]["content"].as_str())
        .flat_map(|content| content.split_inclusive('\n'))
        .map(|line| line.split_once('\t').expect("numbered").1)
        .collect()
}

#[tokio::test]
async fn each_row_of_a_read_batch_renders_within_its_share_and_reads_as_alone() {
    let workspace = Workspace::new();
    let sources = (0..5).map(|file| escaped(file, 600)).collect::<Vec<_>>();
    for (file, text) in sources.iter().enumerate() {
        workspace.write(&format!("src/f{file}.txt"), text);
    }
    let runtime = workspace.runtime(&[]);
    let queries = (0..5)
        .map(|file| json!({"path": format!("src/f{file}.txt")}))
        .collect::<Vec<_>>();
    let pages = walk(&runtime, "localFetch", json!({ "queries": queries })).await;
    let longest = sources
        .iter()
        .flat_map(|text| text.lines())
        .map(str::len)
        .max()
        .unwrap_or(0);
    assert!(
        longest * 2 < (WINDOW - frame(sources.len())) / sources.len(),
        "no one line outgrows a share"
    );
    for (at, page) in pages.iter().enumerate() {
        assert!(
            page.get("oversized").is_none(),
            "page {at} is oversized: {page}"
        );
        assert!(
            utf16(page) <= WINDOW,
            "page {at} renders {} chars over the {WINDOW}-char window",
            utf16(page)
        );
        // Each row reads with an equal share of the window
        // (`response::read_share::row_window`); the sources are alike, so
        // every row continues on every page, and all of them fit one page.
        let share = (WINDOW - frame(sources.len())) / sources.len();
        assert_eq!(
            page["results"].as_array().map_or(0, Vec::len),
            sources.len(),
            "page {at}: every row's page fits the response"
        );
        for row in page["results"].as_array().into_iter().flatten() {
            assert!(
                row.get("rowPart").is_none(),
                "page {at}: row {} split into parts",
                row["index"]
            );
            assert!(
                utf16(&row["data"]) <= share,
                "page {at}: row {} renders {} chars over its {share}-char share",
                row["index"],
                utf16(&row["data"])
            );
        }
    }
    // Every row reads its source exactly once, as a walk of it alone reads it.
    for (index, text) in sources.iter().enumerate() {
        let alone = walk(
            &runtime,
            "localFetch",
            json!({"queries": [{"path": format!("src/f{index}.txt")}]}),
        )
        .await;
        let index = u64::try_from(index).expect("index");
        assert_eq!(read(&alone, 0), *text, "row {index} alone");
        assert_eq!(read(&pages, index), *text, "row {index} in the batch");
    }
    runtime.close().await;
}
