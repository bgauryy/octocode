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
