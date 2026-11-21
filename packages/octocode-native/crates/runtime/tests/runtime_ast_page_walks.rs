//! Paged astSearch results started from an absolute `path` state, in their
//! JSON envelope, that pages remain (`pagination.hasMore`, `pagination.total`)
//! and lead on with an executable `next.nextPage`. Continuations respell
//! that path workspace-relative, and following only them still reaches
//! every row exactly once, over the CLI and the MCP channel (whose one text
//! block is the same JSON): page cuts never depend on the path's spelling.
//! A page the response pager splits into row parts is read to its last part
//! (`responsePagination.next`), where `next.nextPage` sits.

mod deep;
mod sample;
mod stream;

use crate::support::{Run as _, Workspace};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use std::collections::BTreeSet;

const FILES: usize = 40;
const DECLARATIONS_PER_FILE: usize = 60;

/// The structured envelope of a CLI or MCP call; an MCP result's text block
/// is that envelope's JSON, so its pagination and `next` read the same.
async fn call(runtime: &ToolRuntime, mcp: bool, id: usize, input: Value) -> Value {
    if mcp {
        let result = runtime
            .run_mcp(format!("walk-{id}"), "astSearch".into(), input)
            .await
            .expect("mcp call");
        assert_ne!(result["isError"], true, "{result}");
        crate::support::mcp_payload(&result)
    } else {
        runtime
            .run(format!("walk-{id}"), "astSearch".into(), input)
            .await
            .expect("cli call")
            .structured_content
            .into_value()
    }
}

/// A directory of TypeScript files, each declaring numbered functions; the
/// absolute path of the directory holding them.
fn fixture(workspace: &Workspace) -> String {
    let mut root = None;
    for file in 0..FILES {
        let source = (0..DECLARATIONS_PER_FILE)
            .map(|index| {
                format!("export function declared_{file:02}_{index:03}(value: number): number {{ return value + {index}; }}\n")
            })
            .collect::<String>();
        let path = workspace.write(&format!("outline/src/file_{file:02}.ts"), source);
        root = path.parent().map(std::path::Path::to_path_buf);
    }
    let root = root.expect("fixture root").to_string_lossy().into_owned();
    assert!(std::path::Path::new(&root).is_absolute(), "{root}");
    root
}

/// Each row of a page as `path key`, `key` naming a row within its file; an
/// outline line (Q5, `"<line>\t<kind> <name> …"`) names its own row.
fn rows(data: &Value, list: &str, key: &str) -> Vec<String> {
    data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let path = file["path"].as_str().expect("file path").to_owned();
            file[list]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |row| match row.as_str() {
                    Some(line) => format!("{path} {line}"),
                    None => format!("{path} {}", row[key]),
                })
        })
        .collect()
}

/// What a walk read: each page's rows in order (a page split into row parts
/// is one page), the exact `pagination.total` each page stated, how many
/// pages stated a floor of it instead (`countScope: loaded`, a match window
/// that ends before its scope does), and how many row parts past the first
/// the response pager split pages into.
struct Walk {
    pages: Vec<Vec<String>>,
    totals: BTreeSet<u64>,
    floors: usize,
    splits: usize,
}

impl Walk {
    fn rows(&self) -> Vec<String> {
        self.pages.concat()
    }

    /// The number of rows on each page: where the walk cut its pages.
    fn cuts(&self) -> Vec<usize> {
        self.pages.iter().map(Vec::len).collect()
    }
}

/// Follows the reply's `next` from the query row `first` until no reply
/// carries one: the one completion rule. A reply that splits its row
/// (`rowPart`) reads on the same way.
async fn walk(
    runtime: &ToolRuntime,
    mcp: bool,
    first: Value,
    page_rows: impl Fn(&Value) -> Vec<String>,
) -> Walk {
    walk_input(runtime, mcp, json!({"queries": [first]}), page_rows).await
}

/// [`walk`] from a whole first input (`queries` plus envelope fields).
async fn walk_input(
    runtime: &ToolRuntime,
    mcp: bool,
    mut input: Value,
    page_rows: impl Fn(&Value) -> Vec<String>,
) -> Walk {
    let mut walk = Walk {
        pages: Vec::new(),
        totals: BTreeSet::new(),
        floors: 0,
        splits: 0,
    };
    let mut calls = 0;
    let mut seen_inputs = BTreeSet::new();
    let mut read = 0;
    loop {
        calls += 1;
        assert!(
            seen_inputs.insert(input.to_string()),
            "mcp={mcp}: repeated continuation"
        );
        let envelope = call(runtime, mcp, calls, input.clone()).await;
        let row = &envelope["results"][0];
        let data = &row["data"];
        assert!(
            data.pointer("/next/nextPage").is_none(),
            "mcp={mcp}: the reply's next is the one walk: {data}"
        );
        let next = envelope.get("next");
        match data.get("pagination") {
            Some(pagination) => {
                assert!(
                    pagination.get("hasMore").is_none() && pagination.get("countScope").is_none(),
                    "mcp={mcp}: {pagination}"
                );
                // A stated total is exact: the same on every page that
                // states one.
                if let Some(total) = pagination["total"].as_u64() {
                    walk.totals.insert(total);
                }
            }
            // A page that reads on and states no count: its total is not
            // known yet (the scan has not reached the end of the scope).
            None if next.is_some() => walk.floors += 1,
            None => {}
        }
        let part = &row["rowPart"];
        if part.is_object() && part["part"] != part["of"] {
            walk.splits += 1;
            assert!(
                next.is_some(),
                "mcp={mcp}: a split row reads on: {envelope}"
            );
        }
        let rows = page_rows(data);
        read += rows.len() as u64;
        walk.pages.push(rows);
        let Some(next) = next else {
            break;
        };
        assert_eq!(next["tool"], "astSearch", "mcp={mcp}: {next}");
        input = next["input"].clone();
        assert!(input["cursor"].is_string(), "mcp={mcp}: {next}");
    }
    assert!(
        walk.pages.len() > 1,
        "mcp={mcp}: the fixture spans several pages"
    );
    assert!(
        walk.totals.len() <= 1,
        "mcp={mcp}: one exact total: {:?}",
        walk.totals
    );
    if let Some(total) = walk.totals.first() {
        assert_eq!(read, *total, "mcp={mcp}: the walk read the stated total");
    }
    walk
}

/// `seen` holds each of `expected` rows exactly once.
fn assert_each_once(seen: &[String], expected: usize, label: &str) {
    let unique = seen.iter().collect::<BTreeSet<_>>();
    assert_eq!(unique.len(), seen.len(), "{label}: a row was repeated");
    assert_eq!(seen.len(), expected, "{label}: every row once");
}

#[tokio::test]
async fn a_directory_outline_states_its_remaining_pages_and_walks_every_symbol_once() {
    let workspace = Workspace::new();
    let root = fixture(&workspace);
    let runtime = workspace.runtime(&[]);
    for mcp in [false, true] {
        let first = json!({"operation": "symbols", "path": root});
        let walked = walk(&runtime, mcp, first, |data| {
            rows(data, "symbols", "symbolName")
        })
        .await;
        let expected = FILES * DECLARATIONS_PER_FILE;
        assert_each_once(&walked.rows(), expected, &format!("symbols mcp={mcp}"));
        // Every page states the whole outline's size, the same on each.
        assert_eq!(
            walked.totals,
            BTreeSet::from([expected as u64]),
            "mcp={mcp}"
        );
    }
    runtime.close().await;
}

/// Many one-match files: each file group is smaller than the drift a
/// spelling-dependent budget would add to page 2, so any drift moves a cut.
const SMALL_FILES: usize = 500;

/// A walk-order match walk crosses windows that each end once their page is
/// full (TE3): those pages state floors (`countScope: loaded`), the last
/// window the exact total, and the walk reads each match a full scan (a
/// ranked order, which reads the whole scope first) lists, once.
#[tokio::test]
async fn a_directory_match_walk_from_an_absolute_path_reaches_every_match_once_across_windows() {
    let workspace = Workspace::new();
    let mut root = None;
    for file in 0..SMALL_FILES {
        let path = workspace.write(
            &format!("small/src/f_{file:03}.ts"),
            format!("export function f{file}(): number {{ return {file}; }}\n"),
        );
        root = path.parent().map(std::path::Path::to_path_buf);
    }
    let root = root.expect("fixture root").to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    for mcp in [false, true] {
        let first = json!({"operation": "match",
            "rule": {"pattern": "export function $NAME(): number { $$$BODY }"},
            "parser": "typescript", "path": root});
        let mut full = first.clone();
        let walked = walk(&runtime, mcp, first, |data| rows(data, "matches", "line")).await;
        assert_each_once(&walked.rows(), SMALL_FILES, &format!("match mcp={mcp}"));
        assert!(walked.floors > 0, "mcp={mcp}: the walk crossed windows");
        // The last window states the exact total.
        assert_eq!(
            walked.totals,
            BTreeSet::from([SMALL_FILES as u64]),
            "mcp={mcp}"
        );
        full["sort"] = json!({"by": "path", "order": "desc"});
        let ranked = walk(&runtime, mcp, full, |data| rows(data, "matches", "line")).await;
        let (mut read, mut scanned) = (walked.rows(), ranked.rows());
        read.sort();
        scanned.sort();
        assert_eq!(read, scanned, "mcp={mcp}: the rows a full scan lists");
    }
    runtime.close().await;
}
