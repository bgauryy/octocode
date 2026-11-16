//! A listing past its window costs one walk of its scope, not one per window,
//! and reads as the walk per window it replaces: every row once, in order,
//! whatever a page holds; an edit between windows restarts it.
use super::super::listing::listing_walks;
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

mod later_windows;

const WINDOW_ROWS: usize = 7;

fn fixture() -> Fixture {
    let root = Fixture::new();
    for dir in 0..6 {
        for file in 0..8 {
            root.write(&format!("d{dir}/f{file}.rs"), "x\n".repeat(file + 1));
        }
    }
    root
}

fn listing(root: &Fixture, operation: &str, extra: &Value) -> Value {
    let mut row = json!({"operation": operation, "path": root.0, "maxDepth": 10});
    row.as_object_mut()
        .expect("row")
        .extend(extra.as_object().expect("extra").clone());
    row
}

/// Listings that walk their whole scope: an outline, and every sort that ranks.
fn ranked() -> Vec<(&'static str, Value)> {
    let sorted = |by: &str, order: &str| ("files", json!({"sort":{"by":by,"order":order}}));
    vec![
        ("tree", json!({})),
        ("files", json!({})),
        sorted("path", "desc"),
        sorted("name", "desc"),
        sorted("sizeBytes", "desc"),
        sorted("modifiedAt", "asc"),
        sorted("totalLines", "desc"),
    ]
}

fn traversal() -> (&'static str, Value) {
    ("files", json!({"sort":{"by":"traversal","order":"asc"}}))
}

fn next_query(out: &Value) -> Option<Value> {
    out["next"]
        .get("nextPage")
        .map(|next| next["input"]["queries"][0].clone())
}

/// Each window's first response, with `cancel` watching the walk, and the
/// query that continues it; `between` runs before every later window.
fn follow(
    root: &Fixture,
    start: Value,
    cancel: &dyn CancellationCheck,
    between: &dyn Fn(&Value),
) -> Vec<Value> {
    let (paths, security) = simple_policy(&root.0);
    let mut outs: Vec<Value> = Vec::new();
    let mut query = start;
    loop {
        assert!(outs.len() < 60, "the listing never ends: {query}");
        let out = execute_row(query.clone(), &paths, &security, cancel).expect("listing");
        let next = next_query(&out);
        outs.push(out);
        match next {
            Some(next) => {
                between(&next);
                query = next;
            }
            None => return outs,
        }
    }
}

/// The first window of `row`, and the query of the window after it.
fn first_window(root: &Fixture, row: Value) -> (Value, Value) {
    let out = run(&root.0, row).expect("first window");
    let next = next_query(&out).unwrap_or_else(|| panic!("a second window: {out}"));
    (out, next)
}

#[test]
fn a_listing_past_its_window_walks_its_scope_once_at_any_page_size() {
    for (operation, extra) in ranked() {
        for budget in [None, Some(40)] {
            let root = fixture();
            let row = listing(&root, operation, &extra);
            let before = listing_walks();
            let pages = with_window(WINDOW_ROWS, || {
                walk_pages_budget(&root.0, row.clone(), budget)
            });
            assert_eq!(
                listing_walks() - before,
                1,
                "{operation} {extra} budget {budget:?}"
            );
            let windows = pages
                .iter()
                .filter(|out| out.get("summary").is_some())
                .count();
            assert!(windows > 1, "{operation} {extra}: {windows} windows");
            if budget.is_some() {
                assert!(pages.len() > windows, "{operation} {extra}: one page each");
            }
        }
    }
}

/// A repeat of a listing walked whole, on an unchanged scope, reads the
/// listing its first call kept: the same rows and snapshot, no walk. An edit
/// fails the kept proof, so the repeat walks again and shows it.
#[test]
fn a_repeated_whole_listing_reads_its_kept_listing_until_the_scope_changes() {
    for (operation, extra) in ranked() {
        let root = fixture();
        let row = listing(&root, operation, &extra);
        let (first, _) = with_window(WINDOW_ROWS, || first_window(&root, row.clone()));
        let before = listing_walks();
        let (again, _) = with_window(WINDOW_ROWS, || first_window(&root, row.clone()));
        assert_eq!(listing_walks() - before, 0, "{operation} {extra}: {again}");
        assert_eq!(paths(&again), paths(&first), "{operation} {extra}");
        assert_eq!(
            crate::tools::test_support::pinned_snapshot(&again),
            crate::tools::test_support::pinned_snapshot(&first),
            "{operation} {extra}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
        root.write("d0/f0.rs", "x\n".repeat(90));
        root.write("d9/new.rs", "x\n");
        let before = listing_walks();
        let (changed, _) = with_window(WINDOW_ROWS, || first_window(&root, row.clone()));
        assert_eq!(listing_walks() - before, 1, "{operation} {extra}");
        assert_ne!(
            crate::tools::test_support::pinned_snapshot(&changed),
            crate::tools::test_support::pinned_snapshot(&first),
            "{operation} {extra}"
        );
    }
}

#[test]
fn a_walk_order_listing_past_its_window_replays_no_earlier_entry() {
    struct Counting(AtomicUsize);
    impl CancellationCheck for Counting {
        fn check(&self) -> Result<(), String> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }
    let root = fixture();
    let (operation, extra) = traversal();
    let row = listing(&root, operation, &extra);
    let whole = Counting(AtomicUsize::new(0));
    follow(&root, row.clone(), &whole, &|_| {});
    let chained = Counting(AtomicUsize::new(0));
    let outs = with_window(WINDOW_ROWS, || follow(&root, row, &chained, &|_| {}));
    assert!(outs.len() > 2, "{} windows", outs.len());
    let (whole, chained) = (whole.0.into_inner(), chained.0.into_inner());
    assert!(
        chained <= whole + outs.len() * 8,
        "{} windows inspected {chained} entries of a walk of {whole}: they replayed",
        outs.len()
    );
}

#[test]
fn every_file_is_listed_once_in_listing_order_across_windows_at_any_page_size() {
    let mut all = ranked();
    all.push(traversal());
    for (operation, extra) in all {
        let root = fixture();
        let mut row = listing(&root, operation, &extra);
        row["entryType"] = json!("file");
        let whole = paths(&run(&root.0, row.clone()).expect("whole"));
        assert_eq!(whole.len(), 48, "{operation} {extra}");
        for budget in [None, Some(40)] {
            let pages = with_window(WINDOW_ROWS, || {
                walk_pages_budget(&root.0, row.clone(), budget)
            });
            let chained = pages.iter().flat_map(paths).collect::<Vec<_>>();
            assert_eq!(chained, whole, "{operation} {extra} budget {budget:?}");
        }
    }
}

/// Windows the edge walk is cut to: one row a page, a window ending at a
/// directory's stub (a directory of the fixture holds eight files), and a
/// window at the whole listing (six directories, forty-eight files).
const EDGE_WINDOWS: [usize; 7] = [1, 2, 8, 9, 53, 54, 55];

/// The windows listing `index` of the sorts is walked at in the gate: the
/// smallest (every row cut off alone) and two others, the second half of the
/// list rotating so each window meets several sorts.
fn gate_windows(index: usize) -> Vec<usize> {
    let others = &EDGE_WINDOWS[1..];
    vec![
        EDGE_WINDOWS[0],
        others[index % others.len()],
        others[(index + others.len() / 2) % others.len()],
    ]
}

/// Directories list too, and a window that ends right at a directory's stub
/// leaves its entries to the next: the same rows at every window edge.
fn assert_no_row_lost_or_repeated(windows_of: impl Fn(usize) -> Vec<usize>) {
    let mut all = ranked();
    all.push(traversal());
    for (index, (operation, extra)) in all.into_iter().enumerate() {
        let root = fixture();
        let row = listing(&root, operation, &extra);
        let files = |pages: &[Value]| {
            let mut files = pages
                .iter()
                .flat_map(paths)
                .filter(|path| path.ends_with(".rs"))
                .collect::<Vec<_>>();
            files.sort();
            files
        };
        let whole = files(std::slice::from_ref(
            &run(&root.0, row.clone()).expect("whole"),
        ));
        for window in windows_of(index) {
            for budget in [None, Some(40)] {
                // `walk_pages` fails on a row listed twice.
                let pages = with_window(window, || walk_pages_budget(&root.0, row.clone(), budget));
                assert_eq!(files(&pages), whole, "{operation} {extra} window {window}");
            }
        }
    }
}

#[test]
fn a_window_edge_anywhere_loses_and_repeats_no_row() {
    assert_no_row_lost_or_repeated(gate_windows);
}

#[test]
#[ignore = "every edge window for every sort: cargo test -- --ignored every_edge_window"]
fn a_window_edge_anywhere_loses_and_repeats_no_row_at_every_edge_window() {
    assert_no_row_lost_or_repeated(|_| EDGE_WINDOWS.to_vec());
}

fn assert_restarts(root: &Fixture, next: Value, label: &str) {
    let stale = run(&root.0, next.clone()).expect("next window");
    assert_eq!(stale["errorCode"], "staleSnapshot", "{label}: {stale}");
    let restart = &stale["next"]["restart"]["input"]["queries"][0];
    assert!(restart.get("scanOffset").is_none(), "{label}: {restart}");
    assert!(restart.get("snapshot").is_none(), "{label}: {restart}");
}

/// An entry created or removed anywhere the windows already listed restarts
/// the listing: the rows after it moved, so its next window would repeat one
/// or skip one.
#[test]
fn an_entry_created_or_removed_between_windows_restarts_the_listing() {
    let mut all = ranked();
    all.push(traversal());
    for (operation, extra) in all {
        let root = fixture();
        let row = listing(&root, operation, &extra);
        with_window(WINDOW_ROWS, || {
            let (_, next) = first_window(&root, row.clone());
            std::thread::sleep(std::time::Duration::from_millis(20));
            root.write("a0.rs", "x\n");
            assert_restarts(&root, next, &format!("created {operation} {extra}"));
            let (_, next) = first_window(&root, row.clone());
            std::thread::sleep(std::time::Duration::from_millis(20));
            std::fs::remove_file(root.0.join("a0.rs")).expect("remove");
            assert_restarts(&root, next, &format!("removed {operation} {extra}"));
        });
    }
}

/// A details row shows its file's size and time: an in-place edit of one the
/// listing already showed restarts it too.
#[test]
fn a_file_edited_between_details_windows_restarts_the_listing() {
    let mut all = ranked();
    all.push(traversal());
    for (operation, extra) in all {
        let root = fixture();
        let mut row = listing(&root, operation, &extra);
        row["view"] = json!("details");
        with_window(WINDOW_ROWS, || {
            let (first, next) = first_window(&root, row.clone());
            let shown = paths(&first)
                .into_iter()
                .find(|path| path.ends_with(".rs"))
                .expect("a file row");
            let relative = shown
                .rsplit_once("/d")
                .map_or(shown.clone(), |(_, tail)| format!("d{tail}"));
            std::thread::sleep(std::time::Duration::from_millis(20));
            root.write(&relative, "edited, and longer than it was\n");
            assert_restarts(&root, next, &format!("{operation} {extra} {relative}"));
        });
    }
}

/// A walk-order listing has not seen what it has not reached: a newcomer past
/// where the listing stands is listed once when the walk gets there, in a
/// directory the walk is inside as much as in one it has not entered. (A
/// details row of that directory shows its time, which the newcomer moved.)
#[test]
fn a_newcomer_past_a_walk_order_window_is_listed_once() {
    for newcomer in ["d5/zz.rs", "d0/zz.rs"] {
        let root = fixture();
        let (operation, extra) = traversal();
        let mut row = listing(&root, operation, &extra);
        row["view"] = json!("paths");
        with_window(WINDOW_ROWS, || {
            let (first, next) = first_window(&root, row.clone());
            std::thread::sleep(std::time::Duration::from_millis(20));
            root.write(newcomer, "x\n");
            let rest = follow(&root, next, &NeverCancel, &|_| {});
            let mut listed = paths(&first);
            listed.extend(rest.iter().flat_map(paths));
            let new = listed
                .iter()
                .filter(|path| path.ends_with(newcomer))
                .count();
            assert_eq!(new, 1, "{newcomer}: {listed:?}");
            let mut distinct = listed.clone();
            distinct.sort();
            distinct.dedup();
            assert_eq!(
                distinct.len(),
                listed.len(),
                "{newcomer}: a repeat: {listed:?}"
            );
        });
    }
}

/// A window a walk serves reads exactly as one the kept listing serves: rows,
/// counts of what it left out, totals and continuations, what is left out
/// before and after the window edge included.
#[test]
fn a_window_from_what_was_kept_reads_as_one_walked_anew() {
    let mut all = ranked();
    all.push(traversal());
    for (operation, extra) in all {
        let root = fixture();
        root.write(".git/HEAD", "ref: refs/heads/main\n");
        root.write(".gitignore", "*.log\n");
        for dir in 0..6 {
            root.write(&format!("d{dir}/skip.log"), "x\n");
            root.write(&format!("d{dir}/.dot"), "x\n");
            root.write(&format!("d{dir}/key.pem"), "x\n");
        }
        for view in ["paths", "details"] {
            let mut row = listing(&root, operation, &extra);
            row["view"] = json!(view);
            with_window(WINDOW_ROWS, || {
                let kept = follow(&root, row.clone(), &NeverCancel, &|_| {});
                let walked = follow(&root, row.clone(), &NeverCancel, &|next| {
                    super::super::windows::forget(next["snapshot"].as_str().expect("pinned"));
                });
                assert!(kept.len() > 2, "{operation} {extra}");
                assert_eq!(kept, walked, "{operation} {extra} {view}");
            });
        }
    }
}

/// Evicted or not, a window pinned to rows that moved restarts: only the walk
/// is left to tell.
#[test]
fn a_walk_that_lost_the_kept_listing_still_restarts_on_an_edit() {
    let mut all = ranked();
    all.push(traversal());
    for (operation, extra) in all {
        let root = fixture();
        let row = listing(&root, operation, &extra);
        with_window(WINDOW_ROWS, || {
            let (_, next) = first_window(&root, row.clone());
            super::super::windows::forget(next["snapshot"].as_str().expect("pinned"));
            root.write("a0.rs", "x\n");
            assert_restarts(&root, next, &format!("{operation} {extra}"));
        });
    }
}

/// Redaction moves a name across others in path order, and the windows keep
/// the order delivered.
#[test]
fn kept_windows_rank_and_show_a_redacted_name_as_delivered() {
    const TOKEN: &str = "ghp_a1B2c3D4a1B2c3D4a1B2c3D4a1B2c3D4a1B2";
    let root = fixture();
    root.write(&format!("d1/{TOKEN}.rs"), "x\n");
    root.write(&format!("{TOKEN}.rs"), "x\n");
    let mut row = listing(&root, "files", &json!({}));
    row["entryType"] = json!("file");
    let whole = paths(&run(&root.0, row.clone()).expect("whole"));
    let pages = with_window(WINDOW_ROWS, || walk_pages(&root.0, row));
    let chained = pages.iter().flat_map(paths).collect::<Vec<_>>();
    assert_eq!(chained, whole);
    assert!(!format!("{pages:?}").contains(TOKEN), "{chained:?}");
}
