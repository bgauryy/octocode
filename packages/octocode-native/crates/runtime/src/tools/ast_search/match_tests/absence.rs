//! Absence needs a disclosure the agent can see: a window that states it, on a
//! scan its budget cut, names what the skipped walks left unproven.
use super::*;
use crate::tools::empty_walk::with_probe_budget;
use serde_json::Value;
use std::time::Duration;

const UNCHECKED: &str = "not checked in time";

fn no_hit_row(root: &std::path::Path) -> Value {
    json!({"operation":"match","path":root,"parser":"rust","rule":{"kind":"identifier","regex":"^zzqneverzz$"}})
}

fn discloses(out: &Value) -> bool {
    let hinted = out["hints"].to_string().contains(UNCHECKED);
    assert_eq!(hinted, skipped_call(out).is_some(), "{out}");
    hinted
}

/// Every window of a scan cut at each file, following `expandScan`.
fn windows(root: &std::path::Path, row: Value) -> Vec<Value> {
    let mut windows = vec![run(root, row).expect("first window")];
    loop {
        let next = windows[windows.len() - 1]["next"]["expandScan"]["input"]["queries"][0].clone();
        if !next.is_object() {
            return windows;
        }
        windows.push(run(root, next).expect("next window"));
    }
}

/// The scan needs more than one window and its probe is cut: every window that
/// says `empty` carries the disclosure, and so does the first one.
#[test]
fn a_cut_scan_with_a_cut_probe_never_states_absence_undisclosed() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs", "dist/late.rs"] {
        root.write(name, "pub fn source() {}\n");
    }
    let _window = crate::tools::ast_search::scan_window::test_window(1);
    let walked = with_probe_budget(Duration::ZERO, || windows(&root.0, no_hit_row(&root.0)));
    assert!(walked.len() > 1, "{walked:?}");
    let last = walked.last().expect("a window");
    assert_eq!(last["status"], "empty", "{walked:?}");
    assert!(discloses(last), "{last}");
    assert!(discloses(&walked[0]), "{}", walked[0]);
    for window in &walked {
        if window["status"] == "empty" {
            assert!(discloses(window), "{window}");
        }
    }
}

/// A later window with rows states no absence: it repeats nothing, though the
/// first window was cut with the check unfinished and said so there.
#[test]
fn a_cut_first_window_with_hits_says_the_check_was_cut_and_later_hits_do_not() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "dist/late.rs"] {
        root.write(name, "pub fn source() {}\n");
    }
    let row =
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"kind":"function_item"}});
    let _window = crate::tools::ast_search::scan_window::test_window(1);
    let walked = with_probe_budget(Duration::ZERO, || windows(&root.0, row));
    assert_eq!(walked[0]["matchCount"], 1, "{walked:?}");
    assert!(discloses(&walked[0]), "{}", walked[0]);
    assert_eq!(walked[1]["matchCount"], 1, "{walked:?}");
    assert!(!discloses(&walked[1]), "{}", walked[1]);
}

/// A probe that finished states what it found on the last window too, when
/// that window states absence: its count, not an unchecked claim.
#[test]
fn an_empty_last_window_names_what_a_finished_probe_found() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "dist/late.rs"] {
        root.write(name, "pub fn source() {}\n");
    }
    let _window = crate::tools::ast_search::scan_window::test_window(1);
    let walked = windows(&root.0, no_hit_row(&root.0));
    let last = walked.last().expect("a window");
    assert_eq!(last["status"], "empty", "{walked:?}");
    assert!(
        last["hints"]
            .to_string()
            .contains("1 rust file behind skipped paths (dist/)"),
        "{last}"
    );
    assert!(skipped_call(last).is_some(), "{last}");
}

/// A scan window cut before it parsed anything still names the skipped walks:
/// its own cut diagnostic does not stand in for them.
#[test]
fn a_first_window_that_parsed_nothing_still_discloses_a_cut_probe() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), b"\0\xff binary \0").expect("binary");
    root.write("dist/late.rs", "pub fn source() {}\n");
    root.write("z.rs", "pub fn source() {}\n");
    let _window = crate::tools::ast_search::scan_window::test_window(1);
    let first = with_probe_budget(Duration::ZERO, || {
        run(&root.0, no_hit_row(&root.0)).expect("first window")
    });
    assert!(first["next"]["expandScan"].is_object(), "{first}");
    assert!(discloses(&first), "{first}");
}

/// A later window that parsed nothing says nothing of the files earlier windows
/// parsed: the scope is not shown to hold none.
#[test]
fn a_later_window_does_not_claim_the_scope_holds_no_file() {
    let root = Fixture::new();
    root.write("a.rs", "pub fn source() {}\n");
    let mut row = no_hit_row(&root.0);
    row["scanOffset"] = json!(5);
    let out = run(&root.0, row).expect("past the last file");
    assert_eq!(out["status"], "empty", "{out}");
    assert!(!out["hints"].to_string().contains("was parsed"), "{out}");
}

/// The same result answers `coverage` the same on each of its match pages, an
/// unchecked probe or not: the requested scope was walked whole, and the hint
/// names what the check of the skipped paths left open.
#[test]
fn every_match_page_states_the_same_coverage_after_an_unchecked_probe() {
    let root = Fixture::new();
    let body = (0..2000)
        .map(|n| format!("fn function_{n}() {{}}\n"))
        .collect::<String>();
    root.write("a.rs", &body);
    root.write("dist/late.rs", "pub fn source() {}\n");
    let row =
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"kind":"function_item"}});
    let (first, later) = with_probe_budget(Duration::ZERO, || {
        let first = run(&root.0, row).expect("page 1");
        let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
        assert_eq!(next["matchPage"], 2, "{first}");
        let later = run(&root.0, next).expect("match page 2");
        (first, later)
    });
    assert!(discloses(&first), "{first}");
    assert!(!discloses(&later), "{later}");
    assert_eq!(first["coverage"]["state"], "complete", "{first}");
    assert_eq!(first["coverage"], later["coverage"], "{first}\n{later}");
}
