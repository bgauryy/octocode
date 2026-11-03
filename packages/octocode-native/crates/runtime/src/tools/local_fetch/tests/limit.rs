//! `limit` is lines per page: a page ends at `limit` lines or at the byte
//! budget, whichever comes first; omitted, the budget alone cuts pages
//! (today's pages); a walk joins into the source exactly once.
use super::*;

/// Every page of `query` (following `next.nextPage`): each page's text.
fn walk(query: LocalFetchQuery, paths: &Paths) -> Vec<String> {
    let mut query = query;
    let mut pages = Vec::new();
    loop {
        assert!(pages.len() < 200, "runaway paging");
        let page = fetch(&query, paths);
        assert_eq!(page.error_code, None, "{page:?}");
        pages.push(page.content.clone().unwrap_or_default());
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => {
                assert_eq!(
                    local_fetch_query::line_limit(&next.query),
                    local_fetch_query::line_limit(&query),
                    "the cursor carries limit"
                );
                query = next.query;
            }
            None => break,
        }
    }
    pages
}

#[test]
fn a_limit_bounds_each_line_page_and_the_walk_joins_the_source_once() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let source = numbered(23);
    let path = t.write("lines.txt", source.clone());
    assert_eq!(
        walk(q(&path), &paths),
        std::slice::from_ref(&source),
        "omitted limit: the whole file, as today"
    );
    for limit in [1usize, 5, 23, 40] {
        let pages = walk(qj(&path, serde_json::json!({"limit": limit})), &paths);
        assert!(
            pages
                .iter()
                .all(|page| (1..=limit).contains(&page.lines().count())),
            "limit {limit}: {pages:?}"
        );
        assert_eq!(
            pages.len(),
            23usize.div_ceil(limit),
            "the budget allows the limit"
        );
        assert_eq!(pages.concat(), source, "limit {limit}: every line once");
    }
}

#[test]
fn ranges_pages_hold_at_most_limit_lines() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let path = t.write("lines.txt", numbered(40));
    let pages = walk(
        qj(
            &path,
            serde_json::json!({"limit": 4, "ranges": [{"startLine": 3, "endLine": 12}]}),
        ),
        &paths,
    );
    assert!(
        pages
            .iter()
            .all(|page| (1..=4).contains(&page.lines().count())),
        "{pages:?}"
    );
    assert_eq!(
        pages.concat(),
        (3..=12).map(|i| format!("l{i}\n")).collect::<String>()
    );
}

/// A page the response budget cut short of `limit` says so; a page that
/// reaches `limit`, or ends the source, does not.
#[test]
fn a_page_cut_short_of_limit_by_the_response_budget_says_why() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let path = t.write("lines.txt", numbered(4000));
    let limited = |limit: usize, window: Option<usize>| {
        let query = qj(&path, serde_json::json!({"limit": limit}));
        execute_local_fetch(&query, &paths, &Safe, &NeverCancel, window, None)
    };
    let budgeted = limited(2000, Some(2_000));
    let held = budgeted
        .content
        .as_deref()
        .unwrap_or_default()
        .lines()
        .count();
    assert!(held < 2000 && budgeted.pagination.is_some(), "{budgeted:?}");
    assert!(
        budgeted
            .hints
            .iter()
            .any(|hint| hint.contains(&format!("holds {held} lines"))
                && hint.contains("response budget")),
        "{:?}",
        budgeted.hints
    );
    let reached = limited(10, Some(2_000));
    assert!(reached.hints.is_empty(), "{:?}", reached.hints);
    let short = t.write("short.txt", numbered(100));
    let last = execute_local_fetch(
        &qj(&short, serde_json::json!({"limit": 5000})),
        &paths,
        &Safe,
        &NeverCancel,
        None,
        None,
    );
    assert!(last.hints.is_empty(), "{:?}", last.hints);
}

/// Paging is not partial coverage (core: "more pages show as
/// pagination.hasMore with next.*, not as isPartial"): a page the budget or
/// `limit` ended has a reachable `next.nextPage` and no `isPartial`.
#[test]
fn a_page_with_a_reachable_continuation_is_not_partial() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let path = t.write("lines.txt", numbered(4000));
    for (limit, window) in [(2000, Some(2_000)), (10, Some(2_000)), (5000, None)] {
        let query = qj(&path, serde_json::json!({"limit": limit}));
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, window, None);
        let pagination = page.pagination.as_ref().expect("pagination");
        assert!(pagination.has_more, "limit {limit}: {page:?}");
        assert!(
            page.next
                .as_ref()
                .is_some_and(|next| next.continue_window.is_some()),
            "limit {limit}: {page:?}"
        );
        assert_eq!(page.is_partial, None, "limit {limit}: {page:?}");
    }
}
