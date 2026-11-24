//! Absence needs a disclosure the agent can see: a window that states it, on a
//! scan its budget cut, names the paths its walk skipped, which were not
//! searched.
use super::*;
use octocode_test_support::json_fixture;
use serde_json::Value;

const UNCHECKED: &str = "were not searched";

fn no_hit_row(root: &std::path::Path) -> Value {
    json!({"operation":"match","path":root,"parser":"rust","rule":{"kind":"identifier","regex":"^zzqneverzz$"}})
}

fn discloses(out: &Value) -> bool {
    // An empty row says it as its hint, a row with evidence as a warning.
    let channel = if out["status"] == "empty" {
        "hints"
    } else {
        "warnings"
    };
    let hinted = out[channel].to_string().contains(UNCHECKED);
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

/// The scan needs more than one window: the window that states absence (the
/// last, empty, with nothing left to scan) names what its walk skipped.
#[test]
fn a_cut_scan_never_states_absence_undisclosed() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "c.rs", "dist/late.rs"] {
        root.write(name, "pub fn source() {}\n");
    }
    let _window = crate::ast_search::scan_window::test_window(1);
    let walked = windows(&root.0, no_hit_row(&root.0));
    assert!(walked.len() > 1, "{walked:?}");
    let last = walked.last().expect("a window");
    assert_eq!(last["status"], "empty", "{walked:?}");
    assert!(discloses(last), "{last}");
}

/// A window with rows already answers: it names no skipped path, first or
/// later.
#[test]
fn a_window_with_hits_names_no_skipped_path() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "dist/late.rs"] {
        root.write(name, "pub fn source() {}\n");
    }
    let row =
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"kind":"function_item"}});
    let _window = crate::ast_search::scan_window::test_window(1);
    let walked = windows(&root.0, row);
    // A window cut before the scope ends counts what it read, a floor; the
    // window that ends the scope counts every window's rows.
    assert_eq!(walked[0]["pagination"]["total"], 1, "{walked:?}");
    assert_eq!(
        walked[0]["pagination"]["countScope"], "loaded",
        "{walked:?}"
    );
    assert!(!discloses(&walked[0]), "{}", walked[0]);
    assert_eq!(walked[1]["matchCount"], 2, "{walked:?}");
    assert_eq!(walked[1]["pagination"]["offset"], 1, "{walked:?}");
    assert!(!discloses(&walked[1]), "{}", walked[1]);
}

/// The last window states absence by what its walk skipped, with the lead
/// that lifts its cause; nothing behind it was read.
#[test]
fn an_empty_last_window_names_its_skipped_paths() {
    let root = Fixture::new();
    for name in ["a.rs", "b.rs", "dist/late.rs"] {
        root.write(name, "pub fn source() {}\n");
    }
    let _window = crate::ast_search::scan_window::test_window(1);
    let walked = windows(&root.0, no_hit_row(&root.0));
    let last = walked.last().expect("a window");
    assert_eq!(last["status"], "empty", "{walked:?}");
    assert_unsearched(
        last,
        &[octocode_tools_shared::empty_walk::Cause::DefaultExcluded],
    );
}

/// A scan window cut before it parsed anything still names the paths its walk
/// skipped: its own cut diagnostic does not stand in for them.
#[test]
fn a_first_window_that_parsed_nothing_still_discloses_its_skipped_paths() {
    let root = Fixture::new();
    std::fs::write(root.0.join("a.rs"), b"\0\xff binary \0").expect("binary");
    root.write("dist/late.rs", "pub fn source() {}\n");
    root.write("z.rs", "pub fn source() {}\n");
    let _window = crate::ast_search::scan_window::test_window(1);
    let first = run(&root.0, no_hit_row(&root.0)).expect("first window");
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

/// The same result answers `coverage` the same on each of its pages: the
/// requested scope was walked whole. Its skipped paths are counted on page 1,
/// never offered beside the matches.
#[test]
fn every_match_page_states_the_same_coverage() {
    let root = Fixture::new();
    let body = (0..2000)
        .map(|n| format!("fn function_{n}() {{}}\n"))
        .collect::<String>();
    root.write("a.rs", &body);
    root.write("dist/late.rs", "pub fn source() {}\n");
    let row =
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"kind":"function_item"}});
    let first = run(&root.0, row).expect("page 1");
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    assert_eq!(next["page"], 2, "{first}");
    let later = run(&root.0, next).expect("page 2");
    assert!(!discloses(&first), "{first}");
    assert!(!discloses(&later), "{later}");
    assert!(
        first["skippedPaths"].to_string().contains("dist"),
        "{first}"
    );
    assert_eq!(first["coverage"]["state"], "complete", "{first}");
    assert_eq!(first["coverage"], later["coverage"], "{first}\n{later}");
}

/// An include glob that matches no file is said as localSearch says it (one
/// shared explanation of how a `/` glob anchors, and the listing that names
/// the files to fix it with), not as an unparsed grammar.
#[test]
fn an_include_glob_that_matches_no_file_gets_the_shared_glob_hint_and_lead() {
    let root = Fixture::new();
    root.write("src/a.rs", "pub fn source() {}\n");
    let mut row = no_hit_row(&root.0);
    row["include"] = json_fixture(r#"["wrong/src/*.rs"]"#);
    let out = run(&root.0, row).expect("no file matches the glob");
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(
        out["hints"],
        json!([octocode_runtime_base::policy::include::glob_miss_hint(
            0,
            &["wrong/src/*.rs".to_owned()],
            &[]
        )]),
        "{out}"
    );
    let lead =
        octocode_runtime_base::test_support::checks::related_call(&out, "structureSearch", |_| {
            true
        });
    assert_eq!(lead["input"]["queries"][0]["operation"], "files", "{out}");
    assert!(
        lead["input"]["queries"][0].get("include").is_none(),
        "{out}"
    );
}

/// Globs that matched only files the security path policy withholds say
/// that, as localSearch does, never that they matched no file: respelling
/// them cannot reach the file. A withheld file the globs do not match is no
/// part of the miss. A withheld dot file is withheld, not hidden: the row
/// says the same with and without `includeHidden`.
#[test]
fn globs_that_match_only_a_withheld_file_say_so_and_name_no_file() {
    let root = Fixture::new();
    root.write("src/a.rs", "pub fn source() {}\n");
    root.write(".env.rs", "pub fn key() {}\n");
    let hints = |include: &str, hidden: bool| {
        let mut row = no_hit_row(&root.0);
        row["include"] = json!([include]);
        if hidden {
            row["includeHidden"] = json!(true);
        }
        let out = run(&root.0, row).expect("no file is read");
        assert_eq!(out["status"], "empty", "{out}");
        assert!(!out["hints"].to_string().contains(".env"), "{out}");
        out["hints"].clone()
    };
    let globs = |glob: &str| [glob.to_owned()];
    let miss =
        octocode_runtime_base::policy::include::glob_miss_hint(0, &globs("absent/*.rs"), &[]);
    let withheld = octocode_runtime_base::policy::include::glob_miss_hint(1, &globs(".env*"), &[]);
    assert_ne!(withheld, miss);
    for hidden in [true, false] {
        assert_eq!(hints(".env*", hidden), json!([withheld]));
        assert_eq!(hints("absent/*.rs", hidden), json!([miss]));
    }
}

/// A directory scan whose every candidate was refused as text (here
/// binary content) states its coverage as an empty scan does:
/// `filesScanned` 0 beside the diagnostic naming the file.
#[test]
fn a_scan_whose_candidates_were_all_refused_states_it_scanned_none() {
    let root = Fixture::new();
    std::fs::write(root.0.join("latin.js"), b"function cafeFn(){}\ncafeFn();\0").expect("fixture");
    let out = run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"kind":"call_expression"}}),
    )
    .expect("match");
    assert_eq!(out["filesScanned"], 0, "{out}");
    assert!(out["diagnostics"].to_string().contains("latin.js"), "{out}");
}
