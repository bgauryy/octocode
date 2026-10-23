use super::super::test_rows::run_row;
use super::*;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::Value;

fn query(path: &std::path::Path, rule: bool) -> Value {
    let mut query =
        json!({"operation":"match", "path":path, "captureText":true, "matchPageSize":1});
    if rule {
        query["rule"] = json!({"kind":"call_expression"});
    } else {
        query["pattern"] = json!("target($A)");
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
                let next = first["next"]["nextMatchPage"]["query"]["queries"][0].clone();
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
            let next = first["next"]["nextMatchPage"]["query"]["queries"][0].clone();
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
