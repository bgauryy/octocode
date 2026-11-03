//! Continuation pages of a full-row answer slice the rows its first page
//! retained: no detector, impact walk or snapshot diff runs again.
use super::*;
use crate::tools::ast_graph::test_support::next_offset;
use std::sync::Arc;

fn detector_runs() -> usize {
    detect::RUNS.with(std::cell::Cell::get)
}

fn retention() -> Arc<SavedRows> {
    Arc::new(SavedRows::new(
        crate::cache::CacheConfig::derived(),
        None,
        "test".into(),
    ))
}

/// The options of the page after `out`, as the runtime's continuation
/// carries them: pinned to its snapshot, one call further.
fn advance(options: &mut QueryOptions, out: &GraphOutput, offset: usize) {
    options.offset = Some(offset);
    options.page = Some(options.page.unwrap_or(1) + 1);
    options.graph = out.value["graph"]["id"].as_str().map(str::to_owned);
}

/// Every page of `op` at `limit` rows a page, first to last.
fn walk(
    dir: &Path,
    rows: &Arc<SavedRows>,
    op: &str,
    limit: usize,
    tweak: impl Fn(&mut QueryOptions),
) -> Vec<GraphOutput> {
    let mut options = QueryOptions {
        op: op.into(),
        workspace: Some(dir.to_path_buf()),
        limit: Some(limit),
        rows: Some(rows.clone()),
        ..Default::default()
    };
    tweak(&mut options);
    let mut pages = Vec::new();
    loop {
        let out = query(&options, &workspace_policy(dir));
        assert_ne!(out.class, ExitClass::InvalidInput, "{}", out.value);
        let more = next_offset(&out);
        if let Some(offset) = more {
            advance(&mut options, &out, offset);
        }
        pages.push(out);
        if more.is_none() {
            return pages;
        }
    }
}

fn all_ids(pages: &[GraphOutput]) -> Vec<String> {
    pages.iter().flat_map(ids).collect()
}

#[test]
fn issues_pages_after_the_first_run_no_detectors() {
    let dir = issues_fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let before = detector_runs();
    let pages = walk(dir.0.as_path(), &retention(), "issues", 1, |_| {});
    assert!(pages.len() >= 3, "{}", pages[0].value);
    assert_eq!(detector_runs() - before, 1, "one detector run per walk");

    // Exactly once, in order: the walk is the whole ordered set.
    let whole = ask(dir.0.as_path(), "issues", None, |o| o.limit = Some(500));
    assert_eq!(all_ids(&pages), ids(&whole));
    let total = whole.value["pagination"]["totalItems"].clone();
    for page in &pages {
        assert_eq!(page.value["pagination"]["totalItems"], total);
    }
    // The summary belongs to the run that made it.
    assert!(pages[0].value["summary"].is_object());
    for page in &pages[1..] {
        assert!(page.value.get("summary").is_none(), "{}", page.value);
    }
}

#[test]
fn a_fresh_request_never_reuses_a_retained_set() {
    let dir = issues_fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let rows = retention();
    walk(dir.0.as_path(), &rows, "issues", 1, |_| {});
    let before = detector_runs();
    let again = walk(dir.0.as_path(), &rows, "issues", 1, |_| {});
    assert_eq!(
        detector_runs() - before,
        1,
        "the walk's first page computes"
    );
    assert!(again[0].value["summary"].is_object());
}

/// A retained set answers only the question it was kept for, of the
/// snapshot it was kept for.
#[test]
fn a_retained_set_is_bound_to_its_query_and_snapshot() {
    let dir = issues_fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    let id = first.value["id"].as_str().expect("id").to_owned();
    let rows = retention();
    let pages = walk(dir.0.as_path(), &rows, "issues", 1, |_| {});
    let ordered = all_ids(&pages);
    let second_page = |tweak: &dyn Fn(&mut QueryOptions)| {
        let mut options = QueryOptions {
            op: "issues".into(),
            workspace: Some(dir.0.clone()),
            limit: Some(1),
            offset: Some(1),
            page: Some(2),
            graph: Some(id.clone()),
            rows: Some(rows.clone()),
            ..Default::default()
        };
        tweak(&mut options);
        query(&options, &workspace_policy(&dir.0))
    };
    let before = detector_runs();
    let same = second_page(&|_| {});
    assert_eq!(detector_runs(), before, "the retained set answers");
    assert_eq!(ids(&same), ordered[1..2]);

    // Another question of the same snapshot computes its own set.
    let other = second_page(&|o| o.detectors = vec!["cycle".into()]);
    assert_eq!(detector_runs(), before + 1);
    assert_ne!(other.value["pagination"], same.value["pagination"]);

    // An unknown snapshot is refused; no retained row stands in for it.
    let unknown = second_page(&|o| o.graph = Some("no-such-snapshot".into()));
    assert_eq!(unknown.value["errorCode"], "notFound", "{}", unknown.value);
    assert!(unknown.value.get("rows").is_none(), "{}", unknown.value);
    assert_eq!(detector_runs(), before + 1);

    // A rebuilt snapshot is another snapshot: the page computes over it.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let rebuilt = ingest_fixture(dir.0.as_path(), None);
    let rebuilt = rebuilt.value["id"].as_str().expect("id").to_owned();
    assert_ne!(rebuilt, id);
    let moved = second_page(&|o| o.graph = Some(rebuilt.clone()));
    assert_eq!(moved.value["graph"]["id"], rebuilt.as_str());
    assert_eq!(detector_runs(), before + 2);
}

#[test]
fn a_page_whose_set_is_not_retained_computes_it_and_retains_it_for_the_next() {
    let dir = issues_fixture();
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let whole = ask(dir.0.as_path(), "issues", None, |o| o.limit = Some(500));
    // Nothing retained, as after an expiry or in a runtime that ended.
    let rows = retention();
    let mut options = QueryOptions {
        op: "issues".into(),
        workspace: Some(dir.0.clone()),
        limit: Some(1),
        offset: Some(1),
        page: Some(2),
        rows: Some(rows.clone()),
        ..Default::default()
    };
    let before = detector_runs();
    let page = query(&options, &workspace_policy(&dir.0));
    assert_eq!(detector_runs() - before, 1);
    assert_eq!(ids(&page), ids(&whole)[1..2]);
    let offset = next_offset(&page).expect("more pages");
    advance(&mut options, &page, offset);
    let next = query(&options, &workspace_policy(&dir.0));
    assert_eq!(detector_runs() - before, 1, "the second page replays");
    assert_eq!(ids(&next), ids(&whole)[2..3]);
}

#[test]
fn impact_and_drift_pages_reconstruct_their_rows_exactly_once() {
    let dir = issues_fixture();
    let first = ingest_fixture(dir.0.as_path(), None);
    let base = first.value["id"].as_str().expect("id").to_owned();
    dir.write("app/src/extra.ts", "export const extra = () => 1;\n");
    dir.write(
        "app/src/orphan2.ts",
        "import './extra';\nexport const x = 1;\n",
    );
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    let rows = retention();
    type Tweak = fn(&mut QueryOptions);
    let questions: [(&str, Tweak); 2] = [
        ("impact", |o| o.target = Some("src/lib.ts".into())),
        ("drift", |o| o.baseline = Some("base".into())),
    ];
    for (op, tweak) in questions {
        let tweak = |o: &mut QueryOptions| {
            tweak(o);
            if op == "drift" {
                o.baseline = Some(base.clone());
            }
        };
        let pages = walk(dir.0.as_path(), &rows, op, 1, tweak);
        let whole = ask(dir.0.as_path(), op, None, |o| {
            tweak(o);
            o.limit = Some(500);
        });
        let total = whole.value["pagination"]["totalItems"]
            .as_u64()
            .unwrap_or(0);
        assert!(total >= 2, "{op}: {}", whole.value);
        assert_eq!(pages.len() as u64, total, "{op}: one row a page");
        let walked = pages
            .iter()
            .flat_map(|page| page.value["rows"].as_array().cloned().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(
            Value::Array(walked),
            whole.value["rows"],
            "{op}: pages are the whole set, once, in order"
        );
    }
}
