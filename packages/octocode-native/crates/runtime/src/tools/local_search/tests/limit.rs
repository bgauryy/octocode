//! `limit` is rows per page: hit rows in content and matchOnly views, files
//! in the list views. A page ends at `limit` rows or at the budget,
//! whichever comes first; omitted, the budget alone cuts pages (today's
//! pages); a walk delivers every row exactly once.
use super::*;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("fixture");
    for file in 0..9 {
        let body = "needle a\nother\nneedle b\nneedle c\n".repeat(1 + file % 3);
        fs::write(root.path().join(format!("f{file}.txt")), body).expect("file");
    }
    root
}

fn start(root: &std::path::Path, view: &str, limit: Option<u64>) -> serde_json::Value {
    let mut query = serde_json::json!({
        "path": root, "matchString": ["needle"], "view": view,
        "sort": {"by": "path", "order": "asc"}
    });
    if let Some(limit) = limit {
        query["limit"] = limit.into();
    }
    query
}

/// Rows one page shows, in the unit `limit` counts.
fn page_rows(page: &serde_json::Value) -> usize {
    row_ids(page).len()
}

#[test]
fn a_limit_bounds_each_page_and_the_walk_delivers_every_row_once() {
    let root = fixture();
    for (view, rows) in [
        ("content", 54),
        ("matchOnly", 54),
        ("files", 9),
        ("countMatches", 9),
    ] {
        let (all, _) = walk_search(root.path(), start(root.path(), view, None), None);
        assert_eq!(all.rows.len(), rows, "{view}");
        for limit in [1, 4, 7] {
            // `walk_search` fails on a row delivered twice or a repeated call.
            let (walk, pages) =
                walk_search(root.path(), start(root.path(), view, Some(limit)), None);
            let counts = pages.iter().map(page_rows).collect::<Vec<_>>();
            assert!(
                counts
                    .iter()
                    .all(|count| (1..=limit as usize).contains(count)),
                "{view} limit {limit}: {counts:?}"
            );
            let mut walked = walk.rows.clone();
            walked.sort();
            let mut expected = all.rows.clone();
            expected.sort();
            assert_eq!(walked, expected, "{view} limit {limit}: every row once");
            // The default budget holds `limit` rows: only the limit ends a page.
            assert_eq!(
                pages.len(),
                rows.div_ceil(limit as usize),
                "{view} limit {limit}"
            );
            // The cursor carries the limit.
            assert!(walk.queries.iter().all(|query| {
                let opened =
                    crate::tools::test_support::opened_input(crate::id::ToolId::LocalSearch, query);
                opened["queries"][0]["limit"] == limit
            }));
        }
    }
}

#[test]
fn omitted_limit_keeps_the_budget_cut_pages_and_a_tight_budget_still_moves() {
    let root = fixture();
    let (walk, pages) = walk_search(root.path(), start(root.path(), "content", None), None);
    assert_eq!(pages.len(), 1, "the whole result fits one default page");
    assert_eq!(walk.rows.len(), 54);
    let (walk, pages) = walk_search(root.path(), start(root.path(), "files", Some(5)), Some(1));
    assert!(
        pages.iter().all(|page| page_rows(page) == 1),
        "at least one row per page"
    );
    assert_eq!(walk.rows.len(), 9);
}
