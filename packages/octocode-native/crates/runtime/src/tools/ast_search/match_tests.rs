//! astSearch `match`: file and match pages, clipped values, captures,
//! scan windows and the reads a match offers.
use super::test_rows::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, simple_policy};
use octocode_contract_types::ErrorCode;
use serde_json::json;

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

#[test]
fn a_match_offers_a_read_of_the_top_files_hits() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "fn a() {\n    hit(1);\n}\n\n\n\n\n\n\n\n\nfn b() { hit(2); }\n",
    );
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"hit($A)"}),
    )
    .expect("match");
    let read = &out["next"]["read"];
    assert_eq!(read["tool"], "localFetch", "{out}");
    let row = &read["query"]["queries"][0];
    assert_eq!(row["ranges"], json!(["1-5", "9-15"]), "{out}");
    assert!(
        row["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("a.rs") && std::path::Path::new(path).is_absolute()),
        "{out}"
    );
    crate::contracts::validate_query("localFetch", row.clone()).expect("a valid localFetch row");
}

/// A multi-line match shows only its header in the row, so the top read
/// covers the whole match (with context), not a window around its first line.
#[test]
fn a_multi_line_match_read_covers_the_whole_match() {
    let root = Fixture::new();
    let body = (1..=8)
        .map(|n| format!("    step({n});\n"))
        .collect::<String>();
    root.write("a.rs", format!("// head\nfn run() {{\n{body}}}\n"));
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0,"language":"rust","pattern":"fn run() { $$$BODY }"}),
    )
    .expect("match");
    let row = &out["next"]["read"]["query"]["queries"][0];
    assert_eq!(row["ranges"], json!(["1-14"]), "{out}");
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

#[test]
fn match_content_length_bounds_each_match_value() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    let args = (0..200)
        .map(|i| format!("arg{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(&source, format!("fn m() {{ call({args}); }}\n")).expect("source");
    let value_len = |length: Option<u32>| {
        let mut query = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"call($$$A)"});
        if let Some(length) = length {
            query["matchContentLength"] = json!(length);
        }
        let out = run(&root.0, query).expect("match");
        lean_row(&out["files"][0]["matches"][0]).2.chars().count()
    };
    assert_eq!(value_len(Some(40)), 40);
    assert!(value_len(Some(5_000)) > 300);
    assert_eq!(value_len(None), 500);
}

/// A value cut at `matchContentLength` is display clipping, not a coverage
/// gap: every match is listed, so the row is not partial. The cut is never
/// silent: the shown text ends in `…`, an object row says `truncated`, and
/// `next.expandValues` reads exactly the clipped rows' lines whole.
#[test]
fn clipped_values_do_not_mark_the_row_partial() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    let args = (0..200)
        .map(|i| format!("arg{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        &source,
        format!("fn n() {{ call(x); }}\nfn m() {{\n    call({args});\n}}\n"),
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"call($$$A)","captureText":true}),
    )
    .expect("match");
    assert!(out.get("isPartial").is_none(), "{out}");
    assert!(out.get("partialReasons").is_none(), "{out}");
    let rows = out["files"][0]["matches"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{out}");
    assert!(rows[0].get("clipped").is_none(), "{out}");
    assert_eq!(rows[1]["clipped"], true, "{out}");
    assert!(
        rows[1]["value"].as_str().is_some_and(|v| v.ends_with('…')),
        "{out}"
    );
    let reads = value_reads(&out);
    assert_eq!(reads.len(), 1, "{out}");
    assert_eq!(reads[0]["ranges"], json!(["3-3"]), "{out}");
    assert!(
        reads[0]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("long.rs")),
        "{out}"
    );
    // The lean view hides the `$$$A` capture as well: one expandCaptures
    // re-run returns captures and whole values; it is still not partial.
    let lean = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"call($$$A)"}),
    )
    .expect("lean match");
    assert!(lean.get("isPartial").is_none(), "{lean}");
    let expand = &lean["next"]["expandCaptures"]["query"]["queries"][0];
    assert_eq!(expand["captureText"], true, "{lean}");
    let full = format!("call({args})");
    assert_eq!(expand["matchContentLength"], full.chars().count(), "{lean}");
    let expanded = run(&root.0, expand.clone()).expect("expanded");
    assert_eq!(expanded["files"][0]["matches"][1]["value"], full.as_str());
    assert!(only_read_left(&expanded), "{expanded}");
}

/// A match row with every file and match page shown and no scope gap is
/// `coverage.state: "complete"`: its expansions (`expandCaptures`, `expandValues`) and the
/// hits read are drill-downs, so the response stage does not mark it
/// partial. A file page left, or a scope gap, keeps it open.
#[test]
fn match_rows_without_unread_pages_are_complete() {
    let root = Fixture::new();
    let long = format!("call({});\n", "y".repeat(3000));
    root.write("a.ts", &long);
    root.write("b.ts", "call(1);\n");
    let base =
        json!({"operation":"match","path":root.0,"pattern":"call($$$A)","language":"typescript"});
    let out = run(&root.0, base.clone()).expect("match");
    assert_eq!(out["coverage"]["state"], "complete", "{out}");
    assert!(out["next"].get("expandCaptures").is_some(), "{out}");
    let row = crate::response::rows::result_row(
        crate::id::ToolId::AstSearch,
        0,
        &base,
        out.clone(),
        None,
    );
    assert!(row.pointer("/data/isPartial").is_none(), "{row}");
    let mut paged = base.clone();
    paged["pageSize"] = json!(1);
    let open = run(&root.0, paged).expect("first file page");
    assert!(open.get("coverage").is_none(), "{open}");
    let mut capped = base;
    capped["maxFiles"] = json!(1);
    let gap = run(&root.0, capped).expect("capped scan");
    assert!(gap.get("coverage").is_none(), "{gap}");
}

/// Clipped rows across files and past the read-range limit each stay
/// reachable: one `expandValues` row per file and per range batch.
#[test]
fn clipped_values_read_per_file_in_range_batches() {
    let root = Fixture::new();
    let long = format!("call({});", "y".repeat(80));
    let many = (0..30)
        .map(|i| {
            if i % 2 == 0 {
                long.clone()
            } else {
                format!("// {i}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    root.write("a.rs", format!("fn a() {{\n{many}\n}}\n"));
    root.write("b.rs", format!("fn b() {{\n{long}\n}}\n"));
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","pattern":"call($A)","captureText":true,"matchContentLength":20}),
    )
    .expect("match");
    assert!(out.get("isPartial").is_none(), "{out}");
    let reads = value_reads(&out);
    let ranges = reads
        .iter()
        .flat_map(|row| row["ranges"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    // 15 clipped calls in a.rs (lines 2, 4, … 30) and one in b.rs (line 2).
    assert_eq!(ranges.len(), 16, "{out}");
    let max = crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
    assert!(
        reads
            .iter()
            .all(|row| row["ranges"].as_array().map_or(0, Vec::len) <= max),
        "{out}"
    );
    assert!(reads.len() >= 3, "{out}");
}

/// A value longer than the `matchContentLength` maximum still reads whole:
/// the lead is a line read, not a longer re-run that would cut again.
#[test]
fn values_past_the_content_maximum_read_their_lines() {
    let root = Fixture::new();
    let source = root.0.join("huge.rs");
    // Few long arguments keep the value past the content maximum while the
    // matcher's work (one node per argument) stays far inside its deadline
    // on a loaded machine.
    let args = (0..400)
        .map(|i| format!("argument_with_a_long_descriptive_name_{i:04}"))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(&source, format!("fn m() {{\n    call({args});\n}}\n")).expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"call($$$A)","captureText":true}),
    )
    .expect("match");
    assert!(out.get("isPartial").is_none(), "{out}");
    assert_eq!(value_reads(&out)[0]["ranges"], json!(["2-2"]), "{out}");
    let codes = out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| d["code"].as_str())
        .collect::<Vec<_>>();
    assert!(!codes.contains(&"structural.match.valueClipped"), "{out}");
}

/// A cut header row (a whole block) expands its captures and whole text in
/// one re-run; a lean cut without hidden captures reads its lines.
#[test]
fn block_and_lean_cuts_expand_to_the_whole_text() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    std::fs::write(&source, "fn m() { call(1); }\nfn n() { call(x); }\n").expect("source");
    let lean = run(
        &root.0,
        json!({"operation":"match","path":source,"pattern":"fn n() { call(x); }","matchContentLength":8}),
    )
    .expect("lean cut");
    assert!(lean.get("isPartial").is_none(), "{lean}");
    assert!(lean["next"].get("expandCaptures").is_none(), "{lean}");
    assert_eq!(value_reads(&lean)[0]["ranges"], json!(["2-2"]), "{lean}");
    let block = root.0.join("block.rs");
    let body = (0..120)
        .map(|i| format!("    let v{i} = {i};"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&block, format!("fn big() {{\n{body}\n}}\n")).expect("block");
    let out = run(
        &root.0,
        json!({"operation":"match","mainGoal":"test","reasoning":"test","path":block,"pattern":"fn $N() { $$$B }"}),
    )
    .expect("block match");
    assert!(value_reads(&out).is_empty(), "{out}");
    let expand = out["next"]["expandCaptures"]["query"]["queries"][0].clone();
    assert_eq!(expand["captureText"], true, "{out}");
    let expanded = run(&root.0, expand).expect("expanded block");
    let value = expanded["files"][0]["matches"][0]["value"]
        .as_str()
        .expect("value");
    assert_eq!(value, format!("fn big() {{\n{body}\n}}"));
    assert!(only_read_left(&expanded), "{expanded}");
}

#[test]
fn match_rows_withhold_captures_by_default_and_omit_single_line_ends() {
    let root = Fixture::new();
    let source = root.0.join("calls.ts");
    std::fs::write(&source, "oldCall(one);\noldCall(\n  two\n);\n").expect("source");
    let query = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,"pattern":"oldCall($A)"});
    let out = run(&root.0, query.clone()).expect("match");
    let matches = out["files"][0]["matches"].as_array().expect("matches");
    // Lean rows (objects): the line and 1-based column (a span adds its
    // end line) and exact source text; no captures.
    assert_eq!(
        matches,
        &vec![
            json!({"line":1,"column":1,"value":"oldCall(one)"}),
            json!({"line":2,"column":1,"endLine":4,"value":"oldCall(\n  two\n)"})
        ],
        "{out}"
    );
    // Equal per-file counts repeat `matches.len()`.
    assert!(out["files"][0].get("totalMatchRows").is_none(), "{out}");
    // Both $A captures show in the values: nothing to expand.
    assert!(
        out.get("next")
            .is_none_or(|next| next.get("expandCaptures").is_none()),
        "{out}"
    );
    // A value cut short hides its capture, which stays one exact
    // continuation away.
    let mut cut = query.clone();
    cut["matchContentLength"] = json!(6);
    let out = run(&root.0, cut).expect("match");
    let expand = &out["next"]["expandCaptures"]["query"]["queries"][0];
    assert_eq!(expand["captureText"], true, "{out}");

    let mut expanded = query;
    expanded["captureText"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let single = &out["files"][0]["matches"][0];
    assert_eq!(single["line"], 1, "{single}");
    assert_eq!(single["column"], 1, "{single}");
    assert!(single.get("endLine").is_none(), "{single}");
    assert!(
        single.get("endColumn").is_none(),
        "single-line end: {single}"
    );
    let span = &out["files"][0]["matches"][1];
    assert_eq!(span["endLine"], 4, "{span}");
    assert!(span["endColumn"].is_u64(), "{span}");
    assert!(
        single.get("metavars").is_none(),
        "duplicate capture map: {single}"
    );
    assert_eq!(single["metavarRanges"]["A"][0]["text"], "one", "{single}");
    assert!(
        single["metavarRanges"]["A"][0].get("endLine").is_none(),
        "{single}"
    );
    assert!(out["next"].get("expandCaptures").is_none(), "{out}");
}

#[test]
fn list_captures_are_withheld_by_default_and_expand_on_request() {
    let root = Fixture::new();
    let source = root.0.join("body.rs");
    std::fs::write(
        &source,
        "fn a() -> u8 {\n    let x = 1;\n    let y = 2;\n    x + y\n}\n",
    )
    .expect("source");
    let query = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,
        "pattern":"fn $N() -> u8 { $$$B }"});
    let out = run(&root.0, query.clone()).expect("match");
    // A lean row: no capture dump, the header stands in for the body.
    let (line, end, value) = lean_row(&out["files"][0]["matches"][0]);
    assert_eq!((line, end, value), (1, Some(5), "fn a() -> u8 …"), "{out}");
    assert!(out["next"]["expandCaptures"].is_object(), "{out}");

    let mut expanded = query;
    expanded["captureText"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let m = &out["files"][0]["matches"][0];
    let body = m["metavarRanges"]["B"].as_array().expect("B ranges");
    assert_eq!(body.len(), 3, "{out}");
    assert_eq!(body[0]["text"], "let x = 1;", "{out}");
    assert_eq!(body[0]["line"], 2, "{out}");
    assert_eq!(m["metavarRanges"]["N"][0]["text"], "a", "{m}");
}

/// A single-file match parses the file as a directory scan does: raw.
/// Redacting first shifted columns and could erase a matched row; values
/// are still redacted in the output.
#[test]
fn single_file_match_keeps_redacted_rows_and_directory_positions() {
    let root = Fixture::new();
    let source = root.0.join("k.ts");
    std::fs::write(
        &source,
        "export const A = \"plain\"; hit(1);\nexport const GITHUB_TOKEN = \"ghp_aBcDeFgHiJkLmNoPqRsTuVwXyZ0123456789\"; hit(2);\n",
    )
    .expect("source");
    let rows = |path: &std::path::Path| {
        let out = run(
            &root.0,
            json!({"operation":"match","mainGoal":"test","reasoning":"test","path":path,"language":"typescript","pattern":"export const $N = $V"}),
        )
        .expect("match");
        let file = out["files"][0].clone();
        assert!(file.is_object(), "{out}");
        file["matches"].as_array().expect("matches").clone()
    };
    let single = rows(&source);
    let directory = rows(&root.0);
    assert_eq!(single.len(), 2, "{single:?}");
    assert_eq!(single, directory);
}

/// A multi-line match with a body block shows its header (text before the
/// body) with line/endLine anchors, not the flattened body; captureText:true
/// (next.expandCaptures) returns the whole match text.
#[test]
fn block_matches_default_to_their_header_and_expand_on_request() {
    let root = Fixture::new();
    let source = root.0.join("lib.rs");
    std::fs::write(
        &source,
        "pub fn load(\n    path: &str,\n) -> Result<u8, String> {\n    let raw = read(path)?;\n    parse(raw)\n}\n",
    )
    .expect("source");
    let query = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":source,
        "rule":{"kind":"function_item"}});
    let out = run(&root.0, query.clone()).expect("match");
    assert_eq!(
        out["files"][0]["matches"][0],
        json!({"line":1,"column":1,"endLine":6,"value":"pub fn load(\n    path: &str,\n) -> Result<u8, String> …","clipped":true}),
        "{out}"
    );
    assert!(out["next"]["expandCaptures"].is_object(), "{out}");

    let mut expanded = query;
    expanded["captureText"] = json!(true);
    let out = run(&root.0, expanded).expect("match");
    let value = out["files"][0]["matches"][0]["value"]
        .as_str()
        .expect("value");
    assert_eq!(
        value,
        "pub fn load(\n    path: &str,\n) -> Result<u8, String> {\n    let raw = read(path)?;\n    parse(raw)\n}"
    );
    assert!(out["next"].get("expandCaptures").is_none(), "{out}");
}

/// `next.expandScan` resumes after the files its window evaluated, so
/// the chain of pages and widened scans lists every match and declaration
/// exactly once, under each sort.
#[test]
fn expanded_scans_reach_every_file_exactly_once() {
    let root = Fixture::new();
    for dir in ["a", "a/deep", "b"] {
        std::fs::create_dir_all(root.0.join(dir)).expect("dir");
        for n in 0..5 {
            let body = "pub fn source() {}\n".repeat(n + 1);
            root.write(&format!("{dir}/f{n}.rs"), body);
        }
    }
    let queries = [
        serde_json::json!({"operation":"match","path":root.0,"language":"rust","pattern":"pub fn source() {}","pageSize":2}),
        serde_json::json!({"operation":"match","path":root.0,"language":"rust","pattern":"pub fn source() {}","pageSize":2,"sort":"matchCount"}),
        serde_json::json!({"operation":"symbols","path":root.0,"pageSize":4}),
    ];
    for query in queries {
        let whole = ast_rows(&ast_chain(&root.0, query.clone()));
        assert_eq!(whole.len(), 45, "{query}");
        let mut cut = query.clone();
        cut["maxFiles"] = serde_json::json!(2);
        let pages = ast_chain(&root.0, cut);
        assert!(pages.len() > 4, "{query}");
        assert!(
            pages
                .iter()
                .any(|page| page["next"]["expandScan"]["query"]["queries"][0]["scanOffset"] == 8),
            "{query}"
        );
        let mut walked = ast_rows(&pages);
        let mut expected = whole.clone();
        walked.sort();
        expected.sort();
        assert_eq!(walked, expected, "{query}: no duplicate, no gap");
    }
}

/// The happy-path rows carry exactly what the next hop takes: a file
/// outline names its file once and each declaration as an object
/// `{symbolName, kind, line, endLine}`; a directory outline groups under each
/// file's `path` without restating the queried directory; a match row is
/// `{line, endLine?, value}` (no `endLine` on a single-line match).
#[test]
fn happy_path_rows_carry_the_next_hop_anchors() {
    let root = Fixture::new();
    std::fs::create_dir_all(root.0.join("src")).expect("src");
    let file = root.0.join("src/lib.rs");
    std::fs::write(
        &file,
        "pub fn alpha() {\n    beta(1);\n}\n\nfn beta(x: u8) {}\n",
    )
    .expect("lib");

    let outline = run(
        &root.0,
        json!({"operation":"symbols","path":file,"symbolName":"alpha"}),
    )
    .expect("file outline");
    assert_eq!(outline["path"], "lib.rs", "{outline}");
    let row = &outline["symbols"][0];
    assert_eq!(
        (
            &row["symbolName"],
            &row["line"],
            &row["endLine"],
            &row["kind"]
        ),
        (&json!("alpha"), &json!(1), &json!(3), &json!("function")),
        "{outline}"
    );

    let directory = run(
        &root.0,
        json!({"operation":"symbols","path":root.0.join("src")}),
    )
    .expect("directory outline");
    assert!(directory.get("path").is_none(), "{directory}");
    assert_eq!(directory["files"][0]["path"], "src/lib.rs", "{directory}");
    assert!(directory["files"][0]["symbols"].is_array(), "{directory}");

    let matched = run(
        &root.0,
        json!({"operation":"match","path":file,"rule":{"any":[
            {"pattern":"fn $N($$$P) { $$$B }"},
            {"pattern":"pub fn $N($$$P) { $$$B }"}
        ]}}),
    )
    .expect("match");
    let rows = matched["files"][0]["matches"].as_array().expect("rows");
    let multi = rows.iter().find(|row| row["line"] == 1).expect("alpha");
    assert_eq!(multi["endLine"], 3, "{matched}");
    assert_eq!(multi["clipped"], true, "{matched}");
    assert!(multi["value"].is_string(), "{matched}");
    let single = rows.iter().find(|row| row["line"] == 5).expect("beta");
    assert!(single.get("endLine").is_none(), "{matched}");
    assert!(single.get("clipped").is_none(), "{matched}");
    for row in rows {
        let keys = row.as_object().expect("row").keys().collect::<Vec<_>>();
        assert!(
            keys.iter().all(
                |key| ["line", "endLine", "value", "column", "clipped"].contains(&key.as_str())
            ),
            "{row}"
        );
    }
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

/// A clipped value that spans more lines than a hit read shows (a whole
/// file's match) is not offered for a whole read: `expandValues` would put
/// the file on the page. Shorter clipped values in the same file still are.
#[test]
fn a_clipped_whole_file_value_is_not_offered_for_reading() {
    let root = Fixture::new();
    let source = root.0.join("long.rs");
    let body = (0..500)
        .map(|i| format!("    call_{i}(argument_{i});"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &source,
        format!(
            "fn m() {{\n{body}\n}}\nfn n() {{ call({}); }}\n",
            "y".repeat(600)
        ),
    )
    .expect("source");
    let out = run(
        &root.0,
        json!({"operation":"match","path":source,"rule":{"kind":"function_item"},"captureText":true}),
    )
    .expect("match");
    assert_eq!(out["matchCount"], 2, "{out}");
    let reads = value_reads(&out);
    assert_eq!(reads.len(), 1, "{out}");
    assert_eq!(reads[0]["ranges"], json!(["503-503"]), "{out}");
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

/// Clipped values in more files than one localFetch call reads are never
/// dropped: `expandValues` batches the first files, and a per-call warning
/// names each file and line range left out of it, with the clipped value too
/// long for a hit read.
#[test]
fn clipped_values_past_one_read_batch_are_named() {
    let root = Fixture::new();
    let long = format!("call({});", "y".repeat(80));
    let files = crate::response::result::rows_per_call(crate::id::ToolId::LocalFetch) + 2;
    for index in 0..files {
        root.write(&format!("f{index}.rs"), format!("fn f() {{\n{long}\n}}\n"));
    }
    // One call spanning 500+ lines: too long for a hit read.
    let args = (0..500)
        .map(|i| format!("    argument_{i},"))
        .collect::<Vec<_>>()
        .join("\n");
    root.write("z.rs", format!("fn z() {{\ncall(\n{args}\n);\n}}\n"));
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"rust","rule":{"kind":"call_expression"},
            "captureText":true,"matchContentLength":20}),
    )
    .expect("match");
    let reads = value_reads(&out);
    assert_eq!(
        reads.len(),
        crate::response::result::rows_per_call(crate::id::ToolId::LocalFetch),
        "{out}"
    );
    let read = reads
        .iter()
        .filter_map(|row| row["path"].as_str())
        .map(|path| path.rsplit('/').next().unwrap_or(path).to_owned())
        .collect::<Vec<_>>();
    let warnings = out["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{out}");
    let warning = warnings[0].as_str().expect("text");
    // Every file with a clipped value is read or named, never both.
    for index in 0..files {
        let name = format!("f{index}.rs");
        let named = warning.contains(&format!("{name} 2-2"));
        assert!(read.contains(&name) != named, "{name}: {out}");
    }
    assert!(warning.contains("z.rs 2-503"), "{out}");
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

/// A pattern that compiles in one grammar of a mixed scope cannot match the
/// files of a grammar it does not compile in: they are outside what it
/// applies to, not unread evidence. The result is whole (not partial, no
/// failure diagnostic, no terminal limit), and one warning names each such
/// language with its file count and the compiler's reason.
#[test]
fn a_pattern_that_does_not_apply_to_a_grammar_is_named_not_failed() {
    let root = Fixture::new();
    root.write("main.py", "def run(a):\n    return a\n");
    // Each spells the pattern's anchor, so the prefilter reads them all.
    for name in ["a.rs", "b.rs", "c.rs"] {
        root.write(name, "// def\nfn x() {}\n");
    }
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"pattern":"def $F($$$A):\n    $$$B","resultView":"countMatches"}),
    )
    .expect("python matches");
    assert_eq!(out["matchCount"], 1, "{out}");
    for field in [
        "isPartial",
        "partialReasons",
        "terminalLimit",
        "diagnostics",
    ] {
        assert!(out.get(field).is_none(), "{field}: {out}");
    }
    let warnings = out["warnings"].as_array().expect("one warning");
    assert_eq!(warnings.len(), 1, "{out}");
    let warning = warnings[0].as_str().expect("text");
    assert!(
        warning.starts_with("The query does not apply to rust (3 files: "),
        "{warning}"
    );
    assert!(!warning.contains("python"), "{warning}");
    // The grammar that compiles it stays the inferred scope.
    assert_eq!(out["inferredLanguage"], "python", "{out}");
}

/// A query that compiles in no grammar of the scope is still an error, and a
/// scope whose every grammar compiles it has nothing to disclose.
#[test]
fn a_pattern_that_compiles_nowhere_errors_and_one_that_compiles_everywhere_is_silent() {
    let root = Fixture::new();
    root.write("main.py", "def run(a):\n    return a\n");
    root.write("a.rs", "// def\nfn x() {}\n");
    let error = run(
        &root.0,
        json!({"operation":"match","path":root.0,"pattern":"run("}),
    )
    .expect_err("no grammar of the scope compiles it");
    assert_eq!(error.code, ErrorCode::InvalidPattern, "{error:?}");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"pattern":"$A","resultView":"files"}),
    )
    .expect("every grammar compiles it");
    assert!(out.get("warnings").is_none(), "{out}");
    assert!(out.get("isPartial").is_none(), "{out}");
}

/// A macro attribute before a method garbles the method's parse: the call
/// inside its body is no `call_expression`, so a call pattern misses it.
const GARBLED_CPP: &str = "class lexer {\n    JSON_HEDLEY_NON_NULL(2)\n    token_type scan_literal(const char_type* literal_text, const std::size_t length,\n                            token_type return_type)\n    {\n        JSON_ASSERT(char_traits<char_type>::to_char_type(current) == literal_text[0]);\n        return return_type;\n    }\n};\n";

/// The same macro attribute, with a call the parse keeps whole.
const LOCAL_ERROR_CPP: &str = "struct S {\n    JSON_HEDLEY_NON_NULL(2)\n    int scan(const char* a, int b)\n    {\n        JSON_ASSERT(a != nullptr);\n        return b;\n    }\n};\n";

/// A match over files whose recovered parse could hide a match says so, as
/// `symbols` does: partial with `parseRecovered`, and one warning with the
/// count and a few names. The count is the scan's, so every page states it;
/// a file whose error is away from the query's anchor is not counted.
#[test]
fn a_match_discloses_files_whose_recovered_parse_could_hide_a_match() {
    let root = Fixture::new();
    for name in ["a", "b", "c", "d"] {
        root.write(&format!("pkg/lib/{name}.cpp"), GARBLED_CPP);
    }
    root.write(
        "pkg/lib/clean.cpp",
        "int f() { JSON_ASSERT(1); return 2; }\n",
    );
    root.write("pkg/lib/local.cpp", LOCAL_ERROR_CPP);
    let row = json!({"operation":"match","mainGoal":"test","reasoning":"test","path":root.0.join("pkg/lib"),
        "language":"cpp","pattern":"JSON_ASSERT($$$A)","pageSize":1});
    let (paths, security) = simple_policy(&root.0);
    let first = execute_row(row, &paths, &security, &NeverCancel).expect("first page");
    assert_eq!(first["isPartial"], json!(true), "{first}");
    assert_eq!(
        first["partialReasons"],
        json!(["parseRecovered"]),
        "{first}"
    );
    let warnings = first["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{first}");
    let warning = warnings[0].as_str().expect("warning");
    assert!(
        warning.starts_with("4 files parsed with syntax errors"),
        "{warning}"
    );
    // Paths read as the rows' do after the response stage, from the
    // workspace root, so each passes to localFetch verbatim.
    assert!(
        warning.contains("(pkg/lib/a.cpp, pkg/lib/b.cpp, pkg/lib/c.cpp, …)"),
        "{warning}"
    );
    assert!(!warning.contains("d.cpp"), "{warning}");
    assert!(
        !warning.contains("local.cpp") && !warning.contains("clean.cpp"),
        "{warning}"
    );
    assert!(first.get("coverage").is_none(), "{first}");
    // Every page states the same coverage.
    let next = first["next"]["nextPage"]["query"]["queries"][0].clone();
    let second = execute_row(next, &paths, &security, &NeverCancel).expect("second page");
    assert_eq!(second["warnings"], first["warnings"], "{second}");
    assert_eq!(
        second["partialReasons"],
        json!(["parseRecovered"]),
        "{second}"
    );
}

/// A recovered parse that cannot hide a match is no gap: errors away from
/// the anchor, and a clean file, leave the answer as complete as before.
#[test]
fn recovered_syntax_away_from_the_match_is_not_disclosed() {
    let root = Fixture::new();
    root.write("local.cpp", LOCAL_ERROR_CPP);
    root.write("clean.cpp", "int f() { JSON_ASSERT(1); return 2; }\n");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"language":"cpp","pattern":"JSON_ASSERT($$$A)"}),
    )
    .expect("match runs");
    assert_eq!(out["matchCount"], 2, "{out}");
    assert!(out.get("isPartial").is_none(), "{out}");
    assert!(out.get("warnings").is_none(), "{out}");
}

/// One file's disclosure names it once; a single-file scan carries it too.
#[test]
fn a_single_file_match_discloses_its_recovered_parse() {
    let root = Fixture::new();
    root.write("src/lexer.cpp", GARBLED_CPP);
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0.join("src/lexer.cpp"),"pattern":"JSON_ASSERT($$$A)"}),
    )
    .expect("match runs");
    assert_eq!(out["partialReasons"], json!(["parseRecovered"]), "{out}");
    let warning = out["warnings"][0].as_str().expect("warning");
    assert!(
        warning.starts_with("1 file parsed with syntax errors"),
        "{warning}"
    );
    assert!(warning.contains("(src/lexer.cpp)"), "{warning}");
}
