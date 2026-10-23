//! File caps, truncation, policy-denied files and read limits.

use super::*;

#[test]
fn reports_truncation_only_when_graph_scan_exceeds_the_file_cap() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    create_supported_files(&root, 3);

    let below_cap = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(4),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan below cap");
    assert_eq!(below_cap.entries.len(), 3);
    assert!(!below_cap.truncated);

    let exact_cap = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(3),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan at cap");
    assert_eq!(exact_cap.entries.len(), 3);
    assert!(!exact_cap.truncated);

    let over_cap = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(2),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan over cap");
    assert_eq!(over_cap.entries.len(), 2);
    assert!(over_cap.truncated);
}

#[test]
fn detects_graph_scan_overflow_across_directory_boundaries() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    // Each child ends exactly at the initial cap, whichever is visited first.
    create_supported_files(&root.join("left"), 1);
    create_supported_files(&root.join("right"), 1);

    let capped = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(1),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan nested graph at cap");
    assert_eq!(capped.entries.len(), 1);
    assert!(
        capped.truncated,
        "the sibling subtree still has a graph file"
    );

    let expanded = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(2),
        skip_files: None,
        ..Default::default()
    })
    .expect("expand nested graph scan");
    assert_eq!(
        scanned_paths(&expanded),
        ["left/entry-0.ts", "right/entry-0.ts"]
    );
    assert!(
        !expanded.truncated,
        "an exactly full complete scan has no overflow"
    );
}

#[test]
fn ignored_files_do_not_make_an_exact_graph_cap_partial() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    create_supported_files(&root, 1);
    fs::write(root.join("ignored.txt"), "not graph input").expect("write ignored file");
    fs::create_dir_all(root.join("ignored-directory")).expect("create ignored subtree");
    fs::write(
        root.join("ignored-directory/ignored.txt"),
        "not graph input",
    )
    .expect("write ignored child");

    let result = scan_graph_facts(GraphFactsScanOptions {
        path: path_string(&root),
        max_files: Some(1),
        skip_files: None,
        ..Default::default()
    })
    .expect("scan exactly one matching graph file");
    assert_eq!(scanned_paths(&result), ["entry-0.ts"]);
    assert!(
        !result.truncated,
        "nonmatching entries are not graph overflow"
    );
}

#[test]
fn policy_denied_graph_files_do_not_consume_the_scan_budget() {
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(root.join("private")).expect("fixture");
    fs::write(root.join("private/hidden.rs"), "pub fn hidden() {}").expect("hidden");
    fs::write(root.join("visible.rs"), "pub fn visible() {}").expect("visible");
    let result = scan_admitted(
        GraphFactsScanOptions {
            path: path_string(&root),
            max_files: Some(1),
            skip_files: None,
            ..Default::default()
        },
        &|path, _| !path.starts_with(root.join("private")),
    )
    .expect("filtered scan");
    assert_eq!(scanned_paths(&result), ["visible.rs"]);
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.files_skipped, 0);
    assert!(!result.truncated);
}

#[test]
fn graph_read_enforces_limit_when_file_grows_after_discovery() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let fixture = tempfile::tempdir().expect("fixture");
    let root = fixture.path().to_path_buf();
    fs::create_dir_all(&root).expect("fixture");
    let file = root.join("growing.rs");
    fs::write(&file, "pub fn initial() {}").expect("initial");
    // Walked after `growing.rs` (name order): it grows once listed.
    let later = root.join("z.rs");
    fs::write(&later, "pub fn later() {}").expect("later");
    let visits = AtomicUsize::new(0);
    let result = scan_admitted(
        GraphFactsScanOptions {
            path: path_string(&root),
            max_file_bytes: Some(32),
            ..Default::default()
        },
        &|path, _| {
            if path == later && visits.fetch_add(1, Ordering::SeqCst) == 0 {
                fs::write(&file, "x".repeat(65_536)).expect("concurrent growth");
            }
            true
        },
    )
    .expect("bounded scan");
    assert_eq!(scanned_paths(&result), ["growing.rs", "z.rs"]);
    assert_eq!(result.skipped[0].relative_path, "growing.rs");
    assert_eq!(result.skipped[0].code, "graph.scan.fileTooLarge");
}
