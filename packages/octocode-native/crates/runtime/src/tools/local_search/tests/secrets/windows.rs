//! Context windows and the default view: lean pages, merged windows, budget-sized
//! pages and accounting.

use super::*;

fn numbered(lines: u32, needles: &[u32]) -> String {
    (1..=lines)
        .map(|n| {
            if needles.contains(&n) {
                format!("line {n} needle\n")
            } else {
                format!("line {n}\n")
            }
        })
        .collect()
}

// Lean defaults: the paginated view returns only the hit line; a page takes
// each file's first 10 rows, then later rows while the window has room, so
// every file fits beside the hot one; `detailed` keeps a ±3 context window.
#[test]
fn default_paginated_view_is_lean_and_detailed_keeps_context() {
    let file = numbered(30, &(1..=12).collect::<Vec<_>>());
    let many: Vec<(String, String)> = (0..25)
        .map(|i| {
            (
                format!("f{i:02}.txt"),
                "line 1 needle\nline 2 needle\n".to_owned(),
            )
        })
        .collect();
    // A hot last file puts the result over one page budget.
    let hot = "needle\n".repeat(1200);
    let mut fixtures: Vec<(&str, &str)> = vec![("a.txt", file.as_str()), ("z.txt", &hot)];
    fixtures.extend(many.iter().map(|(p, c)| (p.as_str(), c.as_str())));
    let body = search_fixture(
        &fixtures,
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "sort": LocalSearchQuerySort::Path}),
            None,
        ),
    );
    let files = body["files"].as_array().expect("files");
    assert_eq!(files.len(), 27, "every file fits the window: {body}");
    let a = &files[0];
    assert_eq!(a["matches"].as_array().expect("rows").len(), 12, "{body}");
    assert_eq!(a["matches"][0]["value"], "line 1 needle");
    let hot = files
        .iter()
        .find(|file| file["path"] == "z.txt")
        .expect("hot file");
    // The file's total is its shown rows plus the later lines named and
    // counted: derivable, so not stated again.
    let pagination = &hot["pagination"];
    assert!(pagination.get("totalItems").is_none(), "{pagination}");
    let shown = hot["matches"].as_array().expect("rows").len();
    let listed = pagination["moreLines"].as_array().expect("moreLines").len();
    let unlisted = pagination["moreLinesUnlisted"].as_u64().unwrap_or(0) as usize;
    assert_eq!(shown + listed + unlisted, 1200, "{pagination}");
    assert!(shown < 1200);
    let detailed = search_fixture(
        &[("a.txt", &numbered(30, &[20]))],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "contextLines": 3}),
            None,
        ),
    );
    assert_eq!(
        detailed["files"][0]["matches"][0]["value"],
        "17\tline 17\n18\tline 18\n19\tline 19\n20\tline 20 needle\n21\tline 21\n22\tline 22\n23\tline 23"
    );
}

/// Following `nextPage` from a default search walks every row exactly
/// once in pages cut by the response budget: a hot file is not paged ten
/// rows at a time, and each continuation runs unchanged.
#[test]
fn a_default_walk_reaches_every_row_once_in_budget_sized_pages() {
    let root = tempfile::tempdir().expect("fixture directory");
    let rows_per_file = 60;
    for n in 0..30 {
        let body = (1..=rows_per_file)
            .map(|line| format!("needle {n} {line} {}\n", "x".repeat(60)))
            .collect::<String>();
        fs::write(root.path().join(format!("f{n:02}.txt")), body).expect("fixture");
    }
    let start = serde_json::to_value(ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned(), "matchString": "needle"}),
        None,
    ))
    .expect("query");
    let (walk, bodies) = walk_search(root.path(), start, None);
    for (index, body) in bodies.iter().enumerate() {
        let chars = octocode_engine::json_size::utf16_units(&body["files"]).unwrap_or(0);
        assert!(
            chars <= crate::response::page_windows::page_budget(None, &serde_json::json!({})),
            "page {}: {chars} chars",
            index + 1
        );
        assert!(
            body["next"].get("nextMatchPage").is_none(),
            "{}",
            body["next"]
        );
        if let Some(next) = body["next"].get("nextPage") {
            assert!(
                next["query"]["queries"][0].get("pageSize").is_none(),
                "{next}"
            );
        }
    }
    // Each row once, every continuation run unchanged.
    assert_eq!(walk.rows.len(), 30 * rows_per_file);
    let pages = bodies.len();
    // About 30 × 60 rows of ~100 bytes: the overview page plus
    // total/budget pages, not one page per ten rows of each file.
    assert!((7..=11).contains(&pages), "{pages} pages");
}

// Overlapping/adjacent ±contextLines windows are emitted once: every source
// line appears exactly once, in order, and every matched line is recorded.
#[test]
fn overlapping_context_windows_merge_into_one_block() {
    let file = numbered(30, &[5, 7, 12, 25]);
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "contextLines": 2}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    // 5 and 7 overlap (3..=9), 12 is adjacent (10..=14 follows 9), 25 is apart.
    assert_eq!(matches.len(), 2, "{body}");
    assert_eq!(matches[0]["line"], 5);
    assert_eq!(matches[0]["matchedLines"], serde_json::json!([5, 7, 12]));
    let expected: String = (3..=14)
        .map(|n| {
            if [5, 7, 12].contains(&n) {
                format!("{n}\tline {n} needle")
            } else {
                format!("{n}\tline {n}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(matches[0]["value"], expected);
    assert_eq!(matches[1]["line"], 25);
    assert!(matches[1].get("matchedLines").is_none(), "{body}");
    assert_eq!(
        matches[1]["value"],
        "23\tline 23\n24\tline 24\n25\tline 25 needle\n26\tline 26\n27\tline 27"
    );
    // Counts stay per matched line.
    assert_eq!(body["matchedLineCount"], 4);
}

#[test]
fn windows_merge_at_file_edges_and_stay_apart_when_disjoint() {
    let file = numbered(6, &[1, 2, 6]);
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "contextLines": 1}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 2, "{body}");
    assert_eq!(
        matches[0]["value"],
        "1\tline 1 needle\n2\tline 2 needle\n3\tline 3"
    );
    assert_eq!(matches[0]["matchedLines"], serde_json::json!([1, 2]));
    assert_eq!(matches[1]["value"], "5\tline 5\n6\tline 6 needle");
    // matchOnly never merges: it carries spans, not windows.
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 3, "{body}");
    assert!(matches.iter().all(|m| m.get("matchedLines").is_none()));
}

// A truncated window must never be merged (its line structure is unknown).
#[test]
fn truncated_windows_are_not_merged() {
    let long = "x".repeat(200);
    let file = format!("{long}\nneedle a\n{long}\nneedle b\n{long}\n");
    let body = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "needle".to_string(), "matchContentLength": 100}),
            None,
        ),
    );
    let matches = body["files"][0]["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 2, "{body}");
    assert!(matches.iter().all(|m| m.get("matchedLines").is_none()));
}

// Single-page results carry no redundant accounting: no engine constant,
// no per-file row counters, and a pagination block only where something
// continues (LS-3: rows left in a shown file count, so the block's `hasMore`
// agrees with `next.nextMatchPage`); per-file pagination only for a file
// with more match pages.
#[test]
fn single_page_results_omit_redundant_accounting() {
    let body = search_fixture(
        &[("a.txt", "foo\n"), ("b.txt", &"foo\n".repeat(3))],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "matchPageSize": 2, "contextLines": 0}),
            None,
        ),
    );
    assert!(body.get("searchEngine").is_none(), "{body}");
    assert_eq!(body["pagination"]["hasMore"], true, "{body}");
    assert_eq!(body["pagination"]["totalPages"], 1, "{body}");
    let finished = search_fixture(
        &[("a.txt", "foo\n"), ("b.txt", &"foo\n".repeat(2))],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "matchPageSize": 2, "contextLines": 0}),
            None,
        ),
    );
    assert!(finished.get("pagination").is_none(), "{finished}");
    let files = body["files"].as_array().expect("files");
    for file in files {
        assert!(file.get("totalMatchRows").is_none(), "{file}");
        assert!(file.get("returnedMatchRows").is_none(), "{file}");
    }
    let b = files.iter().find(|f| f["path"] == "b.txt").expect("b");
    let a = files.iter().find(|f| f["path"] == "a.txt").expect("a");
    assert!(a.get("pagination").is_none(), "{a}");
    assert_eq!(b["pagination"]["hasMore"], true);
    assert_eq!(b["pagination"]["totalItems"], 3);
    assert!(b["pagination"].get("pageSize").is_none(), "{b}");
    let next = &body["next"]["nextMatchPage"]["query"]["queries"][0];
    assert!(next["snapshot"].is_string(), "{body}");
    // Multi-page file results still carry file pagination.
    let body = search_fixture(
        &[("a.txt", "foo\n"), ("b.txt", "foo\n")],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "pageSize": 1}),
            None,
        ),
    );
    assert_eq!(body["pagination"]["totalPages"], 2, "{body}");
    assert!(
        body["next"]["nextPage"]["query"]["queries"][0]["snapshot"].is_string(),
        "{body}"
    );
}

// Continuations carry only the caller's window fields; the next call
// derives the view's defaults (the default context lines, the page size) the
// same way, so its snapshot still matches.
#[test]
fn continuations_derive_the_view_window_defaults() {
    let file = "foo\n".repeat(3);
    let make = |view| {
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "resultView": view, "matchPageSize": 1}),
            None,
        )
    };
    let body = search_fixture(
        &[("a.txt", &file)],
        make(LocalSearchQueryResultView::MatchOnly),
    );
    let next = &body["next"]["nextMatchPage"]["query"]["queries"][0];
    assert!(next.get("contextLines").is_none(), "{next}");
    let sent = search_fixture(
        &[("a.txt", &file)],
        ls_query(
            serde_json::json!({"matchString": "foo", "matchPageSize": 1, "contextLines": 0, "matchContentLength": 300}),
            None,
        ),
    );
    let next = &sent["next"]["nextMatchPage"]["query"]["queries"][0];
    assert_eq!(next["contextLines"], 0, "{next}");
    assert_eq!(next["matchContentLength"], 300, "{next}");

    let root = tempfile::tempdir().expect("fixture directory");
    fs::write(root.path().join("a.txt"), &file).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": root.path().to_string_lossy().into_owned()}),
        Some(&make(LocalSearchQueryResultView::Paginated)),
    );
    let first = execute_local_search(&request, &policy, &security, &NeverCancel, None, None, None)
        .expect("first page");
    let body = serde_json::to_value(&first).expect("serialize");
    let next = body["next"]["nextMatchPage"]["query"]["queries"][0].clone();
    // The continuation re-derives the view's defaults: neither the
    // context window nor the value width is repeated in it.
    assert!(next.get("contextLines").is_none(), "{next}");
    assert!(next.get("matchContentLength").is_none(), "{next}");
    let mut continued: LocalSearchQuery =
        serde_json::from_value(next).expect("continuation parses");
    continued.path = request.path.clone();
    execute_local_search(
        &continued,
        &policy,
        &security,
        &NeverCancel,
        None,
        None,
        None,
    )
    .expect("detailed continuation is not stale");
}
