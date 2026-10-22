//! A listing walk is bounded by time: a walk out of budget returns what it
//! scanned, names where it stopped, and leads to narrower scopes, never an
//! empty or whole-looking page.
use super::super::{StructureSearchQuery, execute_structure_within};
use super::run;
use crate::execution::cancel::{CancellationCheck, NeverCancel};
use crate::id::ToolId;
use crate::tools::test_support::{Fixture, assert_row_contract, simple_policy};
use serde_json::{Value, json};
use std::time::Duration;

fn tree() -> Fixture {
    let root = Fixture::new();
    for path in ["a/one.rs", "b/two.rs", "b/deep/three.rs", "top.rs"] {
        root.write(path, "fn f() {}\n");
    }
    root
}

fn within(root: &std::path::Path, query: Value, budget: Duration) -> Value {
    within_checked(root, query, budget, &NeverCancel)
}

fn within_checked(
    root: &std::path::Path,
    query: Value,
    budget: Duration,
    cancel: &dyn CancellationCheck,
) -> Value {
    let (paths, security) = simple_policy(root);
    let query: StructureSearchQuery = serde_json::from_value(query).expect("typed row");
    execute_structure_within(&query, &paths, &security, cancel, None, budget).expect("listing")
}

/// The walk time `Slow` spends at one entry; the small budgets below are
/// fractions of whole steps, so they cut a fixture walk after a few entries.
const STEP: Duration = Duration::from_millis(3);

/// A budget no fixture walk spends: its windows are cut by `maxEntries` alone.
const UNSPENT: Duration = Duration::from_secs(3600);

/// Spends [`STEP`] of the walk clock at every entry the walk reaches, so a
/// small budget cuts a fixture walk after a few entries, at the same entry
/// on every run; the schedules vary where the cuts fall.
struct Slow;

impl CancellationCheck for Slow {
    fn check(&self) -> Result<(), String> {
        octocode_engine::scan_budget::advance_walk_clock(STEP);
        Ok(())
    }
}

fn rows(operation: &str, root: &std::path::Path) -> Value {
    json!({"operation": operation, "path": root, "include": ["*.rs"]})
}

#[test]
fn a_listing_out_of_time_is_partial_names_where_it_stopped_and_leads_narrower() {
    for operation in ["files", "tree"] {
        let root = tree();
        let query = rows(operation, &root.0);
        let out = within(&root.0, query.clone(), Duration::ZERO);
        assert_row_contract(ToolId::StructureSearch, &query, &out);
        assert_eq!(out["isPartial"], true, "{operation}: {out}");
        assert_eq!(out["partialReasons"], json!(["walkDeadline"]), "{out}");
        assert_eq!(out["terminalLimit"], true, "{out}");
        // Nothing was scanned: absence is unproven, so never `empty`.
        assert!(out.get("status").is_none(), "{operation}: {out}");
        let warning = out["warnings"]
            .as_array()
            .and_then(|w| {
                w.iter()
                    .filter_map(Value::as_str)
                    .find(|w| w.contains("time budget"))
            })
            .unwrap_or_else(|| panic!("{operation} warns of the time budget: {out}"));
        assert!(warning.starts_with("terminalLimit:"), "{warning}");
        assert!(
            warning.contains("before `a`"),
            "names the first unscanned entry: {warning}"
        );
        let narrow = &out["next"]["narrowScope"]["query"]["queries"][0];
        assert_eq!(narrow["operation"], "tree", "{out}");
        assert_eq!(narrow["maxDepth"], 1, "{out}");
        assert!(out["next"].get("continueWalk").is_none(), "{out}");
    }
}

#[test]
fn a_listing_within_its_time_is_whole() {
    for operation in ["files", "tree"] {
        let root = tree();
        let out = within(&root.0, rows(operation, &root.0), Duration::from_secs(3600));
        assert!(out.get("isPartial").is_none(), "{operation}: {out}");
        assert!(out.get("terminalLimit").is_none(), "{out}");
        assert!(out.get("status").is_none(), "{out}");
        assert!(
            !out["files"].as_array().is_some_and(Vec::is_empty),
            "{operation}: {out}"
        );
    }
}

/// The production entry runs under a real budget, far above a fixture walk.
#[test]
fn the_default_entry_is_not_cut_on_a_small_tree() {
    let root = tree();
    let out = run(&root.0, rows("files", &root.0)).expect("listing");
    assert!(out.get("isPartial").is_none(), "{out}");
}

/// The entries one page lists, as `dir/entry` strings.
fn listed_entries(out: &Value) -> Vec<String> {
    super::listed(out)
        .iter()
        .map(|entry| entry["path"].as_str().expect("path").to_owned())
        .collect()
}

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
            assert_row_contract(ToolId::StructureSearch, &query, &out);
            chained.extend(listed_entries(&out));
            let Some(next) = out["next"].get("continueWalk") else {
                assert!(out.get("isPartial").is_none(), "{operation}: {out}");
                break;
            };
            // A resumable cut is no terminal limit and offers no way out of it.
            assert_eq!(out["isPartial"], true, "{operation}: {out}");
            assert_eq!(out["partialReasons"], json!(["walkDeadline"]), "{out}");
            assert!(out.get("terminalLimit").is_none(), "{operation}: {out}");
            assert!(out["next"].get("narrowScope").is_none(), "{out}");
            assert!(
                !out["warnings"].to_string().contains("terminalLimit"),
                "{operation}: {out}"
            );
            assert!(out.get("status").is_none(), "{operation}: {out}");
            let resumed = &next["query"]["queries"][0];
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
        let retry = &out["next"]["includeSkipped"]["query"]["queries"][0];
        assert_eq!(retry["includeIgnored"], true, "{operation}: {out}");
        assert!(retry.get("walkOffset").is_none(), "{operation}: {retry}");
    }
}

/// Runs a listing through every window its `continueWalk` offers (resuming
/// after a time cut, widening after a `maxEntries` cut), each under the next
/// budget of `budgets` (the last repeats): timing that cuts one window early
/// and another late. Returns the rows each window listed, and how many
/// windows the time budget cut.
fn follow_windows(
    root: &std::path::Path,
    start: Value,
    budgets: &[Duration],
) -> (Vec<Vec<String>>, usize) {
    let (outs, resumed) = follow_outputs(root, start, budgets);
    let time_cuts = resumed.iter().filter(|resumed| **resumed).count();
    (outs.iter().map(listed_entries).collect(), time_cuts)
}

/// [`follow_windows`], keeping each window's whole response, and for each
/// window whether its `continueWalk` resumes a time cut: a resume moves
/// `walkOffset`, a widened `maxEntries` keeps the window's own.
fn follow_outputs(
    root: &std::path::Path,
    start: Value,
    budgets: &[Duration],
) -> (Vec<Value>, Vec<bool>) {
    let mut windows = Vec::new();
    let mut resumed = Vec::new();
    let mut query = start;
    loop {
        let budget = budgets[windows.len().min(budgets.len() - 1)];
        assert!(windows.len() < 60, "the walk never finishes: {query}");
        // Only a budget a window can spend needs the walk slowed down.
        let cancel: &dyn CancellationCheck = if budget < UNSPENT {
            &Slow
        } else {
            &NeverCancel
        };
        let out = within_checked(root, query.clone(), budget, cancel);
        assert_row_contract(ToolId::StructureSearch, &query, &out);
        assert!(
            out["next"].get("nextPage").is_none(),
            "a fixture window is one page: {out}"
        );
        let next = out["next"]
            .get("continueWalk")
            .map(|next| next["query"]["queries"][0].clone());
        resumed.push(
            next.as_ref()
                .is_some_and(|next| next.get("walkOffset") != query.get("walkOffset")),
        );
        let terminal = out.get("terminalLimit").is_some();
        windows.push(out);
        let Some(next) = next else {
            assert!(!terminal, "the chain ended on a terminal limit");
            return (windows, resumed);
        };
        query = next;
    }
}

fn flat_listing(operation: &str, root: &Fixture, extra: Value) -> Value {
    for index in 0..9 {
        root.write(&format!("f{index}.rs"), "fn f() {}\n");
    }
    let mut query = rows(operation, &root.0);
    query
        .as_object_mut()
        .expect("row")
        .extend(extra.as_object().expect("extra").clone());
    query
}

#[test]
fn windows_cut_by_maxentries_and_by_time_never_overlap_whatever_the_timing() {
    let big = UNSPENT;
    let small = STEP * 8 / 3;
    let schedules: [&[Duration]; 4] = [
        &[big, small, big],
        &[small, big],
        &[big, small, small, big],
        &[small],
    ];
    for (operation, extra) in [
        ("tree", json!({})),
        ("files", json!({})),
        ("files", json!({"sort": "path"})),
    ] {
        let mut time_cuts = 0;
        for max_entries in [2, 3, 5] {
            for budgets in schedules {
                let root = Fixture::new();
                let mut start = flat_listing(operation, &root, extra.clone());
                let whole = {
                    let mut all = listed_entries(&run(&root.0, start.clone()).expect("whole"));
                    all.sort();
                    all
                };
                assert_eq!(whole.len(), 9, "{operation}: {whole:?}");
                start["maxEntries"] = json!(max_entries);
                let (windows, cuts) = follow_windows(&root.0, start, budgets);
                time_cuts += cuts;
                let mut chained = windows.concat();
                chained.sort();
                assert_eq!(
                    chained, whole,
                    "{operation} maxEntries {max_entries} {budgets:?}: every entry exactly once, windows {windows:?}"
                );
            }
        }
        // The properties above are about mixed cuts: the small budgets did
        // cut walks (each entry takes `STEP`, and the budget is a few).
        assert!(time_cuts > 0, "{operation}: no window was cut by time");
    }
}

/// A listing sorted by something else than walk order and cut by time lists
/// its walk's first `maxEntries` rows in walk order, so the resumed walk
/// reaches the rest: no row is lost to a sort a cut walk ranks differently.
#[test]
fn a_sorted_listing_cut_by_time_resumes_exactly() {
    for sort in ["name", "size", "modified", "lines"] {
        let root = Fixture::new();
        let mut start = flat_listing("files", &root, json!({"sort": sort}));
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
        let widen = first["next"]["continueWalk"]["query"]["queries"][0].clone();
        assert_eq!(widen["scanOffset"], 4, "{operation}: {first}");
        // The widened walk is cut after two or three entries, before the four
        // rows the first window listed.
        let second = within_checked(&root.0, widen, STEP * 5 / 3, &Slow);
        let cut = second["next"]["continueWalk"]["query"]["queries"][0].clone();
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
    let mut query = flat_listing("files", &root, json!({"sort": "name"}));
    query["scanOffset"] = json!(4);
    query["maxEntries"] = json!(8);
    let out = within_checked(&root.0, query.clone(), STEP * 5 / 3, &Slow);
    assert_row_contract(ToolId::StructureSearch, &query, &out);
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
                out["warnings"].to_string().contains("withheld"),
                out["summary"].get("hidden").is_some(),
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
    assert_eq!(whole["summary"]["hidden"], 1, "{whole}");
    query["walkOffset"] = json!(4);
    let resumed = within(&root.0, query, Duration::from_secs(3600));
    assert!(resumed["summary"].get("hidden").is_none(), "{resumed}");
}

/// What a window's page says its walk left out: entries `.gitignore` hid, dot
/// entries skipped, and entries the path policy withheld.
fn left_out(out: &Value) -> (u64, u64, u64) {
    let count = |key| out["summary"][key].as_u64().unwrap_or(0);
    let withheld = out["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|warning| warning.contains("withheld"))
        .map(|warning| {
            warning
                .split(|c: char| !c.is_ascii_digit())
                .find_map(|digits| digits.parse::<u64>().ok())
                .expect("the notice counts the entries")
        })
        .sum();
    (count("ignored"), count("hidden"), withheld)
}

/// A walk's windows count what the walk left out once between them: a window
/// that ends at its `maxEntries`-th row leaves the entries after it to the
/// window that resumes there, and neither counts them twice. (A widening
/// `continueWalk` walks the same walk again, so it is no resume: only chains
/// of resuming windows, whose query carries `walkOffset`, add up.)
#[test]
fn windows_count_what_the_walk_left_out_once_whatever_the_timing() {
    let big = UNSPENT;
    let small = STEP * 8 / 3;
    // Budgets that span several entries past `maxEntries` rows cut a window
    // at its rows, with entries left out after the last of them.
    let (long, longer) = (STEP * 6, STEP * 11);
    let schedules: [&[Duration]; 6] = [
        &[small],
        &[big, small, big],
        &[small, big],
        &[long],
        &[longer],
        &[long, longer, small],
    ];
    for operation in ["tree", "files"] {
        let root = Fixture::new();
        root.write(".git/HEAD", "ref: refs/heads/main\n");
        root.write(".gitignore", "*.log\n");
        for index in 0..6 {
            root.write(&format!("s{index}.rs"), "fn f() {}\n");
            root.write(&format!("s{index}.log"), "x\n");
            root.write(&format!("s{index}.pem"), "x\n");
            root.write(&format!("t{index}/.hidden"), "x\n");
            root.write(&format!("t{index}/u.rs"), "fn f() {}\n");
        }
        let start = json!({"operation": operation, "path": root.0, "includeHidden": false});
        let whole = left_out(&run(&root.0, start.clone()).expect("whole"));
        assert!(
            whole.0 > 0 && whole.1 > 0 && whole.2 > 0,
            "{operation}: the fixture leaves out all three: {whole:?}"
        );
        let (mut time_cuts, mut chains) = (0, 0);
        for max_entries in [2, 4] {
            for budgets in schedules {
                let mut start = start.clone();
                start["maxEntries"] = json!(max_entries);
                let (outs, resumed) = follow_outputs(&root.0, start, budgets);
                time_cuts += resumed.iter().filter(|resumed| **resumed).count();
                let resumes = resumed[..resumed.len() - 1].iter().all(|resumed| *resumed);
                if !resumes {
                    continue;
                }
                chains += 1;
                let summed = outs.iter().map(left_out).fold((0, 0, 0), |sum, window| {
                    (sum.0 + window.0, sum.1 + window.1, sum.2 + window.2)
                });
                assert_eq!(
                    summed,
                    whole,
                    "{operation} maxEntries {max_entries} {budgets:?}: counted once across {} windows",
                    outs.len()
                );
            }
        }
        assert!(time_cuts > 0, "{operation}: no window was cut by time");
        assert!(chains > 0, "{operation}: no chain of resumed windows");
    }
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
    let next = out["next"]["continueWalk"]["query"]["queries"][0].clone();
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
                Some(next) => query = next["query"]["queries"][0].clone(),
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
