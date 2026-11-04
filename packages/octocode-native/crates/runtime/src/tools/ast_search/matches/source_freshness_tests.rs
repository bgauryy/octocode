use super::*;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::Value;
use serde_json::json;

fn run_row(row: &Value, paths: &PathPolicy, security: &crate::security::ContentSecurity) -> Value {
    let query = serde_json::from_value(row.clone()).expect("canonical match");
    super::super::execute_ast(
        &query,
        paths,
        security,
        &crate::execution::cancel::NeverCancel,
        Some(1),
    )
    .expect("budgeted source page")
}

fn query(path: &std::path::Path, rule: bool) -> Value {
    let mut query = json!({"operation":"match","path":path,"includeCaptures":true});
    if rule {
        query["rule"] = json!({"kind":"call_expression"});
    } else {
        query["rule"] = json!({"pattern":"target($A)"});
    }
    query
}

#[test]
fn copied_match_pages_reject_same_count_source_edits() {
    let original = "target(\"v0\");\ntarget(\"v1\");\ntarget(\"v2\");\n";
    for rule in [false, true] {
        for directory in [false, true] {
            for (edited, restore_mtime) in [
                (original.replace("v1", "changed"), false),
                (original.replace("v1", "w1"), true),
                (format!("// new header\n{original}"), false),
                (
                    format!("{original}// changed outside matched nodes\n"),
                    false,
                ),
            ] {
                let root = Fixture::new();
                let file = root.0.join("source.ts");
                std::fs::write(&file, original).expect("source");
                let (paths, security) = simple_policy(&root.0);
                let scope = if directory {
                    root.0.as_path()
                } else {
                    file.as_path()
                };
                let first = run_row(&query(scope, rule), &paths, &security);
                let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
                assert!(next.is_object(), "{first}");
                let copied = run_row(&next, &paths, &security);
                assert_eq!(copied["files"][0]["matches"][0]["value"], "target(\"v1\")");
                SCANS.evict(first["snapshot"].as_str().expect("snapshot"));
                let modified = std::fs::metadata(&file)
                    .expect("metadata")
                    .modified()
                    .expect("mtime");
                std::fs::write(&file, edited).expect("edit");
                if restore_mtime {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .open(&file)
                        .expect("open")
                        .set_times(std::fs::FileTimes::new().set_modified(modified))
                        .expect("restore mtime");
                }
                let changed = run_row(&next, &paths, &security);
                assert_eq!(
                    changed["errorCode"], "staleSnapshot",
                    "rule={rule}, directory={directory}: {changed}"
                );
            }
        }
    }
}

#[test]
fn same_runtime_match_memo_rejects_restored_mtime_edits() {
    for rule in [false, true] {
        for directory in [false, true] {
            let root = Fixture::new();
            let file = root.0.join("source.ts");
            std::fs::write(&file, "target(\"v0\");\ntarget(\"v1\");\n").expect("source");
            let (paths, security) = simple_policy(&root.0);
            octocode_engine::stamp::settle_written_so_far();
            let scope = if directory {
                root.0.as_path()
            } else {
                file.as_path()
            };
            let first = run_row(&query(scope, rule), &paths, &security);
            let snapshot = first["snapshot"].as_str().expect("snapshot");
            if cfg!(unix) {
                assert!(SCANS.holds(snapshot), "settled scan must exercise the memo");
            }
            let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
            let copied = run_row(&next, &paths, &security);
            assert_eq!(copied["files"][0]["matches"][0]["value"], "target(\"v1\")");
            let modified = std::fs::metadata(&file)
                .expect("metadata")
                .modified()
                .expect("mtime");
            std::fs::write(&file, "target(\"v0\");\ntarget(\"w1\");\n").expect("same size edit");
            std::fs::OpenOptions::new()
                .write(true)
                .open(&file)
                .expect("open")
                .set_times(std::fs::FileTimes::new().set_modified(modified))
                .expect("restore mtime");
            let changed = run_row(&next, &paths, &security);
            assert_eq!(
                changed["errorCode"], "staleSnapshot",
                "rule={rule}, directory={directory}: {changed}"
            );
            assert!(!SCANS.holds(snapshot), "changed scan must be evicted");
        }
    }
}

const OPEN: &str = "pub mod sub;\n";
const GATED: &str = "#[cfg(test)]\npub mod sub;\n";

/// `src/sub`, a module of `src/lib.rs` (outside the scans' scope) that says
/// `declares` it, with settled matches; the path of `lib.rs`.
fn module_beside_its_parent(root: &Fixture, declares: &str) -> std::path::PathBuf {
    let parent = root.write("src/lib.rs", declares);
    root.write("src/sub/mod.rs", "pub mod a;\n");
    root.write(
        "src/sub/a.rs",
        "fn f() {\n    a.unwrap();\n    b.unwrap();\n    c.unwrap();\n}\n",
    );
    octocode_engine::stamp::settle_written_so_far();
    parent
}

/// A match under a `testCode` scope follows a parent module outside its
/// scope: moving the module across the line leaves the pages stale, for a
/// directory and for one file.
#[test]
fn a_test_scoped_match_follows_a_parent_outside_its_scope() {
    for (test_code, before, after) in [("exclude", OPEN, GATED), ("only", GATED, OPEN)] {
        for scope in ["src/sub", "src/sub/a.rs"] {
            let root = Fixture::new();
            let parent = module_beside_its_parent(&root, before);
            let (paths, security) = simple_policy(&root.0);
            let row = json!({"operation":"match","path":root.0.join(scope),"rule":{"pattern":"$E.unwrap()"},"testCode":test_code});
            let first = run_row(&row, &paths, &security);
            let snapshot = first["snapshot"].as_str().expect("snapshot");
            let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
            let second = run_row(&next, &paths, &security);
            assert!(
                second.get("errorCode").is_none(),
                "{test_code} {scope}: {second}"
            );
            std::fs::write(&parent, after).expect("edit");
            let third = run_row(&next, &paths, &security);
            assert_eq!(
                third["errorCode"], "staleSnapshot",
                "{test_code} {scope}: {third}"
            );
            assert!(!SCANS.holds(snapshot), "{test_code} {scope}: not proved");
        }
    }
}

/// Without a filtering scope nothing depends on a parent: the scan stays
/// stored and serves its later pages, whatever a parent says.
#[test]
fn a_match_with_no_test_code_filter_keeps_its_scan() {
    for test_code in [None, Some("include")] {
        let root = Fixture::new();
        let parent = module_beside_its_parent(&root, OPEN);
        let (paths, security) = simple_policy(&root.0);
        let mut row = json!({"operation":"match","path":root.0.join("src/sub"),"rule":{"pattern":"$E.unwrap()"}});
        if let Some(test_code) = test_code {
            row["testCode"] = json!(test_code);
        }
        let first = run_row(&row, &paths, &security);
        let snapshot = first["snapshot"].as_str().expect("snapshot");
        if cfg!(unix) {
            assert!(SCANS.holds(snapshot), "{test_code:?}");
        }
        std::fs::write(&parent, GATED).expect("gate");
        let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
        let second = run_row(&next, &paths, &security);
        assert!(second.get("errorCode").is_none(), "{test_code:?}: {second}");
        if cfg!(unix) {
            assert!(SCANS.holds(snapshot), "{test_code:?}");
        }
    }
}
