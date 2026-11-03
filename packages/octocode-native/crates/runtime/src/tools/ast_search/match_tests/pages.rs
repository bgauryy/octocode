//! Budgeted match paging, bounded scans and pinned source identity.
use super::*;
use crate::id::ToolId;
use serde_json::Value;

/// A cut scan says so and resumes: partial, named by its cause, with the
/// `expandScan` lead that reads the files after it.
#[test]
fn a_cut_scan_is_surfaced_and_resumes_after_its_window() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("source file");
    }
    let _window = crate::tools::ast_search::scan_window::test_window(1);
    for query in [
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"pub fn source() {}"}}),
        json!({"operation":"symbols","path":root.0}),
    ] {
        let out = run(&root.0, query).expect("scan");
        assert_eq!(out["partialReasons"], json!(["scanBudget"]), "{out}");
        assert!(out.get("terminalLimit").is_none(), "{out}");
        let expand = &out["next"]["expandScan"]["input"]["queries"][0];
        assert!(expand.get("page").is_none(), "page 1 is the default: {out}");
        assert!(
            out["diagnostics"]
                .to_string()
                .contains("structural.scan.truncated"),
            "{out}"
        );
    }
}

#[test]
fn match_continuation_rejects_stale_snapshot() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("file");
    }
    let page1 = run_window(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"pub fn source() {}"}}),
        1,
    )
    .expect("page1");
    let next = page1["next"]["nextPage"]["input"].clone();
    root.write("e.rs", "pub fn source() {}\n");
    let page2 = run_window(&root.0, next, 1).expect("page2");
    assert_eq!(page2["errorCode"], json!("staleSnapshot"), "got {page2}");
    // Never a dead end: the restart is the same query from its first page
    // (the default), without any cursor.
    let restart = &crate::tools::test_support::checks::related_call(&page2, "astSearch", |_| true)
        ["input"]["queries"][0];
    for cursor in ["page", "matchPage", "snapshot", "scanOffset"] {
        assert!(restart.get(cursor).is_none(), "{cursor}: {page2}");
    }
    let fresh = run(&root.0, restart.clone()).expect("restart runs");
    assert_eq!(fresh["pagination"]["totalItems"], 5, "{fresh}");
}

#[test]
fn budgeted_match_pages_reach_every_span_once_and_report_unread_work() {
    let root = Fixture::new();
    for file in 0..3 {
        root.write(
            &format!("f{file}.rs"),
            format!(
                "fn f{file}() {{\n{}\n}}",
                (0..20).map(|n| format!("hit({n});\n")).collect::<String>()
            ),
        );
    }
    let mut pending = vec![
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"}}),
    ];
    let mut queries = std::collections::BTreeSet::new();
    let mut spans = std::collections::BTreeSet::new();
    let mut pages = 0;
    while let Some(query) = pending.pop() {
        assert!(queries.insert(query.to_string()), "duplicate continuation");
        let out = run_window(&root.0, query, 600).expect("budgeted match");
        pages += 1;
        for file in out["files"].as_array().into_iter().flatten() {
            for row in file["matches"].as_array().into_iter().flatten() {
                assert!(
                    spans.insert((
                        file["path"].to_string(),
                        row["line"].to_string(),
                        row["column"].to_string()
                    )),
                    "repeated span: {out}"
                );
            }
        }
        // The row's walk reaches every file's remaining matches; no file's
        // pager repeats its call.
        assert_no_repeated_continuation(&out);
        assert_eq!(
            out["pagination"]["hasMore"].as_bool().unwrap_or(false),
            out["next"]["nextPage"].is_object(),
            "hasMore and the row's nextPage agree: {out}"
        );
        if let Some(call) = out["next"].get("nextPage") {
            assert_eq!(call["kind"], "continue");
            pending.push(call["input"]["queries"][0].clone());
        }
        assert!(pages < 100, "nonterminating match walk");
    }
    assert!(pages > 1);
    assert_eq!(spans.len(), 60);
}

#[test]
fn match_pages_are_file_collection_pages_not_partial_coverage() {
    let root = Fixture::new();
    root.write("a.rs", "fn f(){hit(1);hit(2);hit(3);}");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"},"limit":2}),
    )
    .expect("match page");
    let file = &out["files"][0];
    assert_eq!(file["pagination"]["hasMore"], true, "{out}");
    assert!(
        out["next"]["nextPage"].is_object() && file["pagination"].get("next").is_none(),
        "the file's remaining matches are the row's own next page, offered once: {out}"
    );
    assert!(
        out.get("isPartial").is_none(),
        "paging is not partial coverage: {out}"
    );
    assert!(
        out.get("partialReasons").is_none(),
        "matchPages is retired: {out}"
    );
}

#[test]
fn an_out_of_range_private_page_is_distinct_from_a_stale_snapshot() {
    let root = Fixture::new();
    root.write("a.rs", "fn f(){hit(1);}");
    let base = json!({"operation":"match","path":root.0,"rule":{"pattern":"hit($A)"}});
    let first = run(&root.0, base.clone()).expect("first");
    let mut beyond = base.clone();
    beyond["page"] = json!(99);
    beyond["snapshot"] = first["snapshot"].clone();
    let out = run(&root.0, beyond).expect("past end");
    assert_eq!(out["pagination"]["outOfRange"], true);
    assert_eq!(out["files"], json!([]));
    let mut wrong = base;
    wrong["page"] = json!(2);
    wrong["snapshot"] = json!("not-the-snapshot");
    assert_eq!(
        run(&root.0, wrong).expect("stale")["errorCode"],
        "staleSnapshot"
    );
}

/// The order of a result is a function of the input: a scan cut into any
/// file windows lists the same rows, in the same order, as the whole scan.
/// The names make a path's text order (`a-b.rs` < `a.rs` < `a/x.rs`) differ
/// from the walk's component order (`a/x.rs` < `a-b.rs` < `a.rs`).
#[test]
fn match_order_does_not_depend_on_the_scan_window() {
    let root = Fixture::new();
    for name in ["a/x.rs", "a-b.rs", "a.rs", "b/c/y.rs", "b-c.rs", "b.rs"] {
        root.write(name, "fn f(){hit(1);hit(2);}");
    }
    let query =
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"}});
    let whole = ast_rows(&ast_chain(&root.0, query.clone()));
    assert_eq!(whole.len(), 12, "{whole:?}");
    for files in [1, 2, 4] {
        let _window = crate::tools::ast_search::scan_window::test_window(files);
        assert_eq!(
            ast_rows(&ast_chain(&root.0, query.clone())),
            whole,
            "windows of {files} files"
        );
    }
}

/// Follows only the row's own linear walk (`next.nextPage` of the page, never
/// a file's pager) from `query` to the end, checking that every page whose
/// `pagination.hasMore` is true carries that call.
fn walk_row_channel(root: &std::path::Path, query: Value) -> Vec<Value> {
    let mut pages = Vec::new();
    let mut call = json!({"queries":[query]});
    loop {
        let input = crate::tools::test_support::opened_input(ToolId::AstSearch, &call);
        let out = run(root, input["queries"][0].clone()).expect("page");
        let next = out["next"]["nextPage"].clone();
        assert_eq!(
            out["pagination"]["hasMore"].as_bool().unwrap_or(false),
            next.is_object(),
            "hasMore and the row's nextPage agree: {out}"
        );
        assert_no_repeated_continuation(&out);
        pages.push(out);
        assert!(pages.len() < 100, "nonterminating row walk");
        if !next.is_object() {
            return pages;
        }
        call = next["input"].clone();
    }
}

/// A file whose matches split across pages is read through the row's own
/// `next.nextPage`, whether or not more files follow: a walker that follows
/// the row channel alone reaches every match exactly once.
#[test]
fn a_split_file_continues_through_the_row_channel_to_the_end() {
    for files in [1, 3] {
        let root = Fixture::new();
        for file in 0..files {
            root.write(
                &format!("f{file}.rs"),
                format!(
                    "fn f() {{\n{}}}",
                    (0..5).map(|n| format!("hit({n});\n")).collect::<String>()
                ),
            );
        }
        let query = json!({
            "operation":"match","path":root.0,"parser":"rust",
            "rule":{"pattern":"hit($A)"},"limit":2
        });
        let pages = walk_row_channel(&root.0, query.clone());
        let rows = ast_rows(&pages);
        assert_eq!(rows.len(), files * 5, "{files} files: {rows:?}");
        assert_eq!(
            rows.iter().collect::<std::collections::BTreeSet<_>>().len(),
            rows.len(),
            "{files} files: a match repeated"
        );
        // The walk that also follows each file's pager reaches the same rows.
        assert_eq!(
            ast_rows(&ast_chain(&root.0, query)).len(),
            rows.len(),
            "{files} files: the file pager is the same walk"
        );
    }
}

/// Files with unequal match counts share one file page, a split file beside
/// an unsplit one: the row's walk reaches each file's matches exactly once,
/// and an exhausted file does not reappear on a later match page.
#[test]
fn unequal_match_counts_on_one_page_are_each_read_once() {
    let root = Fixture::new();
    for (file, hits) in [("a.rs", 1), ("b.rs", 7), ("c.rs", 3), ("d.rs", 2)] {
        root.write(
            file,
            format!(
                "fn f() {{\n{}}}",
                (0..hits)
                    .map(|n| format!("hit({n});\n"))
                    .collect::<String>()
            ),
        );
    }
    let query = json!({
        "operation":"match","path":root.0,"parser":"rust",
        "rule":{"pattern":"hit($A)"},"limit":2
    });
    let pages = walk_row_channel(&root.0, query);
    let rows = ast_rows(&pages);
    assert_eq!(rows.len(), 13, "{rows:?}");
    assert_eq!(
        rows.iter().collect::<std::collections::BTreeSet<_>>().len(),
        rows.len(),
        "a match repeated: {rows:?}"
    );
    // The split file b.rs leaves the walk once its matches ran out.
    let last = pages.last().expect("pages");
    assert!(last["pagination"].get("next").is_none(), "{last}");
}

/// A scan cut before it evaluated any file has no position to resume from:
/// the response is a contract error, never `hasMore` without a call.
#[test]
fn a_cut_scan_with_no_resume_position_is_an_error_not_a_dead_end() {
    let root = Fixture::new();
    root.write("a.rs", "fn f(){hit(1);}");
    let _window = crate::tools::ast_search::scan_window::test_window(0);
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"}}),
    );
    let error = out.expect_err("a scan with no resume position cannot continue");
    assert_eq!(error.code, ErrorCode::OutputContractViolation);
    assert!(error.message.contains("continuation"), "{error:?}");
}
