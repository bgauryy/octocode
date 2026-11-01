//! `limit` is rows per page: matches in view content, files in the file
//! views, top-level declarations in symbols. A page ends at `limit` rows or
//! at the response window, whichever comes first; omitted, the window alone
//! cuts pages (today's pages); a walk reaches every row exactly once.
use super::paging_tests::{match_rows, numbered_functions, symbol_rows, walk};
use crate::tools::test_support::Fixture;
use serde_json::{Value, json};

fn file_rows(out: &Value) -> Vec<String> {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| file["path"].to_string())
        .collect()
}

fn fixture() -> Fixture {
    let root = Fixture::new();
    for file in 0..6 {
        let body = (0..=file)
            .map(|index| format!("hit({index});\n"))
            .collect::<String>();
        root.write(&format!("f{file}.ts"), body);
    }
    root.write("outline.ts", numbered_functions(9));
    root
}

fn matches(root: &Fixture, view: &str, limit: Option<u64>) -> Value {
    let mut row = json!({"operation":"match","rule":{"pattern":"hit($A)"},"path":root.0,"parser":"typescript","view":view});
    if let Some(limit) = limit {
        row["limit"] = json!(limit);
    }
    row
}

/// Walks `row` at each limit: every page holds 1..=limit rows, the walk
/// reaches the unlimited walk's rows once each, and the default window
/// holds `limit` rows, so the limit alone ends pages.
fn check(
    root: &Fixture,
    row: impl Fn(Option<u64>) -> Value,
    rows: fn(&Value) -> Vec<String>,
    total: usize,
) {
    let all = walk(&root.0, row(None), rows);
    assert_eq!(all.len(), 1, "omitted limit: one window-cut page, as today");
    assert_eq!(rows(&all[0]).len(), total);
    for limit in [1, 2, 5] {
        let pages = walk(&root.0, row(Some(limit)), rows);
        let counts = pages.iter().map(|out| rows(out).len()).collect::<Vec<_>>();
        assert!(
            counts
                .iter()
                .all(|count| (1..=limit as usize).contains(count)),
            "limit {limit}: {counts:?}"
        );
        let mut walked = pages.iter().flat_map(rows).collect::<Vec<_>>();
        walked.sort();
        let mut expected = rows(&all[0]);
        expected.sort();
        assert_eq!(walked, expected, "limit {limit}: every row once");
    }
}

#[test]
fn match_content_pages_hold_at_most_limit_matches() {
    let root = fixture();
    check(
        &root,
        |limit| matches(&root, "content", limit),
        match_rows,
        21,
    );
}

#[test]
fn match_file_pages_hold_at_most_limit_files() {
    let root = fixture();
    check(&root, |limit| matches(&root, "files", limit), file_rows, 6);
}

#[test]
fn symbol_pages_hold_at_most_limit_declarations() {
    let root = fixture();
    let row = |limit: Option<u64>| {
        let mut row = json!({"operation":"symbols","path":root.0.join("outline.ts")});
        if let Some(limit) = limit {
            row["limit"] = json!(limit);
        }
        row
    };
    check(&root, row, symbol_rows, 9);
}
