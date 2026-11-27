//! A sorted listing ranks its whole scope whatever its time budget does: a
//! window the budget pauses lists no row and resumes with what it ranked, so
//! every window lists the rows a walk that never paused lists, in its order.
use super::{STEP, Slow, UNSPENT, listed_entries, within_checked};
use octocode_runtime_base::execution::cancel::{CancellationCheck, NeverCancel};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::test_support::{Fixture, assert_row_contract};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::time::Duration;

/// Twelve files over three directories, with sizes and line counts that rank
/// in another order than the walk's: the largest is neither first nor last.
fn spread() -> Fixture {
    let root = Fixture::new();
    for index in 0..12_usize {
        let dir = ["a", "b", "c/d"][index % 3];
        root.write(
            &format!("{dir}/f{index:02}.rs"),
            "x\n".repeat((index * 7) % 12 + 1),
        );
    }
    root
}

/// Every row a listing lists, window after window, in listing order, each
/// window under the next budget of `budgets` (the last repeats). `forget`
/// drops what paused windows kept before each call, as another process
/// would. Returns the rows and the windows the budget paused.
fn chain(root: &Fixture, start: Value, budgets: &[Duration], forget: bool) -> (Vec<String>, usize) {
    let (mut rows, mut paused, mut query) = (Vec::new(), 0, start);
    // Each chain's first window walks under its own budget, not served from a
    // ranking an earlier chain kept.
    let _walking = super::super::super::windows::walking_firsts();
    for call in 0.. {
        assert!(call < 400, "the listing never finishes: {query}");
        let budget = budgets[call.min(budgets.len() - 1)];
        let cancel: &dyn CancellationCheck = if budget < UNSPENT {
            &Slow
        } else {
            &NeverCancel
        };
        if forget {
            super::super::super::standing::forget_kept(&root.0);
        }
        let out = within_checked(&root.0, query.clone(), budget, cancel);
        assert_row_contract(ToolId::StructureSearch, &out);
        assert!(out.get("terminalLimit").is_none(), "{out}");
        let listed = listed_entries(&out);
        if out["warnings"]
            .to_string()
            .contains("paused at its time budget")
        {
            paused += 1;
            assert!(listed.is_empty(), "a paused window lists no row: {out}");
            assert_eq!(
                out["partialReasons"],
                json_fixture(r#"["walkDeadline"]"#),
                "{out}"
            );
        }
        rows.extend(listed);
        let Some(next) = out["next"].get("nextPage") else {
            return (rows, paused);
        };
        query = next["input"]["queries"][0].clone();
    }
    unreachable!()
}

fn sorted(root: &Fixture, by: &str, order: &str) -> Value {
    json!({
        "operation": "files",
        "path": root.0,
        "entryType": "file",
        "sort": {"by": by, "order": order},
    })
}

#[test]
fn a_sorted_listing_lists_the_same_rows_in_the_same_order_whatever_the_clock_does() {
    let root = spread();
    let schedules: [&[Duration]; 5] = [
        &[STEP],
        &[STEP * 2],
        &[STEP * 5],
        &[STEP * 3, UNSPENT],
        &[UNSPENT, STEP * 2],
    ];
    for (by, order) in [
        ("bytes", "desc"),
        ("bytes", "asc"),
        ("totalLines", "desc"),
        ("modifiedAt", "desc"),
        ("name", "desc"),
        ("path", "asc"),
    ] {
        let start = sorted(&root, by, order);
        let (whole, unpaused) =
            super::super::with_window(3, || chain(&root, start.clone(), &[UNSPENT], false));
        assert_eq!(whole.len(), 12, "{by}: {whole:?}");
        assert_eq!(unpaused, 0);
        for schedule in schedules {
            for forget in [false, true] {
                let (rows, paused) =
                    super::super::with_window(3, || chain(&root, start.clone(), schedule, forget));
                assert_eq!(rows, whole, "{by} {order} {schedule:?} forget={forget}");
                // Later windows of a first walk that never paused are served
                // from the listing it kept, not walked.
                assert!(
                    paused > 0 || schedule[0] == UNSPENT,
                    "{by} {schedule:?}: the budget never paused the walk"
                );
            }
        }
    }
}

/// The largest file is the first row once the walk ends, however many
/// windows the budget paused before it.
#[test]
fn the_largest_file_heads_the_first_rows_a_paused_walk_lists() {
    let root = spread();
    let largest = "c/d/f05.rs";
    let mut query = sorted(&root, "bytes", "desc");
    query["limit"] = json!(1);
    let mut calls = 0;
    let first = loop {
        calls += 1;
        let out = within_checked(&root.0, query.clone(), STEP * 2, &Slow);
        let listed = listed_entries(&out);
        if let Some(first) = listed.first() {
            break first.clone();
        }
        query = out["next"]["nextPage"]["input"]["queries"][0].clone();
        assert!(
            query["walkOffset"].as_u64().is_some(),
            "a pause resumes the walk: {out}"
        );
    };
    assert!(first.ends_with(largest), "{first}");
    assert!(calls > 1, "the budget never paused the walk");
}
