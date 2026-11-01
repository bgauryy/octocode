//! `limit` is rows per page: a page ends at `limit` entries or at the
//! response budget, whichever comes first; omitted, the budget alone cuts
//! pages (today's pages); a walk lists every entry exactly once.
use super::*;

fn fixture(files: usize) -> Fixture {
    let root = Fixture::new();
    for index in 0..files {
        root.write(&format!("f{index:02}.rs"), "x\n");
    }
    root
}

fn query(root: &Fixture, limit: Option<u64>) -> Value {
    let mut query = json!({"operation":"files","path":root.0,"entryType":"file","view":"paths"});
    if let Some(limit) = limit {
        query["limit"] = json!(limit);
    }
    query
}

#[test]
fn a_limit_bounds_each_page_and_the_walk_lists_every_entry_once() {
    let root = fixture(7);
    for limit in [1, 3, 7, 50] {
        // `walk_pages` fails on an entry listed twice or a repeated call.
        let pages = walk_pages(&root.0, query(&root, Some(limit)));
        let counts = pages.iter().map(|out| paths(out).len()).collect::<Vec<_>>();
        assert!(
            counts
                .iter()
                .all(|count| (1..=limit as usize).contains(count)),
            "limit {limit}: {counts:?}"
        );
        assert_eq!(counts.iter().sum::<usize>(), 7, "limit {limit}");
        assert_eq!(
            pages.len(),
            7usize.div_ceil(limit as usize),
            "limit {limit}: the budget allows the limit"
        );
    }
}

#[test]
fn omitted_limit_keeps_the_budget_cut_pages() {
    let root = fixture(7);
    let pages = walk_pages(&root.0, query(&root, None));
    assert_eq!(pages.len(), 1, "seven short paths fit one default page");
    assert_eq!(paths(&pages[0]).len(), 7);
    // A tiny budget still cuts one entry per page, limit or not.
    let tight = walk_pages_budget(&root.0, query(&root, Some(5)), Some(1));
    assert!(
        tight.iter().all(|out| paths(out).len() == 1),
        "at least one row per page"
    );
    assert_eq!(tight.iter().map(|out| paths(out).len()).sum::<usize>(), 7);
}

/// The walk window stays internal: a limited page's `continueWalk` (were
/// one needed) never rewrites `limit`, and `nextPage` carries it.
#[test]
fn the_cursor_carries_the_limit() {
    let root = fixture(5);
    let first = run(&root.0, query(&root, Some(2))).expect("page 1");
    assert_eq!(
        first["next"]["nextPage"]["input"]["queries"][0]["limit"], 2,
        "{first}"
    );
}
