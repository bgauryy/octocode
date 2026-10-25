//! A listing walk is bounded by time: a walk out of budget returns what it
//! scanned, names where it stopped, and leads to narrower scopes, never an
//! empty or whole-looking page.
use super::super::{StructureSearchQuery, execute_structure_within};
use super::{related, run};
use crate::execution::cancel::{CancellationCheck, NeverCancel};
use crate::id::ToolId;
use crate::tools::test_support::{Fixture, assert_row_contract, simple_policy, skipped_count};
use serde_json::{Value, json};
use std::time::Duration;

mod resume;
mod windows;

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
    {
        let mut row =
            json!({"operation": operation, "path": root, "include": ["*.rs"], "maxDepth": 10});
        if operation == "files" {
            row["sort"] = json!({"by":"traversal","order":"asc"});
        }
        row
    }
}

#[test]
fn a_listing_out_of_time_is_partial_names_where_it_stopped_and_leads_narrower() {
    for operation in ["files", "tree"] {
        let root = tree();
        let query = rows(operation, &root.0);
        let out = within(&root.0, query.clone(), Duration::ZERO);
        assert_row_contract(ToolId::StructureSearch, &out);
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
        let narrow = &related(&out, "structureSearch")["input"]["queries"][0];
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
        assert_row_contract(ToolId::StructureSearch, &out);
        assert!(
            out["next"].get("nextPage").is_none(),
            "a fixture window is one page: {out}"
        );
        let next = out["next"]
            .get("continueWalk")
            .map(|next| next["input"]["queries"][0].clone());
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
