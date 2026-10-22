use super::{
    EntryKind, Snapshot, TreeEntry,
    snapshot::{FileIdentity, Store, write_file},
};
use octocode_github::{ProviderErrorKind, RequestContext};
use octocode_io::budget::RequestBudget;
use std::{path::Path, time::Duration};

fn snapshot(home: &Path) -> Snapshot<'_> {
    Snapshot {
        root: home,
        owner: "octo",
        repo: "demo",
        commit: "abc123",
    }
}

fn context() -> RequestContext {
    RequestContext::new(
        RequestBudget::with_timeout(Duration::from_secs(5), 1024),
        None,
    )
}

fn entry(path: &str, bytes: &[u8]) -> TreeEntry {
    TreeEntry {
        path: path.into(),
        kind: EntryKind::File,
        size: Some(bytes.len() as u64),
    }
}

#[tokio::test]
async fn reuse_checks_content_identity_and_never_trusts_same_size_edits() {
    let home = tempfile::tempdir().unwrap();
    let mut store = Store::open(&snapshot(home.path()), &context())
        .await
        .unwrap();
    let path = "src/file.ts";
    let original = b"original";
    write_file(home.path(), &store.root, path, original)
        .await
        .unwrap();
    store
        .record(vec![(path.into(), FileIdentity::of(original))])
        .await
        .unwrap();
    let entry = entry(path, original);
    assert!(store.reusable(&entry).await);
    std::fs::write(store.root.join(path), b"modified").unwrap();
    assert!(!store.reusable(&entry).await);
    write_file(home.path(), &store.root, path, original)
        .await
        .unwrap();
    assert!(store.reusable(&entry).await);
}

#[cfg(unix)]
#[tokio::test]
async fn leaf_replacement_and_ancestor_rejection_keep_outside_bytes_unchanged() {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(&snapshot(home.path()), &context())
        .await
        .unwrap();
    let outside = home.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("file.ts"), b"outside").unwrap();
    let leaf = store.root.join("file.ts");
    std::os::unix::fs::symlink(outside.join("file.ts"), &leaf).unwrap();
    write_file(home.path(), &store.root, "file.ts", b"inside")
        .await
        .unwrap();
    assert!(
        !std::fs::symlink_metadata(&leaf)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(std::fs::read(leaf).unwrap(), b"inside");
    std::os::unix::fs::symlink(&outside, store.root.join("src")).unwrap();
    let error = write_file(home.path(), &store.root, "src/file.ts", b"escape")
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Configuration);
    assert_eq!(std::fs::read(outside.join("file.ts")).unwrap(), b"outside");
    let original_root = store.root.with_extension("moved");
    std::fs::rename(&store.root, &original_root).unwrap();
    std::os::unix::fs::symlink(&outside, &store.root).unwrap();
    assert!(
        write_file(home.path(), &store.root, "file.ts", b"escape")
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(outside.join("file.ts")).unwrap(), b"outside");
}

#[tokio::test]
async fn concurrent_snapshot_writers_keep_every_recorded_file() {
    let home = tempfile::tempdir().unwrap();
    let write = |name: &'static str| {
        let home = home.path();
        async move {
            let mut store = Store::open(&snapshot(home), &context()).await.unwrap();
            write_file(home, &store.root, name, name.as_bytes())
                .await
                .unwrap();
            store
                .record(vec![(name.into(), FileIdentity::of(name.as_bytes()))])
                .await
                .unwrap();
        }
    };
    tokio::join!(write("first.ts"), write("second.ts"), write("third.ts"));
    let store = Store::open(&snapshot(home.path()), &context())
        .await
        .unwrap();
    for name in ["first.ts", "second.ts", "third.ts"] {
        assert!(store.reusable(&entry(name, name.as_bytes())).await);
    }
}

#[tokio::test]
async fn snapshot_lock_wait_obeys_deadline_and_cancellation() {
    let home = tempfile::tempdir().unwrap();
    let _held = Store::open(&snapshot(home.path()), &context())
        .await
        .unwrap();
    let short = RequestContext::new(
        RequestBudget::with_timeout(Duration::from_millis(20), 1024),
        None,
    );
    let error = Store::open(&snapshot(home.path()), &short)
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, ProviderErrorKind::Timeout);
    let cancelled = context();
    cancelled.budget.cancellation.cancel();
    let error = Store::open(&snapshot(home.path()), &cancelled)
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, ProviderErrorKind::Cancelled);
}

#[tokio::test]
async fn a_storage_failure_is_not_invalid_input_and_old_snapshots_are_untouched() {
    let home = tempfile::tempdir().unwrap();
    let old = home.path().join("tmp/materialize/v2");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("keep"), b"old snapshot").unwrap();
    let store = Store::open(&snapshot(home.path()), &context())
        .await
        .unwrap();
    assert_eq!(std::fs::read(old.join("keep")).unwrap(), b"old snapshot");
    std::fs::write(store.root.join("file-parent"), b"not a directory").unwrap();
    let error = write_file(home.path(), &store.root, "file-parent/file.ts", b"new")
        .await
        .unwrap_err();
    assert_eq!(error.kind, ProviderErrorKind::Configuration);
    assert!(!error.retryable);
}

#[tokio::test]
async fn an_unrecorded_nonempty_snapshot_is_refused_without_deleting_its_files() {
    let home = tempfile::tempdir().unwrap();
    let root = super::materialize_root(home.path()).join("octo/demo/abc123");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("foreign.ts"), b"unrecorded bytes").unwrap();
    let error = Store::open(&snapshot(home.path()), &context())
        .await
        .err()
        .unwrap();
    assert_eq!(error.kind, ProviderErrorKind::Configuration);
    assert_eq!(
        std::fs::read(root.join("foreign.ts")).unwrap(),
        b"unrecorded bytes"
    );
}
