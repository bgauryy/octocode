//! A batch of source reads shares one response window: each row's page, as it
//! renders (the line gutter and JSON escapes included), fits the row's share,
//! so the response fits the window without a row split or an oversized page,
//! and walking the batch reads every row exactly as a walk of that row alone.
//! The rows fill the window like water (`response::read_share::widened`): a
//! row that finishes under its share leaves the rest to the rows still
//! reading, and those rows continue in one call, so a batch takes no more
//! calls than its rows read one by one.
use super::walk;
use crate::support::Workspace;
use serde_json::{Value, json};

/// The response window a call without `responseChars` pages under.
const WINDOW: usize = octocode_runtime_base::id::DEFAULT_RESPONSE_CHARS;
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
async fn the_rows_of_a_read_batch_fit_the_window_they_share_and_read_as_alone() {
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
        // The rows read with an equal share of the window first
        // (`response::read_share::row_window`), and a row that finishes
        // under it leaves the rest to the rows still reading: together they
        // fit the room the envelope leaves. The rows still reading continue
        // in one call, which reads them all again.
        let rows_here = page["results"].as_array().map_or(0, Vec::len);
        if at == 0 {
            assert_eq!(
                rows_here,
                sources.len(),
                "page {at}: every row's page fits the response"
            );
        } else {
            assert!(rows_here >= 1, "page {at}: a continuation reads a row");
        }
        let rows = page["results"].as_array().into_iter().flatten();
        let shown = rows.clone().map(|row| utf16(&row["data"])).sum::<usize>();
        assert!(
            shown <= WINDOW - frame(rows_here),
            "page {at}: its rows render {shown} chars over the room they share"
        );
        for row in rows {
            assert!(
                row.get("rowPart").is_none(),
                "page {at}: row {} split into parts",
                row["index"]
            );
        }
        assert!(
            super::the_next_calls(page).len() <= 1,
            "page {at}: the rows still reading continue in one call: {page}"
        );
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

/// Lines of about 45 chars, numbered by file.
fn lines(file: usize, count: usize) -> String {
    (1..=count)
        .map(|line| format!("file {file} line {line}: a row of plain source text\n"))
        .collect()
}

/// Hoist X8: a batch of reads took more calls than its rows read one by
/// one, because each row kept an equal share of the window and continued by
/// its own call. Now a short row leaves its share to a whole file, and the
/// rows still reading continue in one call that fills its page again: a
/// whole file beside a 10-line range is one call, and whole files batched
/// take no more calls than one by one. Every row still reads as alone.
#[tokio::test]
async fn a_read_batch_takes_no_more_calls_than_its_rows_read_one_by_one() {
    let workspace = Workspace::new();
    // (lines per file) in one batch; in the first, the second row reads lines 1-10.
    let mixes: [&[usize]; 5] = [
        &[360, 274],
        &[330, 250, 250],
        &[420, 90, 210, 300, 140],
        &[1_100, 40, 60],
        &[440, 440],
    ];
    let runtime = workspace.runtime(&[]);
    for (mix, sizes) in mixes.iter().enumerate() {
        let paths = sizes
            .iter()
            .enumerate()
            .map(|(file, count)| {
                let path = format!("mix{mix}/f{file}.txt");
                workspace.write(&path, lines(file, *count));
                path
            })
            .collect::<Vec<_>>();
        let mut queries = paths
            .iter()
            .map(|path| json!({"path": path}))
            .collect::<Vec<_>>();
        if mix == 0 {
            queries[1]["ranges"] = json!([{"startLine": 1, "endLine": 10}]);
        }
        let batch = walk(&runtime, "localFetch", json!({ "queries": queries })).await;
        let mut alone = 0;
        for (index, query) in queries.iter().enumerate() {
            let own = walk(&runtime, "localFetch", json!({ "queries": [query] })).await;
            alone += own.len();
            let index = u64::try_from(index).expect("index");
            assert_eq!(read(&batch, index), read(&own, 0), "mix {mix} row {index}");
        }
        assert!(
            batch.len() <= alone,
            "mix {mix}: the batch took {} calls, its rows one by one {alone}",
            batch.len()
        );
        if mix == 0 {
            assert_eq!(batch.len(), 1, "a whole file beside a range: one call");
        }
        eprintln!(
            "mix {mix}: batched {} calls, one by one {alone}",
            batch.len()
        );
    }
    runtime.close().await;
}
