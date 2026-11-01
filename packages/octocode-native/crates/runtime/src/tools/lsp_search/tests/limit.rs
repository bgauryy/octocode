//! `limit` is rows (locations, symbols) per page: a page ends at `limit`
//! rows or at the budget, whichever comes first; omitted, the budget alone
//! cuts pages (today's pages); a walk reaches every row exactly once.
use super::*;

/// Every page of `first` (copying `next.nextPage` unchanged): each page's
/// row names, in order.
fn pages(first: &LspSearchQuery, items: &[serde_json::Value], budget: usize) -> Vec<Vec<String>> {
    let mut q = first.clone();
    let mut pages = Vec::new();
    for call in 0.. {
        assert!(call < 1_000, "the walk must end");
        let row = with_next(
            &q,
            items_payload(&q, "symbols", serde_json::json!(items), budget),
        );
        assert!(row.get("errorCode").is_none(), "page {call}: {row}");
        let names = ["matches", "symbols"]
            .iter()
            .filter_map(|key| row[*key].as_array())
            .flatten()
            .filter_map(|item| item["name"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        pages.push(names);
        let Some(next) = row.pointer("/next/nextPage/input/queries/0") else {
            break;
        };
        assert_eq!(
            next.get("limit"),
            lsp_search_query::to_row(first).get("limit"),
            "the cursor carries limit"
        );
        q = serde_json::from_value(next.clone()).expect("executable continuation");
    }
    pages
}

#[test]
fn a_limit_bounds_each_page_and_the_walk_reaches_every_row_once() {
    let items = (0..23)
        .map(|index| serde_json::json!({"name": format!("s{index:02}"), "detail": "x".repeat(index % 5 * 20)}))
        .collect::<Vec<_>>();
    let expected = (0..23)
        .map(|index| format!("s{index:02}"))
        .collect::<Vec<_>>();
    let row = serde_json::json!({"operation": "workspaceSymbol", "path": "file:///repo", "symbolQuery": "s"});
    let unlimited = pages(&query(row.clone()), &items, TEST_BUDGET);
    assert_eq!(
        unlimited,
        std::slice::from_ref(&expected),
        "omitted limit: one budget-cut page, as today"
    );
    for limit in [1, 4, 10] {
        let mut limited = row.clone();
        limited["limit"] = limit.into();
        let walked = pages(&query(limited.clone()), &items, TEST_BUDGET);
        assert!(
            walked.iter().all(|page| (1..=limit).contains(&page.len())),
            "limit {limit}"
        );
        assert_eq!(
            walked.len(),
            23usize.div_ceil(limit),
            "the budget allows the limit"
        );
        assert_eq!(walked.concat(), expected, "limit {limit}: every row once");
        // A budget below one row still moves one row per page.
        let tight = pages(&query(limited), &items, 1);
        assert!(tight.iter().all(|page| page.len() == 1));
        assert_eq!(tight.concat(), expected);
    }
}
