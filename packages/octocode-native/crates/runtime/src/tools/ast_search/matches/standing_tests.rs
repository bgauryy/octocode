//! A ranked order lists the scope's files whatever its time budget does: a
//! window the budget cuts lists no file and resumes with what it evaluated,
//! so the pages a ranked scan lists are the pages of a scan never cut, in
//! their order.
use crate::id::ToolId;
use crate::tools::ast_search::scan_window::test_window;
use crate::tools::ast_search::test_rows::run;
use crate::tools::test_support::{Fixture, assert_row_contract};
use serde_json::{Value, json};

/// Twelve files over three directories whose match counts rank in another
/// order than the walk's: the file with the most is neither first nor last.
fn spread() -> Fixture {
    let root = Fixture::new();
    for index in 0..12_usize {
        let dir = ["a", "b", "c/d"][index % 3];
        let hits = (index * 7) % 12 + 1;
        root.write(
            &format!("{dir}/f{index:02}.rs"),
            format!(
                "fn f() {{\n{}}}\n",
                (0..hits)
                    .map(|n| format!("hit({n});\n"))
                    .collect::<String>()
            ),
        );
    }
    // A scan of settled sources is kept for its later pages.
    octocode_engine::stamp::settle_written_so_far();
    root
}

/// A file row as its page lists it: path, count and every listed match.
fn file_rows(out: &Value) -> Vec<String> {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| {
            format!(
                "{} {} {}",
                file["path"].as_str().expect("path"),
                file["matchCount"],
                file.get("matches").unwrap_or(&Value::Null)
            )
        })
        .collect()
}

/// Every row a chain of calls lists, call after call, each window under the
/// next file window of `windows` (the last repeats; `None` is the time budget
/// alone, which a fixture never spends). `forget` drops what cut windows and
/// pages kept before each call, as another process would: a later page is
/// then scanned again. Returns the rows and how many windows the cut paused.
fn chain(
    root: &Fixture,
    start: Value,
    windows: &[Option<u32>],
    forget: bool,
) -> (Vec<String>, usize) {
    let (mut rows, mut paused, mut query) = (Vec::new(), 0, start);
    for call in 0.. {
        assert!(call < 200, "the scan never finishes: {query}");
        let _window = windows[call.min(windows.len() - 1)].map(test_window);
        if forget {
            super::standing::forget_all();
            if let Some(snapshot) = query["snapshot"].as_str() {
                super::memo::SCANS.evict(snapshot);
            }
        }
        let out = run(&root.0, query.clone()).expect("page");
        assert_row_contract(ToolId::AstSearch, &out);
        let listed = file_rows(&out);
        if out["warnings"]
            .to_string()
            .contains("paused at its time budget")
        {
            paused += 1;
            assert!(listed.is_empty(), "a paused window lists no file: {out}");
            assert_eq!(out["partialReasons"], json!(["scanBudget"]), "{out}");
            assert!(out["next"].get("nextPage").is_none(), "{out}");
        }
        rows.extend(listed);
        let next = out["next"]
            .get("nextPage")
            .or_else(|| out["next"].get("expandScan"));
        let Some(next) = next else {
            assert!(out.get("terminalLimit").is_none(), "{out}");
            return (rows, paused);
        };
        query = next["input"]["queries"][0].clone();
    }
    unreachable!()
}

fn ranked(root: &Fixture, extra: Value) -> Value {
    let mut query = json!({
        "operation": "match",
        "path": root.0,
        "parser": "rust",
        "rule": {"pattern": "hit($A)"},
    });
    for (field, value) in extra.as_object().expect("fields") {
        query[field] = value.clone();
    }
    query
}

#[test]
fn a_ranked_scan_lists_the_same_pages_in_the_same_order_whatever_the_cut() {
    let root = spread();
    let schedules: [&[Option<u32>]; 5] = [
        &[Some(1)],
        &[Some(2)],
        &[Some(5)],
        &[Some(3), None],
        &[None, Some(2)],
    ];
    for extra in [
        json!({"sort": {"by": "matchCount", "order": "desc"}, "limit": 8}),
        json!({"sort": {"by": "matchCount", "order": "asc"}, "limit": 12}),
        json!({"sort": {"by": "path", "order": "desc"}}),
        json!({"sort": {"by": "matchCount", "order": "desc"}, "view": "files"}),
        json!({"sort": {"by": "matchCount", "order": "desc"}, "view": "countMatches", "limit": 5}),
    ] {
        let start = ranked(&root, extra.clone());
        let (whole, unpaused) = chain(&root, start.clone(), &[None], false);
        assert_eq!(unpaused, 0);
        assert!(whole.len() >= 12, "{extra}: {whole:?}");
        for schedule in schedules {
            for forget in [false, true] {
                let (rows, paused) = chain(&root, start.clone(), schedule, forget);
                assert_eq!(rows, whole, "{extra} {schedule:?} forget={forget}");
                assert!(
                    paused > 0 || schedule[0].is_none(),
                    "{extra} {schedule:?}: the cut never paused the scan"
                );
            }
        }
    }
}

/// The file with the most matches heads the first page once the scan ends,
/// however many windows the cut paused before it.
#[test]
fn the_file_with_the_most_matches_heads_the_first_page_of_a_cut_scan() {
    let root = spread();
    // (index * 7) % 12 + 1 is 12 at index 5, which lies in c/d.
    let most = "c/d/f05.rs";
    let _window = test_window(2);
    let mut query = ranked(
        &root,
        json!({"sort": {"by": "matchCount", "order": "desc"}, "view": "countMatches", "limit": 1}),
    );
    let mut calls = 0;
    let first = loop {
        calls += 1;
        let out = run(&root.0, query.clone()).expect("page");
        if let Some(first) = out["files"].as_array().and_then(|files| files.first()) {
            break first.clone();
        }
        query = out["next"]["expandScan"]["input"]["queries"][0].clone();
        assert!(
            query["scanOffset"].as_u64().is_some(),
            "a pause resumes the scan: {out}"
        );
    };
    assert!(
        first["path"]
            .as_str()
            .is_some_and(|path| path.ends_with(most)),
        "{first}"
    );
    assert_eq!(first["matchCount"], 12, "{first}");
    assert!(calls > 1, "the cut never paused the scan");
}

/// The first page that lists files, window after window under `windows`.
fn first_listing(root: &Fixture, start: Value, windows: &[Option<u32>], forget: bool) -> Value {
    let mut query = start;
    for call in 0..200 {
        let _window = windows[call.min(windows.len() - 1)].map(test_window);
        if forget {
            super::standing::forget_all();
        }
        let out = run(&root.0, query.clone()).expect("page");
        match out["next"].get("expandScan") {
            Some(next) if out.get("files").is_none() => {
                query = next["input"]["queries"][0].clone();
            }
            _ => return out,
        }
    }
    panic!("the scan never finishes: {query}")
}

/// What the scan's walk met (files no grammar parses, a test directory a
/// `testCode` scope leaves unread) and what reading its files showed (test
/// lines left out, prefilter skips) are stated on the first page as a scan
/// never cut states them: that page is the same page.
#[test]
fn the_first_page_of_a_cut_ranked_scan_is_the_first_page_of_a_scan_never_cut() {
    let root = spread();
    root.write("notes.txt", "hit(1)\n");
    root.write("tests/t.rs", "fn t() {\nhit(1);\nhit(2);\n}\n");
    root.write(
        "b/mixed.rs",
        "fn f() {\nhit(1);\n}\n#[cfg(test)]\nmod t {\nfn g() {\nhit(2);\nhit(3);\n}\n}\n",
    );
    root.write("c/plain.rs", "fn quiet() {}\n");
    let unparsed = json!({
        "operation": "match",
        "path": root.0,
        "rule": {"pattern": "hit($A)"},
        "sort": {"by": "matchCount", "order": "desc"},
    });
    for start in [
        ranked(
            &root,
            json!({"sort": {"by": "matchCount", "order": "desc"}, "testCode": "exclude", "limit": 4}),
        ),
        ranked(
            &root,
            json!({"sort": {"by": "matchCount", "order": "asc"}, "view": "countMatches"}),
        ),
        unparsed,
    ] {
        let whole = first_listing(&root, start.clone(), &[None], false);
        assert!(whole.get("files").is_some(), "{whole}");
        for schedule in [&[Some(1)][..], &[Some(4)], &[Some(2), None]] {
            for forget in [false, true] {
                let cut = first_listing(&root, start.clone(), schedule, forget);
                assert_eq!(cut, whole, "{start} {schedule:?} forget={forget}");
            }
        }
    }
}
