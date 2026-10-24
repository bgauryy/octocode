//! Budgeted match paging, bounded scans and pinned source identity.
use super::*;

#[test]
fn match_directory_scan_truncation_is_surfaced_not_silent() {
    let root = Fixture::new();
    // Three candidate files, each with the matchable identifier `source`.
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("source file");
    }
    let (paths, security) = simple_policy(&root.0);
    let out = execute_row(
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"pub fn source() {}"},"maxFiles":1}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("match runs");

    assert!(out.get("truncated").is_none(), "undeclared field: {out}");
    assert_eq!(
        out["partialReasons"],
        json!(["maxFiles"]),
        "a capped scan names its cap; got {out}"
    );
    assert_eq!(
        out["isPartial"],
        json!(true),
        "capped scan is partial; got {out}"
    );
    let diagnostics = out["diagnostics"].as_array().cloned().unwrap_or_default();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"] == json!("structural.scan.truncated")),
        "expected a structural.scan.truncated diagnostic; got {out}"
    );
}

#[test]
fn a_raisable_max_files_cut_offers_an_expanded_scan() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("source file");
    }
    let rows = [
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"pub fn source() {}"},"maxFiles":1}),
        json!({"operation":"symbols","path":root.0,"maxFiles":1}),
    ];
    for query in rows {
        let out = run(&root.0, query.clone()).expect("scan");
        assert!(out.get("terminalLimit").is_none(), "{out}");
        let expand = &out["next"]["expandScan"]["input"]["queries"][0];
        assert_eq!(expand["maxFiles"], 2, "{out}");
        assert_eq!(expand["page"], 1, "{out}");
        assert!(
            expand
                .get("snapshot")
                .is_none_or(serde_json::Value::is_null),
            "{out}"
        );
        assert!(
            out["diagnostics"]
                .as_array()
                .is_some_and(|diagnostics| diagnostics
                    .iter()
                    .any(|d| d["code"] == "structural.scan.truncated")),
            "{out}"
        );
        let expanded = run(&root.0, expand.clone()).expect("expanded scan");
        assert_eq!(
            expanded["next"]["expandScan"]["input"]["queries"][0]["maxFiles"], 4,
            "{expanded}"
        );
        let whole = {
            let mut whole = query.clone();
            whole["maxFiles"] = json!(4);
            run(&root.0, whole).expect("whole scan")
        };
        assert!(whole["next"].get("expandScan").is_none(), "{whole}");
        assert!(whole.get("terminalLimit").is_none(), "{whole}");
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
