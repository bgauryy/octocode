//! Paged astSearch results started from an absolute `path` state, in their
//! JSON envelope, that pages remain (`pagination.hasMore`, `totalItems`)
//! and lead on with an executable `next.nextPage`. Continuations respell
//! that path workspace-relative, and following only them still reaches
//! every row exactly once, over the CLI and the MCP channel (whose one text
//! block is the same JSON): page cuts never depend on the path's spelling.
//! A page the response pager splits into row parts is read to its last part
//! (`responsePagination.next`), where `next.nextPage` sits.

mod deep;

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
        let text = result["content"][0]["text"].as_str().expect("text block");
        let structured = &result["structuredContent"];
        assert_eq!(
            serde_json::from_str::<Value>(text).expect("JSON text"),
            *structured,
            "the text block is the structured JSON"
        );
        structured.clone()
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

/// Each row of a page as `path key`, `key` naming a row within its file.
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
                .map(move |row| format!("{path} {}", row[key]))
        })
        .collect()
}

/// What a walk read: each page's rows in order (a page split into row parts
/// is one page), the `totalItems` each page stated, and how many row parts
/// past the first the response pager split pages into.
struct Walk {
    pages: Vec<Vec<String>>,
    totals: BTreeSet<u64>,
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

/// Follows `next.nextPage` from the query row `first` to the last page,
/// reading each page through every row part (`responsePagination.next`)
/// first: a split page carries `next.nextPage` on its last part only.
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
        splits: 0,
    };
    let mut calls = 0;
    loop {
        let (mut rows, mut has_more, mut next_page) = (Vec::new(), None, None);
        loop {
            calls += 1;
            assert!(calls < 200, "mcp={mcp}: the walk terminates");
            let envelope = call(runtime, mcp, calls, input.clone()).await;
            let row = &envelope["results"][0];
            let data = &row["data"];
            if let Some(pagination) = data.get("pagination") {
                walk.totals
                    .insert(pagination["totalItems"].as_u64().expect("totalItems"));
                has_more = Some(pagination["hasMore"] == true);
            }
            rows.extend(page_rows(data));
            if let Some(next) = data["next"].get("nextPage") {
                if let Some(part) = row.get("rowPart") {
                    assert_eq!(
                        part["part"], part["of"],
                        "mcp={mcp}: nextPage on part {part}"
                    );
                }
                next_page = Some(next.clone());
            }
            let response = &envelope["responsePagination"];
            if response["hasMore"] != true {
                break;
            }
            walk.splits += 1;
            input = response["next"]["query"].clone();
            assert!(input.is_object(), "mcp={mcp}: {response}");
        }
        walk.pages.push(rows);
        if has_more.expect("every page states pagination") {
            // Pages remain: the envelope says so and leads to the next.
            let next = next_page.unwrap_or_else(|| panic!("mcp={mcp}: hasMore without nextPage"));
            assert_eq!(next["tool"], "astSearch", "mcp={mcp}: {next}");
            input = next["query"].clone();
            assert!(input["queries"][0].is_object(), "mcp={mcp}: {next}");
        } else {
            assert!(
                next_page.is_none(),
                "mcp={mcp}: the last page leads nowhere"
            );
            break;
        }
    }
    assert!(
        walk.pages.len() > 1,
        "mcp={mcp}: the fixture spans several pages"
    );
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
        let first = json!({"operation": "symbols", "path": root,
            "mainGoal": "Outline every declaration.",
            "reasoning": "Follow next.nextPage to the end."});
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

#[tokio::test]
async fn a_directory_match_walk_from_an_absolute_path_reaches_every_match_once() {
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
            "pattern": "export function $NAME(): number { $$$BODY }",
            "language": "typescript", "path": root,
            "mainGoal": "Find every exported function.",
            "reasoning": "Follow next.nextPage to the end."});
        let walked = walk(&runtime, mcp, first, |data| rows(data, "matches", "line")).await;
        assert_each_once(&walked.rows(), SMALL_FILES, &format!("match mcp={mcp}"));
        // Match pages count files.
        assert_eq!(
            walked.totals,
            BTreeSet::from([SMALL_FILES as u64]),
            "mcp={mcp}"
        );
    }
    runtime.close().await;
}
