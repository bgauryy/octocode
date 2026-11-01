//! Checkout locks, rollback and independent output lifetimes.
use super::*;

#[test]
fn stale_lock_and_failed_publication_preserve_existing_output() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&home);
    config.lock_wait = Duration::from_secs(1);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let first = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let clone_path = PathBuf::from(&first.location.local_path);
    // A holder that ended without unlocking (a crash closes its descriptor)
    // leaves only the lock file: the kernel released the lock, so the next
    // clone takes it at once, with no age or pid heuristics.
    let lock = storage::lock_path(&home, &clone_path);
    let crashed = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock)
        .expect("lock file");
    crashed.try_lock().expect("crashed holder takes the lock");
    drop(crashed);
    assert!(lock.is_file(), "the lock file stays behind");
    clone_pinned(&fixture, &query(), &context).expect("a crashed holder's lock does not block");

    let destination = root.0.join("publication");
    fs::create_dir_all(&destination).expect("old destination");
    fs::write(destination.join("old"), "preserved").expect("old content");
    let missing_stage = root.0.join("missing-stage");
    assert!(storage::promote(&missing_stage, &destination).is_err());
    assert_eq!(
        fs::read_to_string(destination.join("old")).expect("restored"),
        "preserved"
    );
}

#[test]
fn live_lock_and_failed_sparse_clone_preserve_other_outputs() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&home);
    config.lock_wait = Duration::from_millis(120);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let main = clone_pinned(&fixture, &query(), &context).expect("main clone");
    let main_path = PathBuf::from(&main.location.local_path);
    // The tag names main's commit: a deeper history is another entry.
    let deep_tag = GhCloneRepoQuery {
        ref_: Some(reference("v1")),
        history_depth: std::num::NonZeroU64::new(2).expect("depth"),
        ..query()
    };
    let tag = clone_pinned(&fixture, &deep_tag, &context).expect("tag clone");
    assert!(main_path.exists(), "clone outputs are never cache-evicted");
    assert!(Path::new(&tag.location.local_path).is_dir());

    let live_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(storage::lock_path(
            &home,
            Path::new(&tag.location.local_path),
        ))
        .expect("live lock file");
    let held = match octocode_io::lock::try_lock(live_lock).expect("lock") {
        octocode_io::lock::TryLock::Locked(held) => held,
        octocode_io::lock::TryLock::Busy(_) => panic!("the live lock is free"),
    };
    let error = clone_pinned(&fixture, &deep_tag, &context).expect_err("live lock must time out");
    assert_eq!(error.code, ErrorCode::Timeout);
    drop(held);

    let missing = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("missing/path"),
            ..query()
        },
        &context,
    )
    .expect_err("missing sparse path");
    assert_eq!(missing.code, ErrorCode::PathNotFound);
    assert!(missing.message.contains("\"missing/path\""), "{missing:?}");
    // The lead, not a hint or the message, names the recovery: nothing near
    // `path` exists at the root, so it lists the root at the commit.
    assert!(missing.hints.is_empty(), "{missing:?}");
    test_support::checks::assert_absent(&serde_json::json!(missing.message), "ghStructure");
    let next = missing.calls.as_deref().expect("recovery lead");
    assert_eq!(next["viewTree"]["tool"], "ghStructure", "{next}");
    assert!(Path::new(&tag.location.local_path).is_dir());
}
