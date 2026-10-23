//! Totals and pages: counted units, streamed pagination, reverse order and the
//! page ceiling.

use super::*;

/// Every page of a walk, from `request`: runs page 1, then each later page
/// pinned to the first page's snapshot, until the page count it reported.
fn walk_pages(files: &[(&str, &str)], request: &LocalSearchQuery) -> Vec<serde_json::Value> {
    let root = tempfile::tempdir().expect("fixture directory");
    for (name, body) in files {
        fs::write(root.path().join(name), body).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned()}),
        Some(request),
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("first page");
    let total = first.pagination.as_ref().map_or(1, |p| p.total_pages);
    let snapshot = first.source_snapshot.clone();
    let mut pages = vec![serde_json::to_value(&first).expect("serialize")];
    for page in 2..=total {
        let next = ls_query(
            serde_json::json!({"page": page, "snapshot": snapshot}),
            Some(&request),
        );
        let result =
            execute_local_search(&next, &policy, &security, &NeverCancel, None, None, None)
                .expect("later page");
        pages.push(serde_json::to_value(&result).expect("serialize"));
    }
    pages
}

/// A page holds as many files as fit the response window, in every view,
/// and names the enclosing declaration of every row it shows: 25 small
/// files are one page, not a page of 20 and a page of 5.
#[test]
fn a_small_result_of_many_files_fills_one_page_with_every_row_named() {
    let sources: Vec<(String, String)> = (0..25)
        .map(|i| {
            (
                format!("f{i:02}.rs"),
                format!("fn f{i}() {{\n    needle();\n}}\n"),
            )
        })
        .collect();
    let fixtures: Vec<(&str, &str)> = sources
        .iter()
        .map(|(name, body)| (name.as_str(), body.as_str()))
        .collect();
    for view in ["matchOnly", "paginated"] {
        let request = ls_query(
            serde_json::json!({"matchString": "needle", "resultView": view}),
            None,
        );
        let pages = walk_pages(&fixtures, &request);
        assert_eq!(pages.len(), 1, "{view}: {pages:?}");
        let counts: Vec<usize> = pages
            .iter()
            .map(|page| page["files"].as_array().expect("files").len())
            .collect();
        assert_eq!(counts, [25], "{view}: {pages:?}");
        for page in &pages {
            assert!(page.get("warnings").is_none(), "{view}: {page}");
            for file in page["files"].as_array().expect("files") {
                assert!(file["matches"][0].get("enclosing").is_some(), "{page}");
            }
        }
    }
}

/// Streamed pages are cut by the response budget, so a file can ride
/// several pages: `totalItems` counts the hit rows the pages walk (each
/// shown exactly once), never the files, and `totalPages` counts those
/// same pages.
#[test]
fn streamed_pagination_counts_pages_and_items_in_one_unit() {
    let hot = "needle\n".repeat(1200);
    let fixtures = [("a.txt", "needle\n"), ("hot.txt", hot.as_str())];
    let request = ls_query(serde_json::json!({"matchString": "needle"}), None);
    let pages = walk_pages(&fixtures, &request);
    assert_eq!(pages.len(), 2, "budget-sized pages");
    let shown: usize = pages
        .iter()
        .flat_map(|page| page["files"].as_array().expect("files"))
        .map(|file| file["matches"].as_array().expect("rows").len())
        .sum();
    assert_eq!(shown, 1201);
    for page in &pages {
        let pagination = &page["pagination"];
        assert_eq!(pagination["totalPages"], pages.len(), "{page}");
        assert_eq!(pagination["totalItems"], shown, "{page}");
        assert!(pagination.get("pageSize").is_none(), "{page}");
    }
}

/// The flat totals of a search row (`matchCount`, `filesScanned`, …) satisfy
/// the strict output contract: no `stats` object, `fileCount`, `capped` or
/// `capReason`, and an unreadable path or a limit surfaces as
/// `fileErrors` / `partialReasons`.
#[test]
fn a_search_row_serializes_flat_totals_that_satisfy_the_output_contract() {
    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("a.txt"), "needle one\nneedle two\n").expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    );
    let result = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("search");
    let mut wire = serde_json::to_value(&result).expect("serialize");
    assert_eq!(wire["matchCount"], 2, "{wire}");
    assert_eq!(wire["filesScanned"], 1, "{wire}");
    for gone in ["stats", "fileCount", "capped", "capReason", "searchTime"] {
        assert!(wire.get(gone).is_none(), "{gone}: {wire}");
    }
    crate::contracts::validate_output(
        "localSearch",
        &serde_json::json!({"results":[{"index":0,"data":wire.take()}]}),
    )
    .expect("flat totals satisfy the output contract");

    let stats = SearchStats {
        total_occurrences: Some(0),
        matched_lines: Some(0),
        files_matched: 0,
        files_searched: 3,
        bytes_searched: None,
        capped: Some(true),
        cap_reasons: Vec::new(),
        partial_reasons: vec!["maxFiles".into(), "maxOnlyMatchingPerLine".into()],
        error_count: Some(1),
        first_error: Some("x.bin: denied".into()),
        file_errors: Some(FileErrors {
            count: 1,
            first: Some("x.bin: denied".into()),
        }),
        binary_skipped: None,
    };
    let wire = serde_json::to_value(&stats).expect("serialize");
    assert_eq!(
        wire["fileErrors"],
        serde_json::json!({"count":1,"first":"x.bin: denied"})
    );
    assert_eq!(
        wire["partialReasons"],
        serde_json::json!(["maxFiles", "maxOnlyMatchingPerLine"])
    );
}

/// A line holding more matches than one page (or the old per-line cap of
/// 1000) still reaches every match: `next.nextMatchPage` walks them all.
#[test]
fn match_only_pages_every_match_of_a_line_with_thousands_of_hits() {
    let root = tempfile::tempdir().expect("fixture directory");
    let hits = 2_500;
    fs::write(
        root.path().join("min.js"),
        format!("{}\n", "needle ".repeat(hits)),
    )
    .expect("fixture");
    let (policy, security) = policy_for(root.path());
    let mut request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle", "resultView": LocalSearchQueryResultView::MatchOnly, "matchPageSize": 400}),
        None,
    );
    let mut seen = Vec::new();
    for _ in 0..20 {
        let result =
            execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
                .expect("page");
        let body = serde_json::to_value(&result).expect("serialize");
        assert!(body.get("partialReasons").is_none(), "{body}");
        for matched in body["files"][0]["matches"].as_array().expect("matches") {
            seen.push(matched["column"].as_u64().expect("column"));
        }
        let Some(next) = body["next"].get("nextMatchPage") else {
            break;
        };
        request = ls_query(next["query"]["queries"][0].clone(), Some(&request));
    }
    let expected: Vec<u64> = (0..hits as u64).map(|n| n * 7 + 1).collect();
    assert_eq!(seen, expected);
}

/// Top-level totals are stated only in units the view counted: countLines
/// counts lines, not occurrences, so it states `matchedLineCount` and no
/// `matchCount`; filesWithout lists files without a hit and states neither.
#[test]
fn totals_are_stated_only_in_the_units_a_view_counted() {
    let files = [("a.txt", "foo foo\nfoo\n"), ("b.txt", "bar\n")];
    let lines = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "countLines"}),
            None,
        ),
    );
    assert_eq!(lines["matchedLineCount"], 2, "{lines}");
    assert!(lines.get("matchCount").is_none(), "{lines}");
    let matches = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "countMatches"}),
            None,
        ),
    );
    assert_eq!(matches["matchCount"], 3, "{matches}");
    let without = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "filesWithout"}),
            None,
        ),
    );
    assert_eq!(without["files"][0]["path"], "b.txt", "{without}");
    assert!(without.get("matchCount").is_none(), "{without}");
    assert!(without.get("matchedLineCount").is_none(), "{without}");
}

/// `reverse` reverses a count view under the default relevance sort, which
/// ranks by count: the fewest hits come first.
#[test]
fn reverse_flips_a_count_view_under_relevance() {
    let files = [("a.txt", "foo\n"), ("b.txt", "foo\nfoo\nfoo\n")];
    let forward = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "countMatches"}),
            None,
        ),
    );
    assert_eq!(forward["files"][0]["path"], "b.txt", "{forward}");
    let reversed = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo", "resultView": "countMatches", "reverse": true}),
            None,
        ),
    );
    assert_eq!(reversed["files"][0]["path"], "a.txt", "{reversed}");
}

/// `reverse` is the exact reverse of the forward order, ties included.
#[test]
fn reverse_is_the_exact_reverse_of_a_count_view_including_ties() {
    let files = [
        ("a.txt", "foo\n"),
        ("b.txt", "foo\nfoo\n"),
        ("c.txt", "foo\nfoo\n"),
        ("d.txt", "foo\n"),
    ];
    for view in ["countMatches", "countLines"] {
        let order = |reverse: bool| -> Vec<String> {
            let body = search_fixture(
                &files,
                ls_query(
                    serde_json::json!({"matchString": "foo", "resultView": view, "reverse": reverse}),
                    None,
                ),
            );
            body["files"]
                .as_array()
                .expect("files")
                .iter()
                .map(|f| f["path"].as_str().unwrap_or_default().to_owned())
                .collect()
        };
        let forward = order(false);
        assert_eq!(forward, ["b.txt", "c.txt", "a.txt", "d.txt"], "{view}");
        let mut expected = forward;
        expected.reverse();
        assert_eq!(order(true), expected, "{view}");
    }
}

/// At the last page a continuation can name, file pages that remain end the
/// row at a terminal limit the shared cursor owner states.
#[test]
fn the_page_ceiling_names_the_file_pages_past_it() {
    let max = crate::contracts::query_schema_max(crate::id::ToolId::LocalSearch, None, "page");
    let root = tempfile::tempdir().expect("fixture");
    for index in 0..max + 2 {
        fs::write(root.path().join(format!("f{index:05}.txt")), "CEILING\n").expect("file");
    }
    let policy = crate::tools::test_support::workspace_policy(root.path());
    let request = ls_query(
        serde_json::json!({"path":root.path(),"matchString":"CEILING","regex":"literal","pageSize":1,"page":max,"sort":"path"}),
        None,
    );
    let result = execute_local_search(
        &request,
        &policy,
        &ContentSecurity::new(),
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("search");
    assert!(
        result
            .next
            .as_ref()
            .is_none_or(|next| next.get("nextPage").is_none()),
        "{:?}",
        result.next
    );
    let warning = format!(
        "terminalLimit: page:{max} is the last a continuation may request; the rest is on no page."
    );
    assert!(result.warnings.contains(&warning), "{:?}", result.warnings);
    assert!(result.terminal_limit && result.is_partial);
    assert_eq!(
        result.stats.partial_reasons,
        [crate::response::paging::CURSOR_MAXIMUM]
    );
}
