//! One stream of matches in walk order: `limit` counts matches across files,
//! a walk takes one call per page, each page states the page count it walks,
//! and every match comes back exactly once.
use super::*;
use serde_json::Value;

/// Files holding 5, 1, 7, 2 and 4 matches (19), each row naming a
/// non-ASCII literal so sizes are counted in UTF-16 units, not bytes.
fn streamed_fixture() -> (Fixture, usize) {
    let root = Fixture::new();
    let counts = [5, 1, 7, 2, 4];
    for (file, count) in counts.iter().enumerate() {
        let calls = (0..*count)
            .map(|n| format!("    hit(\"é漢{file}-{n}\");\n"))
            .collect::<String>();
        root.write(&format!("f{file}.rs"), format!("fn f() {{\n{calls}}}\n"));
    }
    (root, counts.iter().sum())
}

/// A page's total: every match of the scope, or, on a window that ends
/// before the scope does or follows another (`countScope: loaded`), a
/// floor of it.
fn assert_total(pagination: &Value, total: usize) {
    let stated = pagination["total"].as_u64().expect("a total") as usize;
    match pagination.get("countScope") {
        Some(scope) => {
            assert_eq!(scope, "loaded", "{pagination}");
            assert!(stated <= total, "{pagination}");
        }
        None => assert_eq!(stated, total, "{pagination}"),
    }
}

fn query(root: &Fixture, limit: usize) -> Value {
    json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"},"limit":limit})
}

/// `limit` matches per page whichever files hold them: the walk takes
/// `ceil(matches / limit)` calls, every page states the matches as its
/// items (all of them, or a floor its `countScope` marks while windows
/// remain), and no match repeats.
#[test]
fn a_match_walk_takes_one_call_per_limit_of_matches() {
    let (root, total) = streamed_fixture();
    for limit in [1, 3, 7] {
        let pages = ast_chain(&root.0, query(&root, limit));
        let calls = total.div_ceil(limit);
        assert_eq!(pages.len(), calls, "limit {limit}: {pages:#?}");
        for page in &pages {
            let pagination = &page["pagination"];
            assert!(
                pagination["length"]
                    .as_u64()
                    .is_some_and(|length| length as usize <= limit),
                "limit {limit}: {page}"
            );
            assert_total(pagination, total);
            assert_eq!(pagination["unit"], "matches", "limit {limit}: {page}");
            let shown = page["files"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|file| file["matches"].as_array().map_or(0, Vec::len))
                .sum::<usize>();
            assert!(shown <= limit, "limit {limit}: {page}");
            assert!(
                page["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .all(|file| file.get("pagination").is_none()),
                "no per-file pages: {page}"
            );
        }
        let rows = ast_rows(&pages);
        let unique = rows.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(rows.len(), total, "limit {limit}");
        assert_eq!(unique.len(), total, "limit {limit}: a match repeats");
    }
}

/// A page whose one match outgrows the response window cannot promise a page
/// count (the response pager splits it into row parts, so calls exceed
/// pages): the one pagination shape states none, and still every match
/// comes once.
#[test]
fn a_page_over_its_budget_states_no_page_count_and_each_match_comes_once() {
    let root = Fixture::new();
    let long = "é漢".repeat(150);
    for file in 0..2 {
        let calls = (0..3)
            .map(|n| format!("    hit(\"{long}{file}-{n}\");\n"))
            .collect::<String>();
        root.write(&format!("f{file}.rs"), format!("fn f() {{\n{calls}}}\n"));
    }
    let mut pending = Some(
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"}}),
    );
    let mut spans = std::collections::BTreeSet::new();
    let mut calls = 0;
    while let Some(query) = pending.take() {
        let out = run_window(&root.0, query, 400).expect("tiny window");
        calls += 1;
        assert!(calls <= 12, "nonterminating walk: {out}");
        assert_total(&out["pagination"], 6);
        for file in out["files"].as_array().into_iter().flatten() {
            for row in file["matches"].as_array().into_iter().flatten() {
                let span = (file["path"].to_string(), row["line"].to_string());
                assert!(spans.insert(span), "repeated match: {out}");
            }
        }
        pending = out["next"]["nextPage"]["input"]["queries"].get(0).cloned();
    }
    assert_eq!(spans.len(), 6);
}
