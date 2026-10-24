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
        let next = first["next"]["nextMatchPage"]["input"]["queries"][0].clone();
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
