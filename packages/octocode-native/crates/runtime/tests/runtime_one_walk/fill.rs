//! A line page fills the response window it is given. The transport walk's
//! source (1,500 short Unicode lines at `responseChars: 900`) pages so that
//! each page stops only where its next line would not fit the window, never
//! splits a row, and every line arrives once, in order.
use super::walk;
use crate::support::Workspace;
use serde_json::{Value, json};

const WINDOW: usize = 900;

fn utf16(value: &Value) -> usize {
    serde_json::to_string(value)
        .expect("serializes")
        .encode_utf16()
        .count()
}

/// Response chars one source line takes as a page counts it: its JSON text
/// and the widest gutter (`<line>\t`, the tab escaped) of a source of `total`
/// lines.
fn counted(line: &str, total: usize) -> usize {
    let text = serde_json::to_string(line).expect("serializes");
    text.encode_utf16().count() - 2 + total.to_string().len() + 2
}

#[tokio::test]
async fn a_line_page_fills_its_window_up_to_the_line_that_does_not_fit() {
    let workspace = Workspace::new();
    let total = 1500;
    let body = (1..=total)
        .map(|line| format!("line {line}: immutable 界😀 evidence\n"))
        .collect::<String>();
    let lines = body.split_inclusive('\n').collect::<Vec<_>>();
    workspace.write("source.txt", &body);
    let runtime = workspace.runtime(&super::persistent());
    let pages = walk(
        &runtime,
        "localFetch",
        json!({"queries":[{"path":"source.txt"}],"responseChars":WINDOW}),
    )
    .await;
    let mut read = Vec::new();
    let (mut least, mut most) = (usize::MAX, 0);
    for (at, page) in pages.iter().enumerate() {
        let chars = utf16(page);
        assert!(chars <= WINDOW, "page {at}: {chars} chars: {page}");
        assert!(page.get("oversized").is_none(), "page {at}: {page}");
        let row = &page["results"][0];
        assert!(row.get("rowPart").is_none(), "page {at} splits its row");
        let content = row["data"]["content"].as_str().expect("content");
        // The page as the reader counts it: each line at the widest gutter,
        // and the row as a cursor's page shows it. The first response to the
        // fresh request drops the `path` that echoes its query after the
        // reader measured the row, so it keeps that room unused; a page read
        // through a cursor keeps its `path` and fills its window exactly.
        let mut shown = 0;
        let mut counted_chars = chars;
        if at == 0 {
            assert!(row["data"].get("path").is_none(), "{page}");
            counted_chars += utf16(&json!({"path":"source.txt"})) - 1;
        } else {
            assert_eq!(row["data"]["path"], "source.txt", "{page}");
        }
        for numbered in content.split_inclusive('\n') {
            let (number, text) = numbered.split_once('\t').expect("numbered");
            counted_chars += total.to_string().len() - number.len();
            read.push(text.to_owned());
            shown += 1;
        }
        if at + 1 < pages.len() {
            least = least.min(chars);
            most = most.max(chars);
            let next = lines[read.len()];
            assert!(
                counted_chars + counted(next, total) > WINDOW,
                "page {at} stops with room for its next line: {chars} chars ({counted_chars} counted), {shown} lines, next {} chars, window {WINDOW}",
                counted(next, total)
            );
        }
    }
    assert_eq!(
        read,
        body.lines()
            .map(|line| format!("{line}\n"))
            .collect::<Vec<_>>()
    );
    eprintln!(
        "fill: {} pages, inner pages {least}..={most} chars of {WINDOW}",
        pages.len()
    );
    runtime.close().await;
}
