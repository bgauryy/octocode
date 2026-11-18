//! `isPartial` on outline pages: more rows are pagination, an unread span is
//! partial.

use super::*;
use octocode_test_support::json_fixture;

/// Every page of `start`, following whichever lead reads on.
fn every_page(root: &Fixture, start: Value) -> Vec<Value> {
    let (mut pages, mut next) = (Vec::new(), Some(start));
    while let Some(row) = next {
        assert!(pages.len() < 500, "the walk must end: {row}");
        let out = run(&root.0, row).expect("page");
        next = out["next"]
            .get("nextPage")
            .or_else(|| out["next"].get("expandScan"))
            .map(|lead| lead["input"]["queries"][0].clone());
        pages.push(out);
    }
    pages
}

/// More rows of a whole outline are what `hasMore` and `next` say. `isPartial`
/// means evidence is missing, and no page here misses any.
#[test]
fn row_limit_pages_of_a_whole_outline_are_not_partial() {
    let root = Fixture::new();
    let source = root.0.join("many.ts");
    let text = (0..6)
        .map(|at| format!("export function f{at}() {{}}\n"))
        .collect::<String>();
    std::fs::write(&source, text).expect("source");
    let pages = every_page(
        &root,
        json!({"operation":"symbols","path":source,"limit":2}),
    );
    assert_eq!(pages.len(), 3, "{pages:?}");
    for (at, page) in pages.iter().enumerate() {
        assert_eq!(page["isPartial"], false, "{page}");
        assert!(page.get("partialReasons").is_none(), "{page}");
        assert_eq!(
            page["pagination"]["hasMore"],
            at + 1 < pages.len(),
            "{page}"
        );
    }
}

/// A scan cut by its clock or its memory bound leaves files unread: every page
/// of that window says so, the row-limit pages inside it too, and the window's
/// last page reads on through `expandScan`. The window that reaches the end is
/// whole.
#[test]
fn every_page_of_a_cut_scan_stays_partial_with_its_lead() {
    use crate::ast_search::scan_window::{test_memory_limit, test_window};
    let tree = |files| {
        let root = Fixture::new();
        for at in 0..files {
            root.write(
                &format!("file{at}.ts"),
                format!("export function a{at}() {{}}\nexport function b{at}() {{}}\n"),
            );
        }
        root
    };
    let row = |root: &Fixture| json!({"operation":"symbols","path":root.0,"limit":1});
    let check = |pages: &[Value]| {
        let (cut, whole) = pages
            .iter()
            .partition::<Vec<_>, _>(|page| page.get("partialReasons").is_some());
        assert!(!cut.is_empty(), "the bound must cut the scan: {pages:?}");
        for page in &cut {
            assert_eq!(page["isPartial"], true, "{page}");
            assert_eq!(
                page["partialReasons"],
                json_fixture(r#"["scanBudget"]"#),
                "{page}"
            );
            assert_eq!(page["pagination"]["hasMore"], true, "{page}");
            assert!(
                page["next"].get("nextPage").is_some() || page["next"].get("expandScan").is_some(),
                "{page}"
            );
        }
        assert!(
            cut.iter()
                .any(|page| page["next"].get("expandScan").is_some()),
            "{pages:?}"
        );
        assert!(
            cut.iter()
                .any(|page| page["next"].get("nextPage").is_some()),
            "a row-limit page inside a cut window: {pages:?}"
        );
        for page in &whole {
            assert_eq!(page["isPartial"], false, "{page}");
        }
        assert!(pages.last().is_some_and(|last| last["isPartial"] == false));
    };
    {
        let root = tree(4);
        let _window = test_window(2);
        check(&every_page(&root, row(&root)));
    }
    // Workers stop between files, so the bound needs more of them to cut.
    let root = tree(64);
    let _memory = test_memory_limit(1);
    let first = run(&root.0, row(&root)).expect("first page");
    assert_eq!(first["isPartial"], true, "{first}");
    assert_eq!(
        first["partialReasons"],
        json_fixture(r#"["scanBudget"]"#),
        "{first}"
    );
    assert_eq!(first["pagination"]["hasMore"], true, "{first}");
    assert!(first["next"].get("nextPage").is_some(), "{first}");
}
