//! Budgeted match paging, bounded scans and pinned source identity.
use super::*;

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
        for name in ["nextPage", "nextMatchPage"] {
            if let Some(call) = out["next"].get(name) {
                assert_eq!(call["kind"], "continue");
                pending.push(call["input"]["queries"][0].clone());
            }
        }
        assert!(pages < 100, "nonterminating match walk");
    }
    assert!(pages > 1);
    assert_eq!(spans.len(), 60);
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
