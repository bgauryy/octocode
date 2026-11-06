//! A walk read across an edit, cut by time: every surviving row exactly once,
//! or `staleSnapshot` with its restart. Nothing else passes.

use super::*;

/// How a chain of windows ended.
enum Ended {
    /// Every window's rows, the first one's included.
    Rows(Vec<String>),
    Stale,
}

/// Follows `next.nextPage` from `query` under `budget`.
fn follow(root: &std::path::Path, first: Vec<String>, query: Value, budget: Duration) -> Ended {
    let mut rows = first;
    let mut query = query;
    for _ in 0..200 {
        let cancel: &dyn CancellationCheck = if budget < UNSPENT {
            &Slow
        } else {
            &NeverCancel
        };
        let out = within_checked(root, query.clone(), budget, cancel);
        if out["errorCode"] == "staleSnapshot" {
            let restart = &out["related"][0]["input"]["queries"][0];
            assert!(restart.is_object(), "{out}");
            for cursor in ["walkOffset", "scanOffset", "snapshot", "page"] {
                assert!(restart.get(cursor).is_none(), "{cursor}: {out}");
            }
            return Ended::Stale;
        }
        rows.extend(listed_entries(&out));
        match out["next"].get("nextPage") {
            Some(next) => query = next["input"]["queries"][0].clone(),
            None => return Ended::Rows(rows),
        }
    }
    panic!("the walk never finishes: {query}");
}

/// The rows of a walk of `root`, sorted.
fn whole(root: &Fixture, query: &Value) -> Vec<String> {
    let mut rows = listed_entries(&within(&root.0, query.clone(), UNSPENT));
    rows.sort();
    rows
}

fn exactly_once(ended: Ended, mut expected: Vec<String>, case: &str) {
    if let Ended::Rows(mut rows) = ended {
        rows.sort();
        expected.sort();
        assert_eq!(rows, expected, "{case}: every surviving row exactly once");
    }
}

/// The shapes a time-cut listing runs under.
fn shapes() -> Vec<(&'static str, Value)> {
    let mut shapes = Vec::new();
    for view in ["paths", "details"] {
        shapes.push(("tree", json!({"view":view})));
        shapes.push(("files", json!({"view":view})));
        for by in ["name", "sizeBytes", "modifiedAt", "lineCount"] {
            shapes.push((
                "files",
                json!({"view":view,"sort":{"by":by,"order":"desc"}}),
            ));
        }
    }
    shapes
}

fn query(operation: &str, root: &Fixture, shape: &Value) -> Value {
    let mut row = rows(operation, &root.0);
    row.as_object_mut()
        .expect("row")
        .extend(shape.as_object().expect("shape").clone());
    row
}

fn flat() -> Fixture {
    let root = Fixture::new();
    for index in 10..30 {
        root.write(&format!("f{index}.rs"), "fn f() {}\n");
    }
    root
}

/// A first window cut by time after a few entries, and the row of its resume.
fn cut(root: &Fixture, query: Value, budget: Duration) -> (Vec<String>, Value) {
    let out = within_checked(&root.0, query, budget, &Slow);
    let next = out["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(next["walkOffset"].as_u64().is_some(), "{out}");
    (listed_entries(&out), next)
}

/// The file behind the row `entry`.
fn file_of(root: &Fixture, entry: &str) -> std::path::PathBuf {
    let name = entry.strip_prefix("./").unwrap_or(entry);
    root.0.join(name)
}

#[test]
fn removing_a_listed_entry_loses_no_row_after_the_cut() {
    for (operation, shape) in shapes() {
        let root = flat();
        let (first, next) = cut(&root, query(operation, &root, &shape), STEP * 6);
        let mut expected = whole(&root, &query(operation, &root, &shape));
        // A paused sorted window lists no row: the first entry in walk order
        // is one its walk passed.
        let victim = first.first().unwrap_or(&expected[0]).clone();
        expected.retain(|row| *row != victim);
        std::thread::sleep(Duration::from_millis(20));
        std::fs::remove_file(file_of(&root, &victim)).expect("remove");
        let mut left = first;
        left.retain(|row| *row != victim);
        exactly_once(
            follow(&root.0, left, next, UNSPENT),
            expected,
            &format!("{operation} {shape}"),
        );
    }
}

/// The entry sorts before the cut, in the directory the walk is inside: a count
/// would land one entry early and list one row twice.
#[test]
fn inserting_an_entry_before_the_cut_repeats_no_row() {
    for (operation, shape) in shapes() {
        let root = flat();
        let (first, next) = cut(&root, query(operation, &root, &shape), STEP * 6);
        let expected = whole(&root, &query(operation, &root, &shape));
        std::thread::sleep(Duration::from_millis(20));
        root.write("f00.rs", "fn f() {}\n");
        let ended = follow(&root.0, first, next, UNSPENT);
        if let Ended::Rows(rows) = &ended {
            let mut once = rows.clone();
            once.sort();
            once.dedup();
            assert_eq!(once.len(), rows.len(), "{operation} {shape}: {rows:?}");
        }
        let mut expected = expected;
        expected.push("./f00.rs".to_owned());
        exactly_once(ended, expected, &format!("{operation} {shape}"));
    }
}

/// The new entry sorts into a directory the walk finished, so no cursor it holds
/// shows it: a walk that ended without it would claim a whole listing.
#[test]
fn inserting_an_entry_in_a_finished_directory_restarts_the_walk() {
    for operation in ["files", "tree"] {
        let root = Fixture::new();
        for dir in 0..6 {
            for file in 0..4 {
                root.write(&format!("d{dir}/f{file}.rs"), "fn f() {}\n");
            }
        }
        let shape = json!({"view":"paths"});
        let (first, next) = cut(&root, query(operation, &root, &shape), STEP * 14);
        assert!(
            first.iter().any(|row| row.contains("d1/")),
            "the walk finished d0: {first:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
        root.write("d0/a.rs", "fn f() {}\n");
        assert!(
            matches!(follow(&root.0, first, next, UNSPENT), Ended::Stale),
            "{operation}"
        );
    }
}

/// A row already listed shows a size and a time that no longer hold.
#[test]
fn editing_a_listed_file_restarts_a_walk_that_showed_its_metadata() {
    for (operation, shape) in shapes()
        .into_iter()
        .filter(|(_, shape)| shape["view"] == "details" || shape.get("sort").is_some())
    {
        if shape["sort"]["by"] == "name" && shape["view"] == "paths" {
            continue;
        }
        let root = flat();
        let (first, next) = cut(&root, query(operation, &root, &shape), STEP * 6);
        let listed = whole(&root, &query(operation, &root, &shape));
        // A paused sorted window showed no row, so nothing it showed went
        // stale: the walk lists the edited tree, every row once.
        let paused = first.is_empty();
        let edited = file_of(&root, first.first().unwrap_or(&listed[0]));
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(edited, "fn longer() {}\n").expect("edit");
        let ended = follow(&root.0, first, next, UNSPENT);
        if paused {
            assert!(matches!(ended, Ended::Rows(_)), "{operation} {shape}");
            exactly_once(ended, listed, &format!("{operation} {shape}"));
        } else {
            assert!(matches!(ended, Ended::Stale), "{operation} {shape}");
        }
    }
}

/// A file the filter left out moved across it: the rows through the cut differ.
#[test]
fn a_filter_verdict_before_the_cut_that_changed_restarts_the_walk() {
    let root = Fixture::new();
    for index in 10..30 {
        root.write(&format!("f{index}.rs"), "x".repeat(200));
    }
    root.write("a.rs", "x");
    let row = query(
        "files",
        &root,
        &json!({"view":"paths","sizeBytes":{"min":100}}),
    );
    let (first, next) = cut(&root, row, STEP * 6);
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(root.0.join("a.rs"), "x".repeat(300)).expect("grow");
    assert!(matches!(
        follow(&root.0, first, next, UNSPENT),
        Ended::Stale
    ));
}

/// An edited ignore rule took a listed row out of the listing.
#[test]
fn a_rule_edit_that_changes_the_rows_before_the_cut_restarts_the_walk() {
    let root = flat();
    std::fs::create_dir(root.0.join(".git")).expect("repository");
    root.write(".gitignore", "nothing\n");
    let shape = json!({"view":"paths"});
    let (first, next) = cut(&root, query("files", &root, &shape), STEP * 8);
    let name = first[0].rsplit('/').next().expect("name").to_owned();
    std::thread::sleep(Duration::from_millis(20));
    root.write(".gitignore", format!("{name}\n"));
    assert!(matches!(
        follow(&root.0, first, next, UNSPENT),
        Ended::Stale
    ));
}

/// Nothing kept (another process, or the cursor evicted) and nothing changed:
/// the resume proves what it passes and lists the rest, each row once.
#[test]
fn a_resume_with_nothing_kept_lists_every_row_once() {
    for (operation, shape) in shapes() {
        let root = flat();
        let (first, next) = cut(&root, query(operation, &root, &shape), STEP * 6);
        super::super::super::resume::forget_all();
        let expected = whole(&root, &query(operation, &root, &shape));
        exactly_once(
            match follow(&root.0, first, next, UNSPENT) {
                Ended::Stale => panic!("{operation} {shape}: nothing changed"),
                rows => rows,
            },
            expected,
            &format!("{operation} {shape}"),
        );
    }
}

/// Nothing kept, and the walk is cut again: each resume pins the rows through
/// its own cut.
#[test]
fn a_chain_of_resumes_with_nothing_kept_lists_every_row_once() {
    let root = flat();
    let shape = json!({"view":"details"});
    let row = query("files", &root, &shape);
    let expected = whole(&root, &row);
    let mut rows = Vec::new();
    let mut next = row;
    let mut windows = 0;
    loop {
        windows += 1;
        assert!(windows < 60, "the walk never finishes");
        let out = within_checked(&root.0, next.clone(), STEP * 6, &Slow);
        assert_ne!(out["errorCode"], "staleSnapshot", "{out}");
        rows.extend(listed_entries(&out));
        super::super::super::resume::forget_all();
        match out["next"].get("nextPage") {
            Some(lead) => next = lead["input"]["queries"][0].clone(),
            None => break,
        }
    }
    assert!(windows > 1);
    exactly_once(Ended::Rows(rows), expected, "chain");
}

/// A resume keeps its rows' pin past a kept cursor too: the edit lands between
/// the second and third windows, before rows the first one listed.
#[test]
fn an_edit_before_the_first_cut_restarts_a_later_resume() {
    let root = flat();
    let row = query("files", &root, &json!({"view":"paths"}));
    let (first, next) = cut(&root, row, STEP * 4);
    let second = within_checked(&root.0, next, STEP * 4, &Slow);
    let third = second["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(third.is_object(), "{second}");
    let victim = first.first().expect("rows").clone();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::remove_file(file_of(&root, &victim)).expect("remove");
    assert!(matches!(
        follow(&root.0, Vec::new(), third, UNSPENT),
        Ended::Stale
    ));
}

/// The row-limit window's pin survives a walk cut before it reached that
/// window's rows, and past them: a removed row is never skipped by count.
#[test]
fn a_window_cut_around_the_rows_it_pinned_still_checks_them() {
    for budget in [STEP * 5 / 3, STEP * 8] {
        for operation in ["files", "tree"] {
            let root = Fixture::new();
            let start = flat_listing(operation, &root, json!({"view":"paths"}));
            let whole_rows = whole(&root, &start);
            WINDOW.set(Some(4));
            let first = within(&root.0, start, UNSPENT);
            WINDOW.set(None);
            let rows = listed_entries(&first);
            assert_eq!(rows.len(), 4, "{first}");
            let widen = first["next"]["nextPage"]["input"]["queries"][0].clone();
            super::super::super::windows::forget(widen["snapshot"].as_str().expect("pinned"));
            let victim = rows[0].clone();
            std::thread::sleep(Duration::from_millis(20));
            std::fs::remove_file(file_of(&root, &victim)).expect("remove");
            let mut left = rows;
            left.retain(|row| *row != victim);
            let mut expected = whole_rows;
            expected.retain(|row| *row != victim);
            let second = within_checked(&root.0, widen, budget, &Slow);
            let ended = if second["errorCode"] == "staleSnapshot" {
                Ended::Stale
            } else {
                left.extend(listed_entries(&second));
                match second["next"].get("nextPage") {
                    Some(next) => {
                        follow(&root.0, left, next["input"]["queries"][0].clone(), UNSPENT)
                    }
                    None => Ended::Rows(left),
                }
            };
            exactly_once(ended, expected, &format!("{operation} {budget:?}"));
        }
    }
}
