//! Walks under a deep directory, started from an absolute `path` over 128
//! characters and from its workspace-relative spelling. Rows print the long
//! workspace-relative path, and pages are sized by it: at the default window
//! the response pager never splits a page into row parts, both spellings cut
//! the same pages, and every row arrives exactly once. Under a small
//! `responseLength` pages do split, and reading each through its row parts
//! still reaches every row once.

use super::{Walk, assert_each_once, rows, walk};
use crate::support::Workspace;
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};

/// One-declaration files: many small file rows, each far smaller than the
/// deep path the response prints for it.
const FILES: usize = 300;
/// Functions of the one source a syntax tree walks.
const FUNCTIONS: usize = 300;
const PATTERN: &str = "export function $NAME(value: number): number { $$$BODY }";

fn function(name: &str, index: usize) -> String {
    format!("export function {name}(value: number): number {{ return value + {index}; }}\n")
}

/// `FILES` one-function sources and one `FUNCTIONS`-function source under
/// three 60-char directory levels: the absolute and the workspace-relative
/// path of the sources' directory and of the big source.
struct Deep {
    dir: (String, String),
    big: (String, String),
}

fn deep(workspace: &Workspace) -> Deep {
    let base = (0..3).fold("deep".to_owned(), |dir, level| {
        let mut name = format!("level_{level}_{}", "x".repeat(60));
        name.truncate(60);
        format!("{dir}/{name}")
    });
    for file in 0..FILES {
        workspace.write(
            &format!("{base}/src/file_{file:03}.ts"),
            function(&format!("declared_{file:03}"), file),
        );
    }
    let big = (0..FUNCTIONS)
        .map(|index| function(&format!("big_{index:04}"), index))
        .collect::<String>();
    workspace.write(&format!("{base}/big.ts"), big);
    let spell = |relative: String| {
        let absolute = workspace.workspace.join(&relative);
        let absolute = absolute.to_string_lossy().into_owned();
        assert!(absolute.len() > 128, "a deep absolute path: {absolute}");
        (absolute, relative)
    };
    Deep {
        dir: spell(format!("{base}/src")),
        big: spell(format!("{base}/big.ts")),
    }
}

fn symbol_rows(data: &Value) -> Vec<String> {
    rows(data, "symbols", "symbolName")
}

fn match_rows(data: &Value) -> Vec<String> {
    rows(data, "matches", "line")
}

/// The node ids of a syntax-tree page: each node row starts with its id.
fn node_ids(data: &Value) -> Vec<String> {
    data["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|node| {
            let node = node.as_str().expect("a node row");
            node.split(' ').next().unwrap_or_default().to_owned()
        })
        .collect()
}

fn first_row(operation: &str, path: &str) -> Value {
    let mut row = json!({"operation": operation, "path": path,
        "mainGoal": "Read every row under a deep directory.",
        "reasoning": "Follow every page to the end."});
    if operation == "match" {
        row["pattern"] = PATTERN.into();
        row["language"] = "typescript".into();
    }
    row
}

/// Walks `operation` from both spellings of `paths` over the CLI and MCP:
/// the same page cuts, every one of `expected` rows once, no row-part split.
async fn walks_alike(
    runtime: &ToolRuntime,
    operation: &str,
    paths: &(String, String),
    page_rows: fn(&Value) -> Vec<String>,
    expected: usize,
) -> Walk {
    let mut cuts = None;
    let mut last = None;
    for mcp in [false, true] {
        for path in [&paths.0, &paths.1] {
            let label = format!("{operation} mcp={mcp} {path}");
            let walked = walk(runtime, mcp, first_row(operation, path), page_rows).await;
            assert_each_once(&walked.rows(), expected, &label);
            assert_eq!(walked.splits, 0, "{label}: pages fit the window");
            let cut = walked.cuts();
            assert_eq!(*cuts.get_or_insert_with(|| cut.clone()), cut, "{label}");
            last = Some(walked);
        }
    }
    last.expect("a walk")
}

#[tokio::test]
async fn deep_symbol_and_match_walks_fit_every_page_and_reach_every_row_once() {
    let workspace = Workspace::new();
    let deep = deep(&workspace);
    let runtime = workspace.runtime(&[]);
    walks_alike(&runtime, "symbols", &deep.dir, symbol_rows, FILES).await;
    let matches = walks_alike(&runtime, "match", &deep.dir, match_rows, FILES).await;
    // Match pages count files.
    assert_eq!(
        matches.totals,
        std::collections::BTreeSet::from([FILES as u64])
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_deep_syntax_tree_walk_reaches_every_node_once_from_either_spelling() {
    let workspace = Workspace::new();
    let deep = deep(&workspace);
    let runtime = workspace.runtime(&[]);
    let first = walk(
        &runtime,
        false,
        first_row("syntaxTree", &deep.big.0),
        node_ids,
    )
    .await;
    let total = first.totals.iter().copied().max().expect("totalItems");
    let total = usize::try_from(total).expect("node count");
    let walked = walks_alike(&runtime, "syntaxTree", &deep.big, node_ids, total).await;
    // Node ids number the whole tree from 0, in order across pages.
    let ids = walked.rows();
    let expected = (0..total).map(|id| id.to_string()).collect::<Vec<_>>();
    assert_eq!(ids, expected);
    runtime.close().await;
}

/// A `responseLength` below a page splits it into row parts; a walk that
/// reads each page through its parts before `next.nextPage` (which only the
/// last part carries) still reaches every row once, from either spelling.
/// The row ids of one page of a walk.
type RowIds = fn(&Value) -> Vec<String>;

#[tokio::test]
async fn split_pages_read_through_their_row_parts_reach_every_row_once() {
    let workspace = Workspace::new();
    let deep = deep(&workspace);
    let runtime = workspace.runtime(&[]);
    let cases: [(&str, &(String, String), RowIds); 3] = [
        ("symbols", &deep.dir, symbol_rows),
        ("match", &deep.dir, match_rows),
        ("syntaxTree", &deep.big, node_ids),
    ];
    for (operation, paths, page_rows) in cases {
        let mut seen = None;
        for mcp in [false, true] {
            for path in [&paths.0, &paths.1] {
                let label = format!("{operation} mcp={mcp} {path}");
                let mut first = json!({"queries": [first_row(operation, path)]});
                first["responseLength"] = 3_000.into();
                let walked = super::walk_input(&runtime, mcp, first, page_rows).await;
                assert!(walked.splits > 0, "{label}: pages split into row parts");
                let rows = walked.rows();
                let total = walked.totals.iter().copied().max().expect("totalItems");
                assert_eq!(walked.totals.len(), 1, "{label}: one total on every page");
                assert_each_once(&rows, usize::try_from(total).expect("count"), &label);
                assert_eq!(*seen.get_or_insert_with(|| rows.clone()), rows, "{label}");
            }
        }
    }
    runtime.close().await;
}
