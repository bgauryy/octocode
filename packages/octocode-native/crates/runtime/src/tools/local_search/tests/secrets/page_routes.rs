//! Page routes: every offered page is one executable query, rows show once,
//! and match pages track what earlier pages showed.

use super::super::super::local_search_query;
use super::*;

/// Every page a localSearch chain reaches when every offered `next` page
/// route is followed, breadth-first from `request`.
struct PageWalk {
    pages: usize,
    /// Each page's page-route names, in walk order.
    routes: Vec<Vec<String>>,
    /// Every shown row, `(path, line)`.
    rows: std::collections::HashSet<(String, u32)>,
}

/// Follow every `nextPage`/`nextMatchPage` of every page (breadth-first),
/// asserting that only the `nextPage` a group-ending match page re-offers
/// repeats an executable query, and no row is shown twice.
fn walk_every_page_route(
    request: &LocalSearchQuery,
    policy: &PathPolicy,
    security: &ContentSecurity,
) -> PageWalk {
    let mut queue = std::collections::VecDeque::from([request.clone()]);
    let mut offered = std::collections::HashSet::new();
    let mut walk = PageWalk {
        pages: 0,
        routes: Vec::new(),
        rows: std::collections::HashSet::new(),
    };
    while let Some(request) = queue.pop_front() {
        let result =
            select_all_search(&request, policy, security, &NeverCancel, None, None).expect("page");
        assert!(
            !result.terminal_limit,
            "a complete scan's exhausted branch is not a terminal limit: {result:?}"
        );
        if local_search_query::page(&request)
            < result
                .pagination
                .as_ref()
                .map_or(1, |page| page.total_pages)
        {
            assert_eq!(result.status, SearchStatus::Partial, "{result:?}");
        }
        walk.pages += 1;
        assert!(walk.pages < 50, "pagination must terminate");
        let match_page = local_search_query::match_page(&request).max(1);
        for file in &result.files {
            for row in file.matches.iter().flatten() {
                assert!(
                    walk.rows.insert((file.path.clone(), row.line)),
                    "repeated evidence {}:{} at page {} matchPage {match_page}",
                    file.path,
                    row.line,
                    local_search_query::page(&request)
                );
            }
        }
        let next = result.next.unwrap_or_default();
        let mut names = Vec::new();
        for name in ["nextMatchPage", "nextPage"] {
            let Some(route) = next.get(name) else {
                continue;
            };
            names.push(name.to_owned());
            let query = route["query"]["queries"][0].clone();
            if !offered.insert(crate::tools::test_support::checks::canonical(&query)) {
                assert!(
                    name == "nextPage" && match_page > 1 && next.get("nextMatchPage").is_none(),
                    "duplicate page route {name} at page {} matchPage {match_page}: {query}",
                    local_search_query::page(&request)
                );
                continue;
            }
            queue.push_back(serde_json::from_value(query).expect("continuation query"));
        }
        walk.routes.push(names);
    }
    walk
}

#[test]
fn next_page_restarts_match_rows_and_next_match_page_tracks_shown_files() {
    let many = "foo\n".repeat(5);
    let files = [
        ("a.txt", many.as_str()),
        ("b.txt", "foo\n"),
        ("c.txt", many.as_str()),
    ];
    let body = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "pageSize": 2, "matchPage": 3, "matchPageSize": 2, "contextLines": 0, "sort": LocalSearchQuerySort::Path}),
            None,
        ),
    );
    // Page 1 at matchPage 3: a.txt's 5th row is shown, nothing is left over.
    assert!(
        body["next"].get("nextMatchPage").is_none(),
        "{}",
        body["next"]
    );
    assert_eq!(
        body["next"]["nextPage"]["query"]["queries"][0]["matchPage"],
        1
    );
    let body = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "pageSize": 2, "page": 2, "matchPageSize": 2, "contextLines": 0, "sort": LocalSearchQuerySort::Path}),
            None,
        ),
    );
    assert_eq!(
        body["next"]["nextMatchPage"]["query"]["queries"][0]["matchPage"],
        2
    );
}

#[test]
fn two_axis_pages_form_one_exact_once_continuation_chain() {
    let root = tempfile::tempdir().expect("fixture");
    for (name, count) in [("a.txt", 3), ("b.txt", 2), ("c.txt", 2)] {
        std::fs::write(root.path().join(name), "foo\n".repeat(count)).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({
            "path":root.path(), "matchString":"foo", "pageSize":2,
            "matchPageSize":2, "sort":"path", "contextLines":0,
            "clasifyContext":{"classifiers":["find every foo row"]}
        }),
        None,
    );
    let walk = walk_every_page_route(&request, &policy, &security);
    // The first page has rows left in its files and files left after them:
    // both routes are offered, depth-first and breadth-first.
    assert_eq!(
        walk.routes[0],
        ["nextMatchPage", "nextPage"],
        "{:?}",
        walk.routes
    );
    assert_eq!(walk.pages, 3);
    assert_eq!(walk.rows.len(), 7);
}

/// Breadth-first and depth-first access: `nextPage` rides beside
/// `nextMatchPage` on the page that opens a file group (matchPage 1) and
/// returns on the page that ends it, so no page with later files is a dead
/// end; following every distinct offered route shows each row once.
#[test]
fn every_offered_page_route_is_one_executable_query_and_rows_show_once() {
    let root = tempfile::tempdir().expect("fixture");
    let files = [
        ("a.txt", 5),
        ("b.txt", 3),
        ("c.txt", 1),
        ("d.txt", 4),
        ("e.txt", 2),
    ];
    for (name, count) in files {
        std::fs::write(root.path().join(name), "foo\n".repeat(count)).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({
            "path":root.path(), "matchString":"foo", "pageSize":2,
            "matchPageSize":2, "sort":"path", "contextLines":0
        }),
        None,
    );
    let walk = walk_every_page_route(&request, &policy, &security);
    let total: usize = files.iter().map(|(_, count)| count).sum();
    assert_eq!(walk.rows.len(), total, "every row once");
    // File pages {a,b} {c,d} {e}: three match pages, then two, then one.
    assert_eq!(walk.pages, 6, "{:?}", walk.routes);
    // A middle match page never re-offers the next file page; the match
    // page that ends a file group does (a repeat the walk does not rerun).
    assert_eq!(
        walk.routes,
        [
            vec!["nextMatchPage", "nextPage"],
            vec!["nextMatchPage"],
            vec!["nextMatchPage", "nextPage"],
            vec!["nextPage"],
            vec!["nextPage"],
            vec![],
        ]
    );
}

/// A grid page names `nextMatchPage` while rows remain in its files, then
/// `nextPage` for later files. Row totals live once, as `matchedLineCount`
/// beside the occurrence
/// count, and a later match page says which match page it shows.
#[test]
fn grid_pages_count_the_rows_a_match_page_reaches_and_name_the_match_page() {
    let many = "foo\n".repeat(5);
    let files = [
        ("a.txt", many.as_str()),
        ("b.txt", "foo foo\n"),
        ("c.txt", many.as_str()),
    ];
    let request = |match_page: u32| {
        ls_query(
            serde_json::json!({"matchString": "foo", "pageSize": 2, "matchPage": match_page, "matchPageSize": 2, "sort": LocalSearchQuerySort::Path}),
            None,
        )
    };
    let body = search_fixture(&files, request(1));
    // The shared page stage says how many rows `nextMatchPage` reaches.
    assert!(body.get("warnings").is_none(), "{body}");
    assert!(body["next"].get("nextMatchPage").is_some(), "{body}");
    assert!(body["pagination"].get("matchCount").is_none(), "{body}");
    assert!(body["pagination"].get("matchPage").is_none(), "{body}");
    assert_eq!(body["matchedLineCount"], 11, "{body}");
    assert_eq!(body["matchCount"], 12, "{body}");
    let second = search_fixture(&files, request(2));
    assert_eq!(second["pagination"]["currentPage"], 1, "{second}");
    assert_eq!(second["pagination"]["matchPage"], 2, "{second}");
    assert!(second["next"].get("nextMatchPage").is_some(), "{second}");
}

#[test]
fn a_repeated_row_follows_every_distinct_row_of_a_clipped_file() {
    // Identical impls of one trait method rank as declarations, but the
    // second copy adds nothing over the distinct call site.
    let file = "fn needle() {\nfn needle() {\ncall(needle);\n";
    let body = search_fixture(
        &[("a.rs", file)],
        ls_query(
            serde_json::json!({"matchString": "needle", "matchPageSize": 2, "contextLines": 0}),
            None,
        ),
    );
    let lines: Vec<u64> = body["files"][0]["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .filter_map(|m| m["line"].as_u64())
        .collect();
    assert_eq!(lines, [1, 3], "{body}");
    assert_eq!(
        body["files"][0]["pagination"]["moreLines"],
        serde_json::json!([2])
    );
}

#[test]
fn line_rows_omit_the_column_that_span_rows_need() {
    let files = [("a.txt", "x needle needle\n")];
    let lines = search_fixture(
        &files,
        ls_query(serde_json::json!({"matchString": "needle"}), None),
    );
    assert!(
        lines["files"][0]["matches"][0].get("column").is_none(),
        "{lines}"
    );
    let spans = search_fixture(
        &files,
        ls_query(
            serde_json::json!({"matchString": "needle", "resultView": LocalSearchQueryResultView::MatchOnly}),
            None,
        ),
    );
    let columns: Vec<u64> = spans["files"][0]["matches"]
        .as_array()
        .expect("spans")
        .iter()
        .filter_map(|m| m["column"].as_u64())
        .collect();
    assert_eq!(columns, [3, 10], "{spans}");
}

#[test]
fn later_match_pages_omit_files_exhausted_on_earlier_pages() {
    let many = "foo\n".repeat(5);
    let files = [("a.txt", many.as_str()), ("b.txt", "foo\n")];
    let request = |match_page| {
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "matchPage": match_page, "matchPageSize": 2, "contextLines": 0, "sort": LocalSearchQuerySort::Path}),
            None,
        )
    };
    let body = search_fixture(&files, request(2));
    let paths: Vec<&str> = body["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect();
    assert_eq!(paths, ["a.txt"], "{body}");
    // Every file exhausted: keep the out-of-range diagnostic rows.
    let body = search_fixture(&files, request(9));
    assert_eq!(body["files"][0]["pagination"]["outOfRange"], true, "{body}");
}

#[test]
fn list_views_have_no_match_rows_to_page() {
    let body = search_fixture(
        &[("a.txt", &"foo\n".repeat(15))],
        ls_query(
            serde_json::json!({"matchString": "foo".to_string(), "resultView": LocalSearchQueryResultView::Files}),
            None,
        ),
    );
    assert!(body.get("next").is_none(), "{}", body["next"]);
    assert_ne!(body["status"], "partial", "{body}");
}
