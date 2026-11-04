//! A stored walk cuts its pages once, for exactly the inputs it was cut for,
//! and is weighed by what it holds.
use super::super::cut::page_ranges;
use super::super::memo;
use super::super::rows::{self, Row};
use super::super::walked::{PageCut, Walked, cost_passes};
use super::*;
use octocode_contract_types::StructureSearchView as View;
use std::sync::Arc;

fn tree(root: &Fixture) {
    for dir in ["a", "b/deep", "c"] {
        for n in 0..7 {
            root.write(&format!("{dir}/f{n}.rs"), "x\n");
        }
    }
}

fn query(root: &Fixture) -> Value {
    json!({"operation":"files","path":root.0,"entryType":"file","view":"paths"})
}

/// The walk the first page of `query` at `budget` stored, and that page's
/// continuation.
fn stored(root: &Fixture, query: Value, budget: usize) -> (Arc<Walked>, Value) {
    let first = run_budget(&root.0, query, budget).expect("first page");
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    let snapshot = next["snapshot"].as_str().expect("snapshot");
    let (paths, _) = simple_policy(&root.0);
    let walk = memo::get(snapshot, &paths.identity()).expect("stored walk");
    (walk, next)
}

/// Every page of a stored walk reuses the cut its first page made: serving
/// one prices no row again.
#[test]
fn a_stored_walk_prices_its_rows_once_however_many_pages_it_serves() {
    let root = Fixture::new();
    tree(&root);
    let before = cost_passes();
    let pages = walk_pages_budget(&root.0, query(&root), Some(120));
    assert!(pages.len() > 3, "several pages: {}", pages.len());
    assert_eq!(cost_passes() - before, 1, "{} pages", pages.len());
}

/// The kept cut answers only for what it was cut for: asked with any other
/// prefix, view, detail reserve, budget or limit, a stored walk cuts afresh
/// and keeps what it holds.
#[test]
fn a_kept_page_cut_answers_only_for_its_own_inputs() {
    let root = Fixture::new();
    tree(&root);
    let details = json!({"operation":"files","path":root.0,"entryType":"file","view":"details"});
    let (walk, _) = stored(&root, details, 400);
    let cut = |prefix: &str, view, deferred_detail, budget, limit| PageCut {
        prefix: prefix.to_owned(),
        view,
        deferred_detail,
        budget,
        limit,
    };
    let kept = cut(".", View::Details, true, 400, None);
    let expect = |cut: &PageCut| {
        page_ranges(
            &rows::costs(&walk.rows, &cut.prefix, cut.view, cut.deferred_detail),
            cut.budget,
            cut.limit,
        )
    };
    let first = walk.pages(kept.clone()).into_owned();
    assert_eq!(first, expect(&kept));
    for other in [
        cut(
            "a/long/prefix/that/costs/more/chars/in/every/header",
            View::Details,
            true,
            400,
            None,
        ),
        cut(".", View::Paths, true, 400, None),
        cut(".", View::Details, false, 400, None),
        cut(".", View::Details, true, 800, None),
        cut(".", View::Details, true, 400, Some(1)),
    ] {
        let fresh = expect(&other);
        assert_ne!(fresh, first, "the inputs must move the cut: {other:?}");
        assert_eq!(*walk.pages(other.clone()), fresh, "{other:?}");
    }
    assert_eq!(*walk.pages(kept), first, "the kept cut stays");
}

/// One stored walk, two response budgets: a page asked at another budget
/// restarts instead of cutting the stored walk elsewhere, and either budget
/// read on its own delivers each row once, in the same order.
#[test]
fn pages_at_two_budgets_never_mix_over_one_stored_walk() {
    let root = Fixture::new();
    tree(&root);
    let (_, next) = stored(&root, query(&root), 120);
    let other = run_budget(&root.0, next, 300).expect("other budget");
    assert_eq!(other["errorCode"], "staleSnapshot", "{other}");
    let whole = paths(&run(&root.0, query(&root)).expect("whole"));
    assert_eq!(whole.len(), 21);
    for budget in [120, 300] {
        let pages = walk_pages_budget(&root.0, query(&root), Some(budget));
        assert!(pages.len() > 1, "budget {budget}");
        let read = pages.iter().flat_map(paths).collect::<Vec<_>>();
        assert_eq!(read, whole, "budget {budget}");
    }
}

/// The weight the memory budget sees covers the rows' own heap: the row
/// table's capacity and each row's strings.
#[test]
fn a_stored_walk_weighs_the_heap_its_rows_hold() {
    let root = Fixture::new();
    tree(&root);
    let (walk, _) = stored(&root, query(&root), 120);
    let strings = |row: &Row| {
        row.dir.capacity()
            + row.entry.capacity()
            + row.display_name.capacity()
            + row.path.capacity()
            + row.name.capacity()
            + row.source.capacity()
    };
    let held = walk.rows.capacity() * std::mem::size_of::<Row>()
        + walk.rows.iter().map(strings).sum::<usize>();
    assert!(
        walk.weight() >= held,
        "weight {} under the rows' {held} bytes",
        walk.weight()
    );
}

/// A paging walk across window edges with a secret-looking path: each row
/// once, in the redacted order, the token on no page, for the default order
/// and a key redaction does not touch.
#[test]
fn a_secret_looking_path_keeps_its_redacted_rank_across_window_edges() {
    let token = format!("ghp_{}", "a1B2c3D4".repeat(5));
    let root = Fixture::new();
    let names = [
        "Z.rs".to_owned(),
        "a.rs".to_owned(),
        format!("{token}.rs"),
        "h.rs".to_owned(),
    ];
    for (size, name) in names.iter().enumerate() {
        root.write(name, "x".repeat(size + 1));
    }
    for sort in [
        json!({"by":"path","order":"asc"}),
        json!({"by":"sizeBytes","order":"desc"}),
    ] {
        let query = json!({"operation":"files","path":root.0,"view":"paths","sort":sort});
        let pages = with_window(1, || walk_pages_budget(&root.0, query.clone(), Some(120)));
        assert!(pages.len() >= names.len(), "{pages:?}");
        for page in &pages {
            assert!(!page.to_string().contains(&token), "{page}");
        }
        let read = pages.iter().flat_map(paths).collect::<Vec<_>>();
        let whole = paths(&run(&root.0, query).expect("one window"));
        assert_eq!(read, whole, "{sort}");
        assert_eq!(read.len(), names.len(), "{read:?}");
    }
    let by_path = paths(
        &run(&root.0, json!({"operation":"files","path":root.0,"view":"paths","sort":{"by":"path","order":"asc"}}))
            .expect("listing"),
    );
    let at = |name: &str| by_path.iter().position(|path| path.ends_with(name));
    let redacted = by_path
        .iter()
        .position(|path| {
            !["Z.rs", "a.rs", "h.rs"]
                .iter()
                .any(|name| path.ends_with(name))
        })
        .expect("the redacted row");
    assert!(at("Z.rs") < Some(redacted), "{by_path:?}");
    assert!(
        Some(redacted) < at("a.rs"),
        "ranked by what it reads as: {by_path:?}"
    );
}
