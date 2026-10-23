//! Match pages: ceilings, scan cuts, snapshots, later pages and the
//! out-of-range page.

use super::*;

#[test]
fn match_pagination_ceilings_preserve_rows_and_disclose_terminal_limits() {
    let root = Fixture::new();
    for i in 0..1001 {
        root.write(&format!("f{i:04}.ts"), "console.log(1);\n");
    }
    let (paths, security) = simple_policy(&root.0);
    let base = json!({"operation":"match","path":root.0,"pattern":"console.log($A)","language":"typescript","pageSize":1,"maxFiles":1500,"mainGoal":"test","reasoning":"test"});
    let first = execute_row(base.clone(), &paths, &security, &NeverCancel).expect("first page");
    let mut last = first["next"]["nextPage"]["query"]["queries"][0].clone();
    last["page"] = json!(1000);
    let page = execute_row(last, &paths, &security, &NeverCancel).expect("ceiling page");
    assert_eq!(page["files"].as_array().expect("files").len(), 1, "{page}");
    assert!(page["next"].get("nextPage").is_none(), "{page}");
    assert_eq!(page["terminalLimit"], true, "{page}");

    let file = root.0.join("rows.ts");
    std::fs::write(&file, "console.log(1);\n".repeat(1001)).expect("match rows");
    let first_rows = execute_row(json!({"operation":"match","path":file,"pattern":"console.log($A)","language":"typescript","matchPageSize":1,"mainGoal":"test","reasoning":"test"}), &paths, &security, &NeverCancel).expect("first match page");
    let mut last_rows = first_rows["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    last_rows["matchPage"] = json!(1000);
    let row_page = execute_row(last_rows, &paths, &security, &NeverCancel).expect("match ceiling");
    assert_eq!(
        row_page["files"][0]["matches"]
            .as_array()
            .expect("rows")
            .len(),
        1,
        "{row_page}"
    );
    assert!(
        row_page["next"].get("nextMatchPage").is_none(),
        "{row_page}"
    );
    assert_eq!(row_page["terminalLimit"], true, "{row_page}");
}

// Regression: astSearch `match` over a directory must not silently drop files
// past `maxFiles`. When the candidate scan is truncated, the result must carry
// an explicit truncation signal (a `structural.scan.truncated` diagnostic and a
// top-level `truncated`/`complete:false`) rather than reporting a bounded set as
// if it were the whole corpus.
#[test]
fn match_directory_scan_truncation_is_surfaced_not_silent() {
    let root = Fixture::new();
    // Three candidate files, each with the matchable identifier `source`.
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("source file");
    }
    let (paths, security) = simple_policy(&root.0);
    let out = execute_row(
        json!({
            "operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust",
            "pattern":"pub fn source() {}","maxFiles":1
        }),
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

/// A `maxFiles` cut below the schema maximum is a raisable bound, not a
/// terminal limit: match and symbols pages carry `next.expandScan`, which
/// doubles the bound from page 1, and a warning naming the cut.
#[test]
fn a_raisable_max_files_cut_offers_an_expanded_scan() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.0.join(name), "pub fn source() {}\n").expect("source file");
    }
    let rows = [
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust",
            "pattern":"pub fn source() {}","maxFiles":1}),
        json!({"operation":"symbols","mainGoal":"test","reasoning":"test","path":root.0,"maxFiles":1}),
    ];
    for query in rows {
        let out = run(&root.0, query.clone()).expect("scan");
        assert!(out.get("terminalLimit").is_none(), "{out}");
        let expand = &out["next"]["expandScan"]["query"]["queries"][0];
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
            expanded["next"]["expandScan"]["query"]["queries"][0]["maxFiles"], 4,
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
    let (paths, security) = simple_policy(&root.0);
    let page1 = execute_row(
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"pub fn source() {}","pageSize":2}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("page1");
    let snapshot = page1["snapshot"].as_str().expect("snapshot").to_string();
    root.write("e.rs", "pub fn source() {}\n");
    let page2 = execute_row(
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"pub fn source() {}","pageSize":2,"page":2,"snapshot":snapshot}),
        &paths,
        &security,
        &NeverCancel,
    )
    .expect("page2");
    assert_eq!(page2["errorCode"], json!("staleSnapshot"), "got {page2}");
    // Never a dead end: the restart is the same query from its first page
    // (the default), without any cursor.
    let restart = &page2["next"]["restart"]["query"]["queries"][0];
    for cursor in ["page", "matchPage", "snapshot", "scanOffset"] {
        assert!(restart.get(cursor).is_none(), "{cursor}: {page2}");
    }
    let fresh = run(&root.0, restart.clone()).expect("restart runs");
    assert_eq!(fresh["pagination"]["totalItems"], 5, "{fresh}");
}

/// A page past the end of an astSearch match listing is the shared
/// out-of-range outcome (`pagination.outOfRange`), with or without the
/// snapshot: a missing snapshot is a fresh scan of that page, never a
/// `staleSnapshot` error, and only a snapshot that no longer matches is stale.
#[test]
fn match_page_past_the_end_is_out_of_range_not_stale() {
    let root = Fixture::new();
    for name in ["a.ts", "b.ts", "c.ts"] {
        std::fs::write(root.0.join(name), "console.log(1);\n").expect("file");
    }
    let base = json!({"operation":"match","path":root.0,"language":"typescript",
        "pattern":"console.log($A)","pageSize":2});
    let first = run(&root.0, base.clone()).expect("first");
    let snapshot = first["next"]["nextPage"]["query"]["queries"][0]["snapshot"].clone();
    let mut bare = base.clone();
    bare["page"] = json!(99);
    let mut pinned = bare.clone();
    pinned["snapshot"] = snapshot;
    for query in [bare, pinned] {
        let out = run(&root.0, query).expect("out of range page");
        assert!(out.get("errorCode").is_none(), "{out}");
        assert_eq!(out["files"], json!([]), "{out}");
        assert_eq!(out["pagination"]["outOfRange"], true, "{out}");
        assert_eq!(out["pagination"]["currentPage"], 99, "{out}");
        assert_eq!(out["pagination"]["totalPages"], 2, "{out}");
    }
    let mut wrong = base;
    wrong["page"] = json!(2);
    wrong["snapshot"] = json!("not-the-snapshot");
    let stale = run(&root.0, wrong).expect("stale");
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
}

#[test]
fn match_pagination_limits_next_match_page_to_the_current_file_page() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() { hit(1); }\n");
    root.write("b.rs", "fn b() { hit(1); hit(2); hit(3); }\n");
    let base = json!({
        "operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"hit($A)",
        "pageSize":1,"matchPageSize":2
    });
    let page1 = run(&root.0, base.clone()).expect("page1");
    // A file whose matches all fit on its match page carries no row counts.
    assert_eq!(
        page1["files"][0]["matches"].as_array().map(Vec::len),
        Some(1),
        "{page1}"
    );
    assert!(
        page1["files"][0].get("totalMatchRows").is_none()
            && page1["files"][0].get("returnedMatchRows").is_none(),
        "{page1}"
    );
    assert!(
        page1["next"].get("nextMatchPage").is_none(),
        "page 1 has no truncated file, so no nextMatchPage: {page1}"
    );
    let next = page1["next"]["nextPage"]["query"]["queries"][0].clone();
    let page2 = run(&root.0, next).expect("page2");
    assert_eq!(
        page2["files"][0]["matches"].as_array().map(Vec::len),
        Some(2),
        "{page2}"
    );
    assert!(
        page2["next"]["nextMatchPage"].is_object(),
        "unreturned matches remain: {page2}"
    );
    let deeper = page2["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    assert_eq!(deeper["matchPage"], 2, "{page2}");
    assert_eq!(deeper["page"], 2, "{page2}");
    let page2b = run(&root.0, deeper.clone()).expect("page2 matchPage2");
    assert_eq!(
        page2b["files"][0]["matches"].as_array().map(Vec::len),
        Some(1),
        "{page2b}"
    );

    // A nextPage continuation always restarts per-file match pagination.
    let mut mid = base;
    mid["matchPage"] = json!(2);
    mid["snapshot"] = page1["snapshot"].clone();
    let mid = run(&root.0, mid).expect("page1 matchPage2");
    assert_eq!(
        mid["next"]["nextPage"]["query"]["queries"][0]["matchPage"], 1,
        "{mid}"
    );
}

#[test]
fn single_file_with_unreturned_matches_is_not_complete() {
    let root = Fixture::new();
    let source = root.0.join("many.rs");
    std::fs::write(&source, "fn m() { hit(1); hit(2); hit(3); }\n").expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"hit($A)","matchPageSize":2}),
    )
    .expect("match");
    assert!(out["next"]["nextMatchPage"].is_object(), "{out}");
}

/// A later match page lists only files it still has matches for; the
/// per-file page facts no longer restate the continuation or the counts.
#[test]
fn a_later_match_page_drops_files_whose_matches_ended() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() { hit(1); hit(2); hit(3); }\n");
    root.write("b.rs", "fn b() { hit(4); }\n");
    let first = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"hit($A)","matchPageSize":2}),
    )
    .expect("page 1");
    let files = first["files"].as_array().expect("files");
    assert_eq!(files.len(), 2, "{first}");
    let a = files
        .iter()
        .find(|file| file["path"].as_str().is_some_and(|p| p.ends_with("a.rs")))
        .expect("a");
    assert!(
        a.get("totalMatchRows").is_none() && a.get("returnedMatchRows").is_none(),
        "{a}"
    );
    assert!(a["pagination"].get("nextMatchPage").is_none(), "{a}");
    let next = first["next"]["nextMatchPage"].clone();
    let second = run(&root.0, next["query"].clone()).expect("match page 2");
    let files = second["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{second}");
    assert_eq!(
        files[0]["matches"].as_array().map(Vec::len),
        Some(1),
        "{second}"
    );
    let seen = |out: &serde_json::Value| {
        out["files"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|file| {
                let path = file["path"].as_str().unwrap_or_default().to_owned();
                file["matches"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(move |row| (path.clone(), row["line"].clone(), row["value"].clone()))
            })
            .collect::<Vec<_>>()
    };
    let (one, two) = (seen(&first), seen(&second));
    assert!(two.iter().all(|row| !one.contains(row)), "{first} {second}");
}

/// A file whose matches continue on a later match page names why the row is
/// partial: `matchPages`, beside `next.nextMatchPage`.
#[test]
fn a_file_with_more_match_pages_names_its_partial_reason() {
    let root = Fixture::new();
    root.write("a.rs", "fn a() { hit(1); hit(2); hit(3); }\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"hit($A)","matchPageSize":2}),
    )
    .expect("match");
    assert!(out["next"].get("nextMatchPage").is_some(), "{out}");
    assert_eq!(out["partialReasons"], json!(["matchPages"]), "{out}");
    let last = run(
        &root.0,
        out["next"]["nextMatchPage"]["query"]["queries"][0].clone(),
    )
    .expect("last match page");
    assert!(last.get("partialReasons").is_none(), "{last}");
}

/// A page number without a snapshot asks for that page of the current
/// result in every astSearch variant (match, symbols of a file and of a
/// directory, syntaxTree), as in structureSearch: never `staleSnapshot`.
/// Only a snapshot that no longer matches is stale.
#[test]
fn a_page_without_a_snapshot_pages_the_current_result_in_every_variant() {
    let root = Fixture::new();
    for name in ["a.ts", "b.ts", "c.ts"] {
        root.write(
            name,
            "export function one() { call(1); }\nexport function two() { call(2); }\nexport function three() { call(3); }\n",
        );
    }
    let file = root.0.join("a.ts");
    for query in [
        json!({"operation":"match","path":root.0,"language":"typescript","pattern":"call($A)","pageSize":1}),
        json!({"operation":"symbols","path":file,"pageSize":1}),
        json!({"operation":"symbols","path":root.0,"pageSize":1}),
        json!({"operation":"syntaxTree","path":file,"pageSize":5}),
    ] {
        let first = run(&root.0, query.clone()).expect("first page");
        let next = first["next"]["nextPage"]["query"]["queries"][0].clone();
        assert!(next.is_object(), "{first}");
        let mut bare = next.clone();
        bare.as_object_mut().expect("row").remove("snapshot");
        let pinned = run(&root.0, next.clone()).expect("pinned page 2");
        let fresh = run(&root.0, bare).expect("bare page 2");
        assert!(fresh.get("errorCode").is_none(), "{query}: {fresh}");
        assert_eq!(
            fresh["pagination"], pinned["pagination"],
            "{query}: {fresh}"
        );
        let mut wrong = next;
        wrong["snapshot"] = json!("not-the-snapshot");
        let stale = run(&root.0, wrong).expect("stale");
        assert_eq!(stale["errorCode"], "staleSnapshot", "{query}: {stale}");
    }
}
