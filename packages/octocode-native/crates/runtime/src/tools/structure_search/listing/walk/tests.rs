use super::super::super::probe::{PROBE_HITS, ProbeBound, Uncovered, probe_listing};
use super::super::super::{WALK_BUDGET, Walking};
use super::*;
use octocode_contract_types::StructureSearchQueryFiles;
use octocode_engine::types::FileSystemEntry;
use serde_json::json;

/// A probe walk past its deadline admits nothing and says it was cut.
#[test]
fn a_probe_walk_past_its_deadline_admits_nothing() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(root.path().join("build")).expect("build");
    std::fs::write(root.path().join("build/a.ts"), "x\n").expect("ts");
    let paths = crate::tools::test_support::workspace_policy(root.path());
    let q: StructureSearchQueryFiles = serde_json::from_value(json!({
        "operation": "files", "path": root.path(), "extensions": ["ts"], "defaultExcludes": false
    }))
    .expect("files query");
    let bound = ProbeBound::new(std::time::Duration::ZERO);
    let walk = walk_listing(
        &q,
        root.path(),
        &paths,
        &ContentSecurity::new(),
        &crate::execution::cancel::NeverCancel,
        PROBE_HITS,
        Walking::Probe(&bound),
    )
    .expect("walk");
    assert!(walk.rows.is_empty());
    assert!(bound.cut());
    // Unbounded, the same walk lists the file.
    let walk = walk_listing(
        &q,
        root.path(),
        &paths,
        &ContentSecurity::new(),
        &crate::execution::cancel::NeverCancel,
        PROBE_HITS,
        Walking::Listing(WALK_BUDGET),
    )
    .expect("walk");
    assert_eq!(walk.rows.len(), 1);
}

/// An empty listing whose probe ran out of time before it found
/// anything proved nothing about what its walk skipped: it still leads
/// to the listing over the skipped paths and says absence there is
/// unproven.
#[test]
fn an_empty_listing_whose_probe_timed_out_still_leads_to_skipped_paths() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(root.path().join("build")).expect("build");
    std::fs::write(root.path().join("build/a.ts"), "x\n").expect("ts");
    let paths = crate::tools::test_support::workspace_policy(root.path());
    let q: StructureSearchQueryFiles = serde_json::from_value(json!({
        "operation": "files", "path": root.path(), "extensions": ["ts"]
    }))
    .expect("files query");
    let seen = Uncovered {
        pruned: vec!["build".to_owned()],
        ..Default::default()
    };
    let mut out = json!({"status": "empty"});
    crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
        probe_listing(
            &mut out,
            &q,
            (root.path(), &paths),
            &seen,
            |relaxed, entries, walking| {
                let walk = walk_listing(
                    relaxed,
                    root.path(),
                    &paths,
                    &ContentSecurity::new(),
                    &crate::execution::cancel::NeverCancel,
                    entries,
                    walking,
                )?;
                assert!(walk.rows.is_empty(), "the zero budget cut the probe");
                Ok(walk)
            },
        )
    })
    .expect("probe");
    // The shared disclosure of a cut check: a warning, and the listing is
    // partial.
    assert_eq!(
        out["warnings"],
        json!(["Skipped paths (build/) were not checked in time, so absence there is unproven."]),
        "{out}"
    );
    assert_eq!(
        out["partialReasons"],
        json!(["skippedPathsUnchecked"]),
        "{out}"
    );
    let retry = &out["next"]["includeSkipped"]["query"]["queries"][0];
    assert_eq!(retry["defaultExcludes"], false, "{out}");
    assert!(retry.get("page").is_none(), "a fresh first page: {out}");
    assert!(retry.get("includeIgnored").is_none(), "{out}");
    assert!(retry.get("snapshot").is_none(), "{out}");
}

/// An ignored directory can hide a default-pruned one the walk never saw
/// as pruned (a `.gitignore`d `target/`): an unproven lead over ignored
/// paths lifts the default prune too, so its first hop reaches them.
#[test]
fn an_unproven_lead_over_ignored_paths_lifts_the_default_prune() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(root.path().join("target/debug")).expect("target");
    std::fs::write(root.path().join("target/debug/a.rlib"), "x\n").expect("rlib");
    let paths = crate::tools::test_support::workspace_policy(root.path());
    let q: StructureSearchQueryFiles = serde_json::from_value(json!({
        "operation": "files", "path": root.path(), "extensions": ["rlib"]
    }))
    .expect("files query");
    let seen = Uncovered {
        ignored: vec!["target/".to_owned()],
        ..Default::default()
    };
    let mut out = json!({"status": "empty"});
    crate::tools::empty_walk::with_probe_budget(std::time::Duration::ZERO, || {
        probe_listing(
            &mut out,
            &q,
            (root.path(), &paths),
            &seen,
            |relaxed, entries, walking| {
                walk_listing(
                    relaxed,
                    root.path(),
                    &paths,
                    &ContentSecurity::new(),
                    &crate::execution::cancel::NeverCancel,
                    entries,
                    walking,
                )
            },
        )
    })
    .expect("probe");
    let hint = out["warnings"][0].as_str().unwrap_or_default();
    assert!(
        hint.starts_with("Skipped paths (ignored; default-excluded)"),
        "{out}"
    );
    let retry = &out["next"]["includeSkipped"]["query"]["queries"][0];
    assert_eq!(retry["includeIgnored"], true, "{out}");
    assert_eq!(retry["defaultExcludes"], false, "{out}");
    assert!(retry.get("includeHidden").is_none(), "{out}");
}

/// A count cut ends the walk before the first uncounted entry, as the
/// walk's own time budget does: the window keeps the entries before it,
/// names it as the time cut, and resumes the walk right there.
#[test]
fn a_count_cut_ends_the_walk_before_the_first_uncounted_entry() {
    let entry = |name: &str, position: u32| FileSystemEntry {
        path: name.to_owned(),
        relative_path: name.to_owned(),
        name: name.to_owned(),
        entry_type: FileKind::File,
        size: Some(1),
        modified_time: None,
        walk_position: position,
    };
    let mut native = octocode_engine::types::FileSystemQueryResult {
        entries: vec![entry("a.rs", 3), entry("b.rs", 5), entry("c.rs", 9)],
        total_discovered: 3,
        was_capped: true,
        skipped: 0,
        permission_denied: 0,
        ignored: Vec::new(),
        pruned_dirs: Vec::new(),
        time_cut: None,
        walk_position: 9,
        ..Default::default()
    };
    cut_walk_before(&mut native, 1);
    assert_eq!(native.entries.len(), 1);
    assert_eq!(native.time_cut.as_deref(), Some("b.rs"));
    assert_eq!(native.walk_position, 4, "the resume inspects b.rs first");
    assert!(!native.was_capped);
    assert_eq!(native.total_discovered, 1);
}
