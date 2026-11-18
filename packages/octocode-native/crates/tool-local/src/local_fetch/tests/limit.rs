//! `limit` is lines per page: a page ends at `limit` lines or at the byte
//! budget, whichever comes first; omitted, the budget alone cuts pages
//! (today's pages); a walk joins into the source exactly once.
use super::*;
use octocode_test_support::json_fixture;

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
            json_fixture(r#"{"limit": 4, "ranges": [{"startLine": 3, "endLine": 12}]}"#),
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

/// A response-sized page can end before limit without prose explaining the
/// cut: pagination and executable next calls reconstruct every source line.
#[test]
fn budget_sized_pages_reconstruct_source_once_without_advice() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let source = (1..=4000)
        .map(|line| format!("l{line} café 🦀\n"))
        .collect::<String>();
    let path = t.write("lines.txt", source.clone());
    let limit = 2000;
    let mut query = qj(&path, serde_json::json!({"limit": limit}));
    let mut seen = std::collections::HashSet::new();
    let mut joined = String::new();
    let mut offset = 0;
    loop {
        assert!(
            seen.insert(serde_json::to_string(&query).expect("query")),
            "repeated next call"
        );
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(2_000), None);
        assert_eq!(page.error_code, None, "{page:?}");
        let content = page.content.as_deref().expect("selected source");
        let lines = content.lines().count();
        let pagination = page.pagination.as_ref().expect("pagination");
        assert_eq!(pagination.offset, offset, "{page:?}");
        assert_eq!(pagination.length, lines, "{page:?}");
        assert!(
            lines < limit && lines > 0,
            "response window sizes the page: {page:?}"
        );
        assert!(
            page.hints.is_empty() && page.warnings.is_empty(),
            "{page:?}"
        );
        joined.push_str(content);
        offset += lines;
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => {
                assert!(pagination.has_more, "a continued page has more");
                query = next.query;
            }
            None => {
                assert!(!pagination.has_more, "the final page ends the selection");
                break;
            }
        }
    }
    assert_eq!(joined, source, "every Unicode source line exactly once");
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

/// `limit` counts shown source lines: the `--` marker between two match
/// windows is no row, so `limit:N` over separated hits shows N hits.
#[test]
fn a_limit_over_separated_matches_counts_lines_not_markers() {
    let t = Fixture::new();
    let (paths, req) = hit_lines(&t, 60, &[3, 20, 40]);
    let mut query = req;
    amend(
        &mut query,
        json_fixture(r#"{"limit": 2, "contextLines": {"before": 0, "after": 0}}"#),
    );
    let page = fetch(&query, &paths);
    let wire = serde_json::to_value(&page).expect("serializable");
    assert_eq!(shown_hits(&wire), json_fixture(r#"[3, 20]"#), "{wire}");
}
