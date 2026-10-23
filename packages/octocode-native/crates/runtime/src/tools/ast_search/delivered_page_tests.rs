//! astSearch pages as a runtime call delivers them, through the response
//! stages that respell paths ([`published_in`]). Every mode recomputes page
//! N from the page budget, so page 1 (an absolute `path`) and the
//! continuation it publishes (that path workspace-relative) must get one
//! budget; and rows under a deep scope are sized by the path the response
//! prints, so a page fits the window it was cut for.
use super::{AstSearchQuery, execute_ast};
use crate::execution::cancel::NeverCancel;
use crate::id::ToolId;
use crate::tools::test_support::{
    Fixture, assert_next_page_keeps_the_budget, checks, published_in, simple_policy,
};
use serde_json::{Value, json};

const WINDOW: usize = 16_384;

const PATTERN: &str = "export function $NAME(value: number): number { $$$BODY }";

/// `base` nested `depth` levels of 60-char directory names.
fn deep_dir(base: &str, depth: usize) -> String {
    (0..depth).fold(base.to_owned(), |dir, level| {
        let mut name = format!("level_{level}_{}", "x".repeat(60));
        name.truncate(60);
        format!("{dir}/{name}")
    })
}

fn typed(row: &Value) -> AstSearchQuery {
    let validated = crate::contracts::validate_query("astSearch", row.clone())
        .unwrap_or_else(|error| panic!("{row}: {error:?}"));
    serde_json::from_value(validated).expect("typed row")
}

/// The page budget astSearch cuts a row's pages by: each mode sizes its
/// typed query under the response window.
fn budget_of(window: Option<usize>, row: &Value) -> usize {
    let value = match typed(row) {
        AstSearchQuery::Symbols(query) => crate::response::result::query_value(&query),
        AstSearchQuery::MatchPattern(query) => crate::response::result::query_value(&query),
        AstSearchQuery::MatchRule(query) => crate::response::result::query_value(&query),
        AstSearchQuery::SyntaxTree(query) => crate::response::result::query_value(&query),
    };
    crate::response::page_windows::page_budget(window, &value)
}

fn functions(prefix: &str, count: usize) -> String {
    (0..count)
        .map(|index| {
            format!("export function {prefix}_{index:04}(value: number): number {{ return value + {index}; }}\n")
        })
        .collect()
}

/// `files` sources under `dir` of `root`, `per_file` functions each.
fn sources(root: &Fixture, dir: &str, files: usize, per_file: usize) {
    for file in 0..files {
        root.write(
            &format!("{dir}/file_{file:03}.ts"),
            functions(&format!("declared_{file:03}"), per_file),
        );
    }
}

/// Every astSearch mode recomputes page N from the page budget: page 1 run
/// from an absolute path and the `next.nextPage` its published response
/// leads to (the path respelled workspace-relative) get the same budget.
#[test]
fn every_mode_cuts_page_two_from_page_one_budget() {
    let root = Fixture::new();
    let dir = deep_dir("walks", 3);
    sources(&root, &format!("{dir}/many"), 500, 1);
    root.write(&format!("{dir}/big.ts"), functions("big", 1_500));
    // The canonical spelling the response stage respells (a temp dir may
    // sit behind a symlink, as `/var` on macOS).
    let base = root.0.canonicalize().expect("canonical root");
    let (paths, security) = simple_policy(&base);
    let many = base.join(format!("{dir}/many"));
    let big = base.join(format!("{dir}/big.ts"));
    let rows = [
        json!({"operation":"symbols","path":many}),
        json!({"operation":"match","pattern":PATTERN,"language":"typescript","path":many}),
        json!({"operation":"match","rule":{"pattern":PATTERN},"language":"typescript","path":many}),
        json!({"operation":"syntaxTree","path":big}),
    ];
    for row in rows {
        assert!(row["path"].as_str().is_some_and(|path| path.len() > 128));
        let validated = crate::contracts::validate_query("astSearch", row.clone()).expect("row");
        let out = execute_ast(&typed(&row), &paths, &security, &NeverCancel, Some(WINDOW))
            .unwrap_or_else(|error| panic!("{row}: {error:?}"));
        assert_eq!(out["pagination"]["hasMore"], true, "{row} spans pages");
        assert_next_page_keeps_the_budget(ToolId::AstSearch, &validated, &out, &paths, budget_of);
    }
}

/// Walks `row` through every page lead (`checks::walk`: each call once,
/// each row once), each page published as a runtime call delivers it, and
/// returns the delivered pages.
fn delivered_walk(root: &Fixture, row: Value, rows: fn(&Value) -> Vec<String>) -> Vec<Value> {
    let (paths, security) = simple_policy(&root.0);
    let mut pages = Vec::new();
    let walk = checks::walk("astSearch", json!({"queries": [row]}), |call| {
        let row = crate::contracts::validate_query("astSearch", call["queries"][0].clone())
            .expect("a published lead validates");
        let out = execute_ast(&typed(&row), &paths, &security, &NeverCancel, Some(WINDOW))
            .unwrap_or_else(|error| panic!("{row}: {error:?}"));
        let output = published_in(ToolId::AstSearch, &row, &out, &paths);
        pages.push(output.clone());
        checks::Page {
            rows: rows(&output["results"][0]["data"]),
            output,
        }
    });
    assert!(walk.queries.len() > 1, "the fixture spans pages");
    pages
}

/// `path name` of each row in a delivered page's files (`list` rows named
/// by `key`), or `path` of each listed file when `list` is empty.
fn file_rows(data: &Value, list: &str, key: &str) -> Vec<String> {
    let files = data["files"].as_array().into_iter().flatten();
    files
        .flat_map(|file| {
            let path = file["path"].as_str().expect("file path").to_owned();
            if list.is_empty() {
                return vec![path];
            }
            file[list]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| format!("{path} {}", row[key]))
                .collect()
        })
        .collect()
}

fn symbol_rows(data: &Value) -> Vec<String> {
    file_rows(data, "symbols", "symbolName")
}

fn match_rows(data: &Value) -> Vec<String> {
    file_rows(data, "matches", "line")
}

fn listed_files(data: &Value) -> Vec<String> {
    file_rows(data, "", "")
}

/// Under a deep scope the response prints each file's path
/// workspace-relative, far longer than the scope-rooted path the tool
/// writes: pages sized by the printed path fit the window (the response
/// pager never splits them) and still fill it, and a walk reaches every row
/// once.
/// The row ids of one page of a walk.
type RowIds = fn(&Value) -> Vec<String>;

#[test]
fn deep_scope_pages_measure_row_paths_as_delivered() {
    const FILES: usize = 300;
    let root = Fixture::new();
    let dir = deep_dir("deep", 5);
    sources(&root, &format!("{dir}/src"), FILES, 1);
    let scope = root.0.join(format!("{dir}/src"));
    let walks: [(Value, RowIds); 3] = [
        (json!({"operation":"symbols","path":scope}), symbol_rows),
        (
            json!({"operation":"match","pattern":PATTERN,"language":"typescript","path":scope}),
            match_rows,
        ),
        (
            json!({"operation":"match","pattern":PATTERN,"language":"typescript","path":scope,
                "resultView":"files"}),
            listed_files,
        ),
    ];
    for (row, rows) in walks {
        let pages = delivered_walk(&root, row.clone(), rows);
        let mut seen = 0;
        for (index, page) in pages.iter().enumerate() {
            let data = &page["results"][0]["data"];
            let path = data["files"][0]["path"].as_str().expect("a file row");
            assert!(path.len() > 300, "rows print the deep path: {path}");
            let chars = octocode_engine::json_size::utf16_units(page).unwrap_or(0);
            assert!(chars <= WINDOW, "{row}: page {index} is {chars} chars");
            if data["pagination"]["hasMore"] == true {
                assert!(
                    chars * 10 >= WINDOW * 7,
                    "{row}: page {index} holds only {chars} chars"
                );
            }
            seen += rows(data).len();
        }
        assert_eq!(seen, FILES, "{row}: every row once");
    }
}
