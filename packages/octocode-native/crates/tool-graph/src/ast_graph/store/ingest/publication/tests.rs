use super::*;
use octocode_test_support::json_fixture;
#[test]
fn cancelled_staging_is_removed_and_previous_latest_is_preserved() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(LATEST_FILE), b"previous").unwrap();
    let staged = Staged::new(dir.path()).unwrap();
    let path = staged.path.clone();
    std::fs::write(staged.graph(), b"partial").unwrap();
    drop(staged);
    assert!(!path.exists());
    assert_eq!(
        std::fs::read(dir.path().join(LATEST_FILE)).unwrap(),
        b"previous"
    );
}
#[test]
fn failed_publication_preserves_previous_latest() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(LATEST_FILE), b"previous").unwrap();
    let staged = Staged::new(dir.path()).unwrap();
    assert!(staged.publish(dir.path(), "next", &json!({})).is_err());
    assert_eq!(
        std::fs::read(dir.path().join(LATEST_FILE)).unwrap(),
        b"previous"
    );
}

#[test]
fn abandoned_stage_cleanup_preserves_unmarked_directories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let staged = std::mem::ManuallyDrop::new(Staged::new(&root).unwrap());
    std::fs::write(staged.graph(), b"partial").unwrap();
    let unrelated = root.join(".tmp-0123456789abcdef0123456789abcdef.graph");
    std::fs::create_dir(&unrelated).unwrap();
    cleanup(
        &root,
        &octocode_runtime_base::test_support::workspace_policy(&root),
        &octocode_runtime_base::execution::cancel::NeverCancel,
    )
    .unwrap();
    assert!(!staged.path.exists());
    assert!(unrelated.is_dir());
}

#[test]
fn failed_latest_update_reports_the_already_committed_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(LATEST_FILE)).unwrap();
    let staged = Staged::new(dir.path()).unwrap();
    std::fs::write(staged.graph(), b"complete graph").unwrap();
    let error = staged
        .publish(
            dir.path(),
            "committed",
            &json_fixture(r#"{"complete": true}"#),
        )
        .unwrap_err();
    assert!(error.to_string().contains("is committed"));
    assert_eq!(
        std::fs::read(dir.path().join("committed").join(GRAPH_FILE)).unwrap(),
        b"complete graph"
    );
    assert!(dir.path().join("committed").join(MANIFEST_FILE).is_file());
}
