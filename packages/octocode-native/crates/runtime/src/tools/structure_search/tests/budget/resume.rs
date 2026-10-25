//! A time-cut listing resumes exactly where it stopped: sorted or walked,
//! past every entry, before rows an earlier window listed, and over a tree
//! that changed.

use super::*;

#[test]
fn a_time_cut_listing_resumes_exactly_where_it_stopped() {
    for operation in ["files", "tree"] {
        let root = Fixture::new();
        for dir in ["a", "b", "c/deep", "d"] {
            for index in 0..4 {
                root.write(&format!("{dir}/f{index}.rs"), "fn f() {}\n");
            }
        }
        root.write("top.rs", "fn f() {}\n");
        let start = rows(operation, &root.0);
        let whole = {
            let mut all = listed_entries(&run(&root.0, start.clone()).expect("whole listing"));
            all.sort();
            all
        };
        assert_eq!(whole.len(), 17, "{operation}: {whole:?}");

        let mut chained = Vec::new();
        let mut windows = 0;
        let mut query = start.clone();
        loop {
            windows += 1;
            assert!(windows <= 40, "{operation}: the walk never finishes");
            let out = within_checked(&root.0, query.clone(), STEP * 3, &Slow);
            assert_row_contract(ToolId::StructureSearch, &out);
            chained.extend(listed_entries(&out));
            let Some(next) = out["next"].get("continueWalk") else {
                assert!(out.get("isPartial").is_none(), "{operation}: {out}");
                break;
            };
            // A resumable cut is no terminal limit and offers no way out of it.
            assert_eq!(out["isPartial"], true, "{operation}: {out}");
            assert_eq!(out["partialReasons"], json!(["walkDeadline"]), "{out}");
            assert!(out.get("terminalLimit").is_none(), "{operation}: {out}");
            assert!(related(&out, "structureSearch").is_null(), "{out}");
            assert!(
                !out["warnings"].to_string().contains("terminalLimit"),
                "{operation}: {out}"
            );
            assert!(out.get("status").is_none(), "{operation}: {out}");
            let resumed = &next["input"]["queries"][0];
            assert!(
                resumed["walkOffset"].as_u64().is_some_and(|at| at > 0),
                "{operation}: {next}"
            );
            assert!(resumed.get("scanOffset").is_none(), "{next}");
            assert!(resumed.get("snapshot").is_none(), "{next}");
            query = resumed.clone();
        }
        assert!(windows > 1, "{operation}: the budget never cut the walk");
        chained.sort();
        assert_eq!(chained, whole, "{operation}: every entry exactly once");
    }
}

#[test]
fn a_resumed_window_past_every_entry_is_not_an_empty_listing() {
    for operation in ["files", "tree"] {
        let root = tree();
        let mut query = rows(operation, &root.0);
        query["walkOffset"] = json!(100);
        let out = within(&root.0, query, Duration::from_secs(3600));
        assert!(out.get("status").is_none(), "{operation}: {out}");
        assert!(out.get("isPartial").is_none(), "{operation}: {out}");
        assert!(listed_entries(&out).is_empty(), "{operation}: {out}");
    }
}

#[test]
fn the_ignored_retry_of_a_resumed_window_lists_from_the_start() {
    let root = Fixture::new();
    std::fs::create_dir(root.0.join(".git")).expect("repository marker");
    root.write(".gitignore", "skip.rs\n");
    for name in ["a.rs", "b.rs", "skip.rs"] {
        root.write(name, "fn f() {}\n");
    }
    for operation in ["files", "tree"] {
        let mut query = rows(operation, &root.0);
        query["walkOffset"] = json!(1);
        let out = within(&root.0, query, Duration::from_secs(3600));
        let retry = &related(&out, "structureSearch")["input"]["queries"][0];
        assert_eq!(retry["includeIgnored"], true, "{operation}: {out}");
        assert!(retry.get("walkOffset").is_none(), "{operation}: {retry}");
    }
}

/// A listing sorted by something else than walk order and cut by time lists
/// its walk's first `maxEntries` rows in walk order, so the resumed walk
/// reaches the rest: no row is lost to a sort a cut walk ranks differently.
#[test]
fn a_sorted_listing_cut_by_time_resumes_exactly() {
    for sort in ["name", "sizeBytes", "modifiedAt", "lineCount"] {
        let root = Fixture::new();
        let mut start = flat_listing("files", &root, json!({"sort": {"by":sort,"order":"desc"}}));
        let whole = {
            let mut all = listed_entries(&run(&root.0, start.clone()).expect("whole"));
            all.sort();
            all
        };
        start["maxEntries"] = json!(3);
        let (windows, _) = follow_windows(&root.0, start, &[STEP * 4]);
        assert!(windows.len() > 1, "{sort}: the budget never cut the walk");
        let mut chained = windows.concat();
        chained.sort();
        assert_eq!(chained, whole, "{sort}: every entry exactly once");
    }
}

/// A widening `continueWalk` skips the rows an earlier window listed. A walk
/// it cut by time that never reached them lists nothing; its resuming
/// `continueWalk` skips the
/// rows still owed, so the rows between its cut and the earlier window's end
/// are not listed twice.
#[test]
fn a_walk_cut_before_the_rows_an_earlier_window_listed_skips_the_rest_after_it() {
    for operation in ["files", "tree"] {
        let root = Fixture::new();
        let mut start = flat_listing(operation, &root, json!({}));
        let whole = listed_entries(&run(&root.0, start.clone()).expect("whole"));
        start["maxEntries"] = json!(4);
        let first = within(&root.0, start, Duration::from_secs(3600));
        let mut chained = listed_entries(&first);
        assert_eq!(chained.len(), 4, "{operation}: {first}");
        let widen = first["next"]["continueWalk"]["input"]["queries"][0].clone();
        assert_eq!(widen["scanOffset"], 4, "{operation}: {first}");
        // The widened walk is cut after two or three entries, before the four
        // rows the first window listed.
        let second = within_checked(&root.0, widen, STEP * 5 / 3, &Slow);
        let cut = second["next"]["continueWalk"]["input"]["queries"][0].clone();
        let at = cut["walkOffset"].as_u64().expect("resume position");
        assert!(at < 4, "{operation}: {second}");
        assert!(listed_entries(&second).is_empty(), "{operation}: {second}");
        assert_eq!(cut["scanOffset"], 4 - at, "{operation}: {cut}");
        let rest = within(&root.0, cut, Duration::from_secs(3600));
        chained.extend(listed_entries(&rest));
        chained.sort();
        let mut whole = whole;
        whole.sort();
        assert_eq!(chained, whole, "{operation}: every entry exactly once");
    }
}

#[test]
fn a_sorted_listing_cut_before_the_rows_an_earlier_window_listed_lists_none_and_is_terminal() {
    let root = Fixture::new();
    let mut query = flat_listing("files", &root, json!({"sort":{"by":"name","order":"asc"}}));
    query["scanOffset"] = json!(4);
    query["maxEntries"] = json!(8);
    let out = within_checked(&root.0, query.clone(), STEP * 5 / 3, &Slow);
    assert_row_contract(ToolId::StructureSearch, &out);
    assert!(listed_entries(&out).is_empty(), "{out}");
    assert_eq!(out["terminalLimit"], true, "{out}");
    assert!(out["next"].get("continueWalk").is_none(), "{out}");
    assert!(
        out["warnings"].to_string().contains("cannot skip the rows"),
        "{out}"
    );
}

/// Entries the walk counted in an earlier window (withheld by policy,
/// dot entries out of view) are not counted again by a window that replays
/// them.
#[test]
fn a_resumed_window_does_not_count_the_withheld_and_hidden_entries_it_replays() {
    let root = Fixture::new();
    root.write(".ssh/id.rs", "fn f() {}\n");
    root.write(".npmrc", "x\n");
    root.write(".dot.rs", "fn f() {}\n");
    root.write("a.rs", "fn f() {}\n");
    root.write("b.rs", "fn f() {}\n");
    for operation in ["files", "tree"] {
        let withheld = |query: &Value| {
            let out = within(&root.0, query.clone(), Duration::from_secs(3600));
            (
                skipped_count(&out, "withheld") > 0,
                skipped_count(&out, "hidden") > 0,
            )
        };
        let mut query = rows(operation, &root.0);
        query["includeHidden"] = json!(true);
        let (withheld_whole, _) = withheld(&query);
        assert!(withheld_whole, "{operation}: the whole walk counts them");
        // `.dot.rs`, `.npmrc`, `.ssh` and `a.rs` come first: replayed.
        query["walkOffset"] = json!(4);
        assert_eq!(withheld(&query), (false, false), "{operation}");
        // The resumed window's own entries are counted.
        query["walkOffset"] = json!(1);
        assert!(withheld(&query).0, "{operation}: `.npmrc` is new here");
    }
    // A tree without `includeHidden` counts the dot entry it hides once (the
    // withheld ones are no hidden entries).
    let mut query = rows("tree", &root.0);
    let whole = within(&root.0, query.clone(), Duration::from_secs(3600));
    assert_eq!(skipped_count(&whole, "hidden"), 1, "{whole}");
    query["walkOffset"] = json!(4);
    let resumed = within(&root.0, query, Duration::from_secs(3600));
    assert_eq!(skipped_count(&resumed, "hidden"), 0, "{resumed}");
}

/// Takes its time at every entry and counts how many entries it saw.
struct Counting(std::sync::atomic::AtomicUsize);

impl CancellationCheck for Counting {
    fn check(&self) -> Result<(), String> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }
}

fn deep_tree() -> Fixture {
    let root = Fixture::new();
    for dir in 0..10 {
        for file in 0..10 {
            root.write(&format!("d{dir}/f{file}.rs"), "fn f() {}\n");
        }
    }
    root
}

/// The first window of a walk cut by time: its entries, the query that
/// resumes it and the offset that query names.
fn first_cut(root: &Fixture, operation: &str) -> (Vec<String>, Value, u64) {
    let query = rows(operation, &root.0);
    let out = within_checked(&root.0, query, STEP * 40, &Slow);
    let next = out["next"]["continueWalk"]["input"]["queries"][0].clone();
    let at = next["walkOffset"].as_u64().expect("the walk was cut");
    (listed_entries(&out), next, at)
}

fn checks(root: &Fixture, query: &Value) -> usize {
    let counting = Counting(std::sync::atomic::AtomicUsize::new(0));
    within_checked(&root.0, query.clone(), UNSPENT, &counting);
    counting.0.into_inner()
}

/// A resumed window does not replay the entries before its offset: it
/// continues where the window it resumes stopped, so a deep resume costs what
/// its own entries cost.
#[test]
fn a_resumed_window_does_not_replay_the_entries_before_its_offset() {
    for operation in ["files", "tree"] {
        let root = deep_tree();
        let whole = checks(&root, &rows(operation, &root.0));
        let (_, resume, at) = first_cut(&root, operation);
        assert!(at >= 10, "{operation}: the first window went deep: {at}");
        let resumed = checks(&root, &resume);
        assert!(
            resumed + at as usize <= whole + 2,
            "{operation}: resumed at {at} saw {resumed} entries of a walk of {whole}: it replayed"
        );
    }
}

/// A resume whose tree changed since the window it continues (an entry
/// created in a directory the walk is inside) replays instead, and lists
/// every entry once.
#[test]
fn a_resumed_window_of_a_changed_tree_replays_and_lists_every_entry_once() {
    for operation in ["files", "tree"] {
        let root = deep_tree();
        let whole = {
            let mut all = listed_entries(&run(&root.0, rows(operation, &root.0)).expect("whole"));
            all.sort();
            all
        };
        let (first, resume, _) = first_cut(&root, operation);
        // Sorts after every entry of the walk, so after the cut.
        std::thread::sleep(Duration::from_millis(30));
        root.write("zzz.rs", "fn f() {}\n");
        let mut chained = Vec::new();
        let mut query = resume;
        loop {
            assert!(chained.len() < 200, "{operation}: the walk never finishes");
            let out = within_checked(&root.0, query.clone(), UNSPENT, &NeverCancel);
            chained.extend(listed_entries(&out));
            match out["next"].get("continueWalk") {
                Some(next) => query = next["input"]["queries"][0].clone(),
                None => break,
            }
        }
        // The resumed windows list what the first window did not, plus the
        // newcomer, once each.
        let mut rest = chained.clone();
        rest.sort();
        rest.dedup();
        assert_eq!(rest.len(), chained.len(), "{operation}: a repeat");
        assert!(
            chained.iter().any(|path| path.ends_with("zzz.rs")),
            "{operation}: the newcomer is listed"
        );
        let mut joined = first;
        joined.extend(
            chained
                .iter()
                .filter(|path| !path.ends_with("zzz.rs"))
                .cloned(),
        );
        joined.sort();
        assert_eq!(joined, whole, "{operation}: every entry exactly once");
    }
}
