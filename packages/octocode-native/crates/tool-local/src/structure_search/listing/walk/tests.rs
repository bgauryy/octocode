use super::super::super::probe::{Uncovered, note_empty};
use super::*;
use octocode_contract_types::StructureSearchQueryFiles;
use octocode_engine::types::FileSystemEntry;
use octocode_test_support::json_fixture;
use serde_json::json;

/// An empty listing reads nothing it skipped: it says the skipped paths were
/// not listed, so absence there is unproven, and leads to the listing with
/// the causes its walk skipped by lifted. The requested walk was whole, so
/// the listing is not partial.
#[test]
fn an_empty_listing_leads_to_its_skipped_paths_without_walking_them() {
    let root = tempfile::tempdir().expect("root");
    let q: StructureSearchQueryFiles = serde_json::from_value(json!({
        "operation": "files", "path": root.path(), "extensions": ["ts"]
    }))
    .expect("files query");
    let seen = Uncovered {
        pruned: vec!["build".to_owned()],
        ..Default::default()
    };
    let mut out = json_fixture(r#"{"status": "empty"}"#);
    note_empty(&mut out, &q, &seen, q.walk());
    assert_eq!(
        out["warnings"],
        json!([format!(
            "Skipped paths (build/) were not searched, so absence there is unproven. {}",
            octocode_tools_shared::leads::skipped::walks_all_skipped(&[
                octocode_tools_shared::empty_walk::Cause::DefaultExcluded
            ])
        )]),
        "{out}"
    );
    assert!(out.get("partialReasons").is_none(), "{out}");
    let retry = &out["calls"]["includeSkipped"]["input"]["queries"][0];
    assert_eq!(retry["defaultExcludes"], false, "{out}");
    assert!(retry.get("page").is_none(), "a fresh first page: {out}");
    assert!(retry.get("includeIgnored").is_none(), "{out}");
    assert!(retry.get("snapshot").is_none(), "{out}");
}

/// An ignored directory can hide a default-pruned one the walk never saw
/// as pruned (a `.gitignore`d `target/`): the lead over ignored paths lifts
/// the default prune too, so its first hop reaches them.
#[test]
fn a_lead_over_ignored_paths_lifts_the_default_prune() {
    let root = tempfile::tempdir().expect("root");
    let q: StructureSearchQueryFiles = serde_json::from_value(json!({
        "operation": "files", "path": root.path(), "extensions": ["rlib"]
    }))
    .expect("files query");
    let seen = Uncovered {
        ignored: vec!["target/".to_owned()],
        ..Default::default()
    };
    let mut out = json_fixture(r#"{"status": "empty"}"#);
    note_empty(&mut out, &q, &seen, q.walk());
    let hint = out["warnings"][0].as_str().unwrap_or_default();
    assert!(
        hint.starts_with("Skipped paths (ignored; default-excluded)"),
        "{out}"
    );
    let retry = &out["calls"]["includeSkipped"]["input"]["queries"][0];
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
        stamp: None,
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
