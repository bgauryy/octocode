//! The match stream walked through the runtime's response pager, as a reader
//! walks it: each response page first (`responsePagination.next`, row parts
//! joined), then the row's `next.nextPage`. Pages count matches, not files:
//! each query page starts where the last ended, the walk ends at the stated
//! `total` matches and reads each once, and a match larger than a page arrives in row parts that join
//! back to its exact source text.

use super::call;
use crate::support::Workspace;
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Files of small functions: more matches than files.
const FILES: usize = 30;
const FUNCTIONS_PER_FILE: usize = 3;

/// What one walk read: every match's value by `path line`, the query pages
/// it called, where the last one ended, the `total` its pages stated, and how
/// many parts continued a split value.
#[derive(Default)]
struct Read {
    values: BTreeMap<String, String>,
    query_pages: u64,
    /// Where the last query page read starts.
    page_offset: u64,
    /// Matches read when the walk ended.
    end: u64,
    total_items: Option<u64>,
    continued: usize,
}

/// Joins one response page's rows into `read`: a part whose `rowPart.text`
/// names a string continues the first match it repeats, whose value is
/// appended to what earlier parts gave; every other match is new.
fn join(read: &mut Read, row: &Value, mcp: bool) {
    let mut continues = row["rowPart"].get("text").is_some();
    read.continued += usize::from(continues);
    for file in row["data"]["files"].as_array().into_iter().flatten() {
        let path = file["path"].as_str().expect("file path");
        for hit in file["matches"].as_array().into_iter().flatten() {
            let key = format!("{path} {}", hit["line"]);
            let value = hit["value"].as_str().expect("match value");
            if std::mem::take(&mut continues) {
                read.values
                    .get_mut(&key)
                    .unwrap_or_else(|| panic!("mcp={mcp}: {key} continues nothing"))
                    .push_str(value);
            } else {
                let first = read.values.insert(key.clone(), value.to_owned());
                assert!(first.is_none(), "mcp={mcp}: {key} read twice");
            }
        }
    }
}

/// Walks `first` under `responseChars`: response pages first, then the
/// query's next page, until neither remains.
async fn walk(runtime: &ToolRuntime, mcp: bool, first: Value, chars: u64) -> Read {
    let mut read = Read::default();
    let mut input = json!({"queries": [first], "responseChars": chars});
    for calls in 1.. {
        assert!(calls < 2000, "mcp={mcp}: the walk terminates");
        let envelope = call(runtime, mcp, calls, input.clone()).await;
        let row = &envelope["results"][0];
        if let Some(pagination) = row["data"].get("pagination") {
            let offset = pagination["offset"].as_u64().expect("query offset");
            // Each query page starts where the matches read so far end; the
            // rows listed are its length, which no field states.
            if read.query_pages == 0 || offset != read.page_offset {
                assert_eq!(
                    offset,
                    read.values.len() as u64,
                    "mcp={mcp}: query pages advance"
                );
                read.query_pages += 1;
                read.page_offset = offset;
            }
            let items = pagination["total"].as_u64().expect("total");
            assert_eq!(*read.total_items.get_or_insert(items), items, "mcp={mcp}");
        }
        join(&mut read, row, mcp);
        read.end = read.values.len() as u64;
        let next = envelope
            .pointer("/responsePagination/next")
            .or_else(|| row["data"]["next"].get("nextPage"));
        match next {
            Some(next) => input = next["input"].clone(),
            None => break,
        }
    }
    read
}

fn function(name: &str, body_lines: usize) -> String {
    let body = (0..body_lines)
        .map(|line| format!("  const v{line} = \"{name} line {line} ✓ padding text\";\n"))
        .collect::<String>();
    format!("function {name}() {{\n{body}}}")
}

fn fixture(workspace: &Workspace, big: Option<&str>) -> String {
    let mut root = None;
    for file in 0..FILES {
        let source = (0..FUNCTIONS_PER_FILE)
            .map(|index| function(&format!("f{file}_{index}"), 2) + "\n")
            .collect::<String>();
        let path = workspace.write(&format!("stream/src/f{file:02}.ts"), source);
        root = path.parent().map(std::path::Path::to_path_buf);
    }
    if let Some(big) = big {
        workspace.write("stream/src/zz_big.ts", format!("{big}\n"));
    }
    root.expect("fixture root").to_string_lossy().into_owned()
}

const MATCHES: usize = FILES * FUNCTIONS_PER_FILE;

#[tokio::test]
async fn a_match_walk_through_the_response_pager_reads_each_match_once_page_after_page() {
    let workspace = Workspace::new();
    let root = fixture(&workspace, None);
    let runtime = workspace.runtime(&[]);
    for mcp in [false, true] {
        let first = json!({"operation": "match", "path": root, "parser": "typescript",
            "rule": {"kind": "function_declaration"}, "includeCaptures": true, "limit": 7});
        let read = walk(&runtime, mcp, first, 6000).await;
        assert!(read.query_pages > 1, "mcp={mcp}: several pages");
        assert_eq!(
            Some(read.end),
            read.total_items,
            "mcp={mcp}: the last query page ends at the total"
        );
        assert_eq!(
            read.total_items,
            Some(MATCHES as u64),
            "mcp={mcp}: total counts matches"
        );
        assert_eq!(read.values.len(), MATCHES, "mcp={mcp}: every match once");
    }
    runtime.close().await;
}

#[tokio::test]
async fn a_match_larger_than_a_page_joins_back_from_its_row_parts() {
    let workspace = Workspace::new();
    let big = function("big", 400);
    let root = fixture(&workspace, Some(&big));
    let runtime = workspace.runtime(&[]);
    for mcp in [false, true] {
        let first = json!({"operation": "match", "path": root, "parser": "typescript",
            "rule": {"kind": "function_declaration"}, "includeCaptures": true, "limit": 7});
        let read = walk(&runtime, mcp, first, 6000).await;
        assert!(read.continued > 0, "mcp={mcp}: the big match was split");
        assert_eq!(read.total_items, Some(MATCHES as u64 + 1), "mcp={mcp}");
        assert_eq!(
            read.values.len(),
            MATCHES + 1,
            "mcp={mcp}: every match once"
        );
        let (_, joined) = read
            .values
            .iter()
            .find(|(key, _)| key.contains("zz_big.ts"))
            .expect("the big match");
        assert_eq!(
            *joined, big,
            "mcp={mcp}: row parts join to the exact source"
        );
    }
    runtime.close().await;
}
