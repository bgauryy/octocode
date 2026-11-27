//! A walk-order content window ends at a start of its pages (TE3): it reads
//! the files its pages need, not its scope; its totals are floors
//! (`countScope: loaded`, no `matchCount`), never a coverage gap; every page
//! but the walk's last is whole, wherever a window ends; and the walk goes
//! on through `next.nextPage`, each match once.
use super::super::test_rows::{ast_chain, ast_rows, run, run_window};
use octocode_runtime_base::test_support::Fixture;
use serde_json::{Value, json};

/// `files` Rust files, each with `per_file` calls of `hit`.
fn hits(files: usize, per_file: usize) -> Fixture {
    let root = Fixture::new();
    for file in 0..files {
        let calls = (0..per_file)
            .map(|n| format!("    hit({n});\n"))
            .collect::<String>();
        root.write(&format!("f{file:02}.rs"), format!("fn f() {{\n{calls}}}\n"));
    }
    root
}

fn query(root: &Fixture, limit: Option<u64>) -> Value {
    let mut query =
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"}});
    if let Some(limit) = limit {
        query["limit"] = json!(limit);
    }
    query
}

/// Page 1 of a broad rule reads through the file that fills it: the window
/// is that one file, ending where page 2 starts inside it, its count a floor
/// of what it lists, and nothing about it is partial.
#[test]
fn a_content_page_reads_only_the_files_its_page_needs() {
    let root = hits(40, 3);
    let out = run(&root.0, query(&root, Some(2))).expect("page 1");
    assert_eq!(out["files"].as_array().map(Vec::len), Some(1), "{out}");
    assert_eq!(
        out["pagination"]["total"], 2,
        "only f00 was read, up to page 2: {out}"
    );
    assert_eq!(out["pagination"]["countScope"], "loaded", "{out}");
    assert_eq!(out["pagination"]["hasMore"], true, "{out}");
    assert!(out.get("matchCount").is_none(), "no final count: {out}");
    for absent in ["partialReasons", "isPartial", "coverage"] {
        assert!(out.get(absent).is_none(), "{absent}: {out}");
    }
    assert!(
        !out.to_string().contains("structural.scan.truncated"),
        "a filled page is no time cut: {out}"
    );
    assert!(out["next"]["nextPage"].is_object(), "{out}");
    assert!(out["next"].get("expandScan").is_none(), "{out}");
}

/// The window's last page leads on through `nextPage` to the next window,
/// and walking every page reads each match of the scope exactly once, as
/// the whole scan would list them.
#[test]
fn walking_paged_windows_reads_every_match_once() {
    for (files, per_file, limit) in [(40, 3, Some(2)), (25, 1, Some(4)), (12, 7, None)] {
        let root = hits(files, per_file);
        let pages = ast_chain(&root.0, query(&root, limit));
        let rows = ast_rows(&pages);
        let distinct = rows.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            rows.len(),
            files * per_file,
            "{files}x{per_file}: {pages:?}"
        );
        assert_eq!(distinct.len(), rows.len(), "each match once");
        // In walk order: file by file, each file's calls in source order.
        let order = rows
            .iter()
            .map(|row| row.split(' ').next().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(order, sorted, "walk order");
        // Every page starts where the rows read before it end, across
        // windows too, and its total is the scope's or a floor of it.
        let mut read = 0_u64;
        for page in &pages {
            let pagination = &page["pagination"];
            assert_eq!(pagination["offset"], read, "{page}");
            let shown = page["files"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|file| file["matches"].as_array().map_or(0, Vec::len))
                .sum::<usize>() as u64;
            read += shown;
            let total = pagination["total"].as_u64().expect("total");
            assert!(read <= total, "{page}");
            if pagination.get("countScope").is_none() {
                assert_eq!(total, (files * per_file) as u64, "{page}");
            }
        }
        let last = pages.last().expect("a last page");
        assert_eq!(last["pagination"]["hasMore"], false, "{last}");
        for page in &pages {
            assert!(page.get("partialReasons").is_none(), "{page}");
        }
    }
}

/// A scope its first page reads whole keeps its exact count: no window
/// was cut, so `matchCount` stands and pagination counts it all.
#[test]
fn a_scope_one_page_holds_keeps_its_exact_count() {
    let root = hits(3, 2);
    let out = run(&root.0, query(&root, None)).expect("page");
    assert_eq!(out["matchCount"], 6, "{out}");
    assert!(out["pagination"].get("countScope").is_none(), "{out}");
}

/// The window a page ends depends only on the files and the page, never on
/// how the scan's chunks were cut or how busy the process is: a page read
/// again under another response window of the same size names the same
/// files, and every continuation page agrees with the page before it.
#[test]
fn a_window_ends_at_the_same_file_on_every_run() {
    let root = hits(30, 2);
    let first = run_window(&root.0, query(&root, None), 400).expect("page 1");
    let again = run_window(&root.0, query(&root, None), 400).expect("page 1 again");
    assert_eq!(first["pagination"], again["pagination"]);
    assert_eq!(first["files"], again["files"]);
    assert_eq!(first["next"], again["next"]);
}

/// A later window reads at least as many candidates as the windows before
/// it, so a walk through the scope takes a number of windows logarithmic in
/// it, not one per page.
#[test]
fn later_windows_grow_so_a_walk_takes_few_windows() {
    let root = hits(64, 1);
    let pages = ast_chain(&root.0, query(&root, Some(1)));
    assert_eq!(ast_rows(&pages).len(), 64);
    let windows = pages
        .iter()
        .filter(|page| page["pagination"]["offset"] == 0)
        .count();
    assert!(
        windows <= 8,
        "{windows} windows for 64 files: each window at least doubles"
    );
}

/// Every page but the walk's last is whole, wherever a scan window ends: a
/// page `limit` caps holds `limit` matches, so the walk takes the fewest
/// pages its matches need, however the windows split the files.
#[test]
fn every_page_but_the_last_is_whole_across_windows() {
    for (files, per_file, limit) in [(40, 3, 2), (12, 7, 3), (5, 40, 9), (64, 1, 5)] {
        let root = hits(files, per_file);
        let pages = ast_chain(&root.0, query(&root, Some(limit)));
        let shown = pages
            .iter()
            .map(|page| {
                page["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|file| file["matches"].as_array().map_or(0, Vec::len))
                    .sum::<usize>()
            })
            .collect::<Vec<_>>();
        let total = files * per_file;
        assert_eq!(shown.iter().sum::<usize>(), total, "{shown:?}");
        let (last, whole) = shown.split_last().expect("a page");
        assert!(
            whole.iter().all(|rows| *rows == limit as usize),
            "{files}x{per_file} limit {limit}: {shown:?}"
        );
        assert!((1..=limit as usize).contains(last), "{shown:?}");
        assert_eq!(pages.len(), total.div_ceil(limit as usize), "{shown:?}");
    }
}

/// A window that ends inside a file pins it: the next window lists that
/// file's rest only while it is unchanged, and restarts the walk when it
/// changed, never listing its matches from two versions.
#[test]
fn a_file_a_window_ended_inside_stays_pinned_for_the_next_window() {
    let root = hits(3, 5);
    let first = run(&root.0, query(&root, Some(2))).expect("page 1");
    assert_eq!(first["pagination"]["countScope"], "loaded", "{first}");
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(next.is_object(), "{first}");
    let second = run(&root.0, next.clone()).expect("page 2");
    assert!(second.get("errorCode").is_none(), "{second}");
    assert_eq!(second["pagination"]["offset"], 2, "{second}");
    let first_rows = ast_rows(std::slice::from_ref(&first));
    let second_rows = ast_rows(std::slice::from_ref(&second));
    assert!(
        second_rows.iter().all(|row| !first_rows.contains(row)),
        "no row twice: {first} {second}"
    );
    root.write("f00.rs", "fn f() {\n    hit(10);\n    hit(11);\n}\n");
    let moved = run(&root.0, next).expect("page 2 again");
    assert_eq!(moved["errorCode"], "staleSnapshot", "{moved}");
}

/// Every page but the walk's last fills the response window, the first one
/// too when the scan's census rides it: a window's pages are cut where the
/// census is charged, so no short page sits between full ones.
#[test]
fn budget_filled_pages_stay_full_across_windows_with_a_census() {
    let root = hits(60, 4);
    root.write("notes.toml", "key = 1\n");
    root.write("app.py", "def f():\n    pass\n");
    root.write(".hidden/skipped.rs", "fn g() { hit(0); }\n");
    let mut shown = Vec::new();
    let mut seen = Vec::new();
    let mut next = query(&root, None);
    loop {
        let page = run_window(&root.0, next, 3000).expect("page");
        let rows = ast_rows(std::slice::from_ref(&page));
        shown.push(rows.len());
        seen.extend(rows);
        let Some(call) = page["next"]["nextPage"]["input"]["queries"].get(0) else {
            break;
        };
        next = call.clone();
    }
    assert!(shown.len() >= 4, "{shown:?}");
    let distinct = seen.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(seen.len(), 60 * 4, "{shown:?}");
    assert_eq!(distinct.len(), seen.len(), "each match once");
    let (_, rest) = shown.split_last().expect("pages");
    let (_, middle) = rest.split_first().expect("pages");
    let full = middle[0];
    assert!(
        middle.iter().all(|rows| rows.abs_diff(full) <= 1),
        "a page between the first and last is full: {shown:?}"
    );
    assert!(shown[0] <= full + 1, "{shown:?}");
}
