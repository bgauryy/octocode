use super::*;
use crate::tools::test_support::{Fixture, simple_policy};
use serde_json::json;

#[test]
fn settled_match_memo_observes_empty_directories_and_ignore_edits() {
    for ignore_name in [None, Some(".ignore"), Some(".gitignore")] {
        let root = Fixture::new();
        let nested = root.0.join("nested");
        std::fs::create_dir(&nested).expect("nested directory");
        std::fs::write(root.0.join("a.ts"), "target(\"one\");\ntarget(\"two\");\n")
            .expect("source");
        if let Some(name) = ignore_name {
            if name == ".gitignore" {
                std::fs::create_dir(root.0.join(".git")).expect("git marker");
            }
            std::fs::write(root.0.join(name), "nested/\n").expect("ignore rules");
            std::fs::write(nested.join("b.ts"), "target(\"new\");\n").expect("ignored source");
        }
        let (paths, security) = simple_policy(&root.0);
        octocode_engine::stamp::settle_written_so_far();
        let first = run_row(
            &json!({"operation":"match","path":root.0,"rule":{"pattern":"target($A)"},"includeCaptures":true}),
            &paths,
            &security,
        );
        assert_eq!(first["matchCount"], 2, "ignore={ignore_name:?}: {first}");
        let snapshot = first["snapshot"].as_str().expect("snapshot");
        // A repository scan is stored like any other: its rules are watched.
        assert!(SCANS.holds(snapshot), "stored: {ignore_name:?}");
        let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
        let unchanged = run_row(&next, &paths, &security);
        assert_eq!(
            unchanged["files"][0]["matches"][0]["value"],
            "target(\"two\")"
        );
        assert!(SCANS.holds(snapshot), "an unchanged page reuses it");
        if let Some(name) = ignore_name {
            std::fs::write(root.0.join(name), "").expect("unignore existing source");
        } else {
            std::fs::write(nested.join("b.ts"), "target(\"new\");\n").expect("new nested source");
        }
        let changed = run_row(&next, &paths, &security);
        assert_eq!(
            changed["errorCode"], "staleSnapshot",
            "ignore={ignore_name:?}: {changed}"
        );
        assert!(
            !SCANS.holds(snapshot),
            "changed discovery evicts the old scan"
        );
    }
}

/// Swaps a source for a link when the scan reaches one chosen cancellation
/// check: after the walk, between the walk and the read, or after the read.
#[cfg(unix)]
struct SwapAt<'a> {
    at: usize,
    checks: std::sync::atomic::AtomicUsize,
    swap: &'a (dyn Fn() + Sync),
}

#[cfg(unix)]
impl crate::execution::cancel::CancellationCheck for SwapAt<'_> {
    fn check(&self) -> Result<(), String> {
        let check = self
            .checks
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if check == self.at {
            (self.swap)();
        }
        Ok(())
    }
}

/// However late a walked source is swapped for a link, the scan never reports
/// what the link names: every cancellation check of the scan is tried.
#[cfg(unix)]
#[test]
fn a_source_swapped_for_a_link_during_a_scan_is_never_followed() {
    let outside = tempfile::tempdir().expect("outside");
    let target = outside.path().join("target.ts");
    std::fs::write(&target, "target(\"beyond_the_link\");\n").expect("target");
    let mut at = 0;
    loop {
        let root = Fixture::new();
        let source = root.0.join("a.ts");
        std::fs::write(&source, "target(\"inside\");\n").expect("source");
        let swap = || {
            std::fs::remove_file(&source).expect("remove");
            std::os::unix::fs::symlink(&target, &source).expect("link");
        };
        let cancel = SwapAt {
            at,
            checks: std::sync::atomic::AtomicUsize::new(0),
            swap: &swap,
        };
        let (paths, security) = simple_policy(&root.0);
        let query = serde_json::from_value(
            json!({"operation":"match","path":root.0,"rule":{"pattern":"target($A)"},"includeCaptures":true}),
        )
        .expect("canonical match");
        let answer = super::super::execute_ast(&query, &paths, &security, &cancel, None);
        let shown = format!("{answer:?}");
        assert!(!shown.contains("beyond_the_link"), "swap at {at}: {shown}");
        if cancel.checks.load(std::sync::atomic::Ordering::SeqCst) <= at {
            break;
        }
        at += 1;
    }
    assert!(at > 1, "the scan checks for cancellation as it goes");
}

fn run_row(
    row: &serde_json::Value,
    paths: &crate::policy::path::PathPolicy,
    security: &crate::security::ContentSecurity,
) -> serde_json::Value {
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
