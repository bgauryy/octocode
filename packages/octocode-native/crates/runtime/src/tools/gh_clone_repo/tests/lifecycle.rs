//! Checkout locks, rollback and independent output lifetimes.
use super::*;

#[test]
fn stale_lock_and_failed_publication_preserve_existing_output() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let first = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let clone_path = PathBuf::from(&first.location.path);
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
    let failed_placement = || Err(GitError::new(ErrorCode::GitFailed, "placement failed"));
    assert!(
        storage::promote(
            &missing_stage,
            &destination,
            storage::rename_dir,
            failed_placement
        )
        .is_err()
    );
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
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let main = clone_pinned(&fixture, &query(), &context).expect("main clone");
    let main_path = PathBuf::from(&main.location.path);
    // The tag names main's commit: a deeper history is another entry.
    let deep_tag = GhCloneRepoQuery {
        ref_: Some(reference("v1")),
        history_depth: std::num::NonZeroU64::new(2).expect("depth"),
        ..query()
    };
    let tag = clone_pinned(&fixture, &deep_tag, &context).expect("tag clone");
    assert!(main_path.exists(), "clone outputs are never cache-evicted");
    assert!(Path::new(&tag.location.path).is_dir());

    let live_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(storage::lock_path(&home, Path::new(&tag.location.path)))
        .expect("live lock file");
    let held = match octocode_io::lock::try_lock(live_lock).expect("lock") {
        octocode_io::lock::TryLock::Locked(held) => held,
        octocode_io::lock::TryLock::Busy(_) => panic!("the live lock is free"),
    };
    // A live holder is waited for until the caller's own deadline.
    let short = GitContext {
        deadline: Instant::now() + Duration::from_millis(300),
        ..setup(&runner, &config, &endpoint, &policy, &NeverCancel, None)
    };
    let error = clone_pinned(&fixture, &deep_tag, &short).expect_err("live lock must time out");
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
    assert_eq!(missing.code, ErrorCode::NotFound);
    assert!(missing.message.contains("\"missing/path\""), "{missing:?}");
    // The lead, not a hint or the message, names the recovery: nothing near
    // `path` exists at the root, so it lists the root at the commit.
    assert!(missing.hints.is_empty(), "{missing:?}");
    test_support::checks::assert_absent(&serde_json::json!(missing.message), "ghStructure");
    let next = missing.calls.as_deref().expect("recovery lead");
    assert_eq!(next["viewTree"]["tool"], "ghStructure", "{next}");
    assert!(Path::new(&tag.location.path).is_dir());
}

/// What the lifecycle rows below share: a runner, a home and a first row.
pub(super) fn first_row(
    fixture: &Fixture,
) -> (test_support::Fixture, PathBuf, RewriteRunner, PathBuf) {
    let (root, home) = home_root();
    let runner = fixture_runner(fixture);
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let first = clone_pinned(fixture, &query(), &context).expect("first clone");
    let path = PathBuf::from(first.location.path);
    (root, home, runner, path)
}

pub(super) fn again(
    fixture: &Fixture,
    root: &test_support::Fixture,
    home: &Path,
    runner: &RewriteRunner,
) -> Result<CloneResult, GitError> {
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let config = GitConfig::persistent(home);
    let context = setup(runner, &config, &endpoint, &policy, &NeverCancel, None);
    clone_pinned(fixture, &query(), &context)
}

/// A worktree directory the user deleted is made again: its stale
/// administration in the store is pruned first.
#[test]
fn a_deleted_worktree_is_made_again() {
    let fixture = Fixture::shared();
    let (root, home, runner, path) = first_row(&fixture);
    fs::remove_dir_all(&path).expect("delete worktree");
    let row = again(&fixture, &root, &home, &runner).expect("recreated");
    assert!(!row.location.reused);
    assert!(path.join(".git").is_file() && path.join("README.md").is_file());
}

/// A worktree whose store was deleted cannot be proven clean by git: it is
/// preserved, with its files, never replaced.
#[test]
fn a_worktree_whose_store_is_gone_is_preserved() {
    let fixture = Fixture::shared();
    let (root, home, runner, path) = first_row(&fixture);
    fs::remove_dir_all(crate::tools::gh_git::store_root(&home)).expect("delete store");
    let error = again(&fixture, &root, &home, &runner).expect_err("store gone");
    assert_eq!(error.code, ErrorCode::CheckoutDirty);
    assert!(path.join("README.md").is_file(), "the worktree is intact");
}

/// A store whose init never finished is finished by the next row.
#[test]
fn an_unfinished_store_is_rebuilt() {
    let fixture = Fixture::shared();
    let (root, home, runner, path) = first_row(&fixture);
    let stores = store_dirs(&home);
    assert_eq!(stores.len(), 1);
    fs::remove_dir_all(&path).expect("delete worktree");
    fs::remove_file(stores[0].join("octocode-store-ready")).expect("unmark");
    again(&fixture, &root, &home, &runner).expect("rebuilt");
    assert!(stores[0].join("octocode-store-ready").is_file());
    assert_eq!(store_dirs(&home), stores, "same generation");
}

/// A row that crashed mid-checkout leaves its stage worktree registered in the
/// store: the next row under the store lock removes the stage whose owning
/// process is gone, so no generation stays in use by a dead row.
#[test]
fn a_stage_left_by_a_crashed_process_is_removed() {
    let fixture = Fixture::shared();
    let (root, home, runner, first) = first_row(&fixture);
    let commit = git_output(&first, &["rev-parse", "HEAD"]);
    let stores = store_dirs(&home);
    let gone = std::process::Command::new("true")
        .spawn()
        .and_then(|mut child| child.wait().map(|_| child.id()))
        .expect("a finished process");
    let stage = home.join(format!("tmp/clone-tmp/0123456789abcdef-crashed-{gone}-0"));
    fs::create_dir_all(stage.parent().expect("parent")).expect("clone-tmp");
    git(
        &stores[0],
        &[
            "worktree",
            "add",
            "--detach",
            stage.to_str().expect("utf8"),
            commit.trim(),
        ],
    );
    assert!(stage.join(".git").is_file());

    let other = GhCloneRepoQuery {
        paths: one("src"),
        ..query()
    };
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    clone_pinned(&fixture, &other, &context).expect("next row");
    assert!(!stage.exists(), "the dead process's stage is removed");
    let registered = fs::read_dir(stores[0].join("worktrees"))
        .expect("worktrees")
        .count();
    assert_eq!(registered, 2, "the two published checkouts only");
}
