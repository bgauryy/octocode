//! The store lock's wait, and the shared store's recovery: integrity,
//! a moved home, refs made in other worktrees and a failed publication.
use super::lifecycle::{again, first_row};
use super::withheld::fixture_with_secrets;
use super::*;
use std::sync::atomic::AtomicUsize;

/// Runs `inner`, pausing `delay` before every step labelled `label`.
struct SlowRunner {
    inner: RewriteRunner,
    label: &'static str,
    delay: Duration,
}

impl GitRunner for SlowRunner {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        if request.label == self.label {
            std::thread::sleep(self.delay);
        }
        self.inner.run(request, control)
    }

    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError> {
        self.inner.run_nul_records(request, control, record)
    }
}

/// A row queued behind a slow fetch of the same repository waits for the
/// store, however long the holder takes: only the caller's own deadline or
/// cancellation ends the wait, and a holder that died released the lock.
#[test]
fn a_row_queued_behind_a_slow_fetch_waits_for_the_store() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let delay = Duration::from_millis(900);
    let runner = SlowRunner {
        inner: fixture_runner(&fixture),
        label: "fetch requested commit",
        delay,
    };
    let config = CloneConfig::persistent(&home);
    let barrier = Barrier::new(2);
    let rows = [one::<String>("src"), one::<String>("README.md")];
    let results = std::thread::scope(|threads| {
        let workers = rows
            .iter()
            .map(|paths| {
                let (runner, config, endpoint, policy, barrier, fixture) =
                    (&runner, &config, &endpoint, &policy, &barrier, &fixture);
                threads.spawn(move || {
                    let context = setup(runner, config, endpoint, policy, &NeverCancel, None);
                    let row = GhCloneRepoQuery {
                        paths: paths.iter().map(|path| reference(path)).collect(),
                        ..query()
                    };
                    barrier.wait();
                    clone_pinned(fixture, &row, &context)
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("worker"))
            .collect::<Vec<_>>()
    });
    for result in results {
        let row = result.expect("a queued row's own work is valid");
        assert!(Path::new(&row.location.path).is_dir());
    }
}

/// The caller's own deadline ends the wait for a held lock.
#[test]
fn a_waiting_row_gives_up_at_its_own_deadline() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let first = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let held = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(storage::lock_path(&home, Path::new(&first.location.path)))
        .expect("lock file");
    let octocode_io::lock::TryLock::Locked(_held) =
        octocode_io::lock::try_lock(held).expect("lock")
    else {
        panic!("the lock is free");
    };
    let started = Instant::now();
    let short = CloneContext {
        deadline: started + Duration::from_millis(300),
        ..setup(&runner, &config, &endpoint, &policy, &NeverCancel, None)
    };
    let error = clone_pinned(&fixture, &query(), &short).expect_err("the lock is held");
    assert_eq!(error.code, ErrorCode::Timeout, "{error:?}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the wait ended at the caller's deadline"
    );
}

/// Overwrite every object pack of `store` after its header.
fn corrupt_packs(store: &Path) {
    let packs = fs::read_dir(store.join("objects/pack"))
        .expect("pack dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "pack")
        })
        .collect::<Vec<_>>();
    assert!(!packs.is_empty(), "the store holds a pack");
    for pack in packs {
        let mut bytes = fs::read(&pack).expect("read pack");
        let end = bytes.len().saturating_sub(20);
        for (at, byte) in bytes.iter_mut().enumerate().take(end).skip(12) {
            *byte = u8::try_from(at.wrapping_mul(37) % 251).expect("byte");
        }
        fs::remove_file(&pack).expect("packs are read-only");
        fs::write(&pack, bytes).expect("write pack");
    }
}

/// A store whose objects are damaged fails git's own integrity check: the
/// row is made in a new generation, and the checkout of the damaged one
/// keeps its files.
#[test]
fn a_store_that_fails_its_integrity_check_is_replaced_by_a_new_generation() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let old = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let stores = store_dirs(&home);
    corrupt_packs(&stores[0]);
    let second = fixture.push_second();

    let next = clone_pinned(&fixture, &query(), &context).expect("a row after the damage");
    assert_eq!(next.location.commit_sha, second);
    let stores = store_dirs(&home);
    assert_eq!(stores.len(), 2, "{stores:?}");
    assert!(stores[1].to_string_lossy().ends_with("__g2.git"));
    let old_path = Path::new(&old.location.path);
    assert!(
        old_path.join("README.md").is_file(),
        "the old checkout stays"
    );
    assert!(old_path.join(".git").is_file());
}

/// A git failure the store did not cause (here, a commit the remote does
/// not have) is that failure: no generation is made for it.
#[test]
fn a_failure_that_is_not_the_stores_makes_no_new_generation() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let absent = GhCloneRepoQuery {
        ref_: Some(reference(&"ab".repeat(20))),
        ..query()
    };
    let error = clone_pinned(&fixture, &absent, &context).expect_err("the remote lacks it");
    assert_eq!(error.code, ErrorCode::GitFailed, "{error:?}");
    assert!(store_dirs(&home).len() <= 1, "{:?}", store_dirs(&home));
    clone_pinned(&fixture, &query(), &context).expect("a valid row after it");
    assert_eq!(store_dirs(&home).len(), 1, "{:?}", store_dirs(&home));
}

/// Where `path` (under `from`) lives once the home `from` is `to`.
fn relocated(path: &str, from: &Path, to: &Path) -> PathBuf {
    to.join(Path::new(path).strip_prefix(from).expect("under the home"))
}

/// Worktree administration holds absolute paths, so a moved
/// `OCTOCODE_HOME` leaves every checkout pointing at the old one: the row
/// reconnects the checkout and reuses it.
#[test]
fn a_moved_home_reconnects_its_checkouts() {
    let fixture = Fixture::shared();
    let (root, home, runner, path) = first_row(&fixture);
    let moved = root.0.join("home-moved");
    fs::rename(&home, &moved).expect("move the home");
    let path = relocated(&path.to_string_lossy(), &home, &moved);
    let row = again(&fixture, &root, &moved, &runner).expect("the moved checkout");
    assert!(row.location.reused, "{row:?}");
    assert_eq!(PathBuf::from(&row.location.path), path);
    assert_eq!(
        git_output(&path, &["rev-parse", "HEAD"]).trim(),
        fixture.first_commit
    );
}

/// A row for another checkout of the repository, first after the move,
/// must not strand the checkouts it does not touch: pruning the store's
/// administration would drop them.
#[test]
fn another_row_after_a_move_keeps_the_other_checkouts_registered() {
    let fixture = Fixture::shared();
    let (root, home, runner, path) = first_row(&fixture);
    let moved = root.0.join("home-moved");
    fs::rename(&home, &moved).expect("move the home");
    let path = relocated(&path.to_string_lossy(), &home, &moved);
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let config = CloneConfig::persistent(&moved);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let other = GhCloneRepoQuery {
        paths: one("src"),
        ..query()
    };
    clone_pinned(&fixture, &other, &context).expect("another checkout");
    let row = clone_pinned(&fixture, &query(), &context).expect("the first checkout");
    assert!(row.location.reused, "{row:?}");
    assert_eq!(
        git_output(&path, &["rev-parse", "HEAD"]).trim(),
        fixture.first_commit
    );
}

/// A branch made in one worktree is not another worktree's local work: only
/// refs that contain the worktree's own `HEAD` are.
#[test]
fn a_branch_made_in_another_worktree_does_not_dirty_this_one() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let old = clone_pinned(&fixture, &query(), &context).expect("first commit");
    fixture.push_second();
    let new = clone_pinned(&fixture, &query(), &context).expect("second commit");
    assert_ne!(old.location.commit_sha, new.location.commit_sha);
    git(Path::new(&old.location.path), &["branch", "feature"]);

    let again = clone_pinned(&fixture, &query(), &context).expect("the other worktree");
    assert!(again.location.reused, "{again:?}");
    // The worktree that owns the branch still preserves it.
    let error = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: Some(reference(&fixture.first_commit)),
            ..query()
        },
        &context,
    )
    .expect_err("the branch is this worktree's work");
    assert_eq!(error.code, ErrorCode::CheckoutDirty, "{error:?}");
}

/// Fails the publication's `worktree move` of the stage (the moves of the
/// replaced checkout, aside and back, run).
struct FailPublish {
    inner: RewriteRunner,
    armed: AtomicBool,
    failed: AtomicUsize,
}

impl GitRunner for FailPublish {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        let aside = request
            .args
            .iter()
            .rev()
            .take(2)
            .any(|argument| argument.to_string_lossy().ends_with(".previous"));
        if request.label == "move worktree" && !aside && self.armed.load(Ordering::SeqCst) {
            self.failed.fetch_add(1, Ordering::SeqCst);
            return Err(CloneError::new(
                ErrorCode::GitFailed,
                "injected move failure",
            ));
        }
        self.inner.run(request, control)
    }

    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError> {
        self.inner.run_nul_records(request, control, record)
    }
}

/// A publication that fails after the replaced checkout was moved aside puts
/// it back registered: the next row reuses or replaces it, never meets an
/// unregistered directory (`checkoutDirty`).
#[test]
fn a_failed_publication_restores_the_replaced_worktree_registered() {
    let fixture = fixture_with_secrets();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = FailPublish {
        inner: fixture_runner(&fixture),
        armed: AtomicBool::new(false),
        failed: AtomicUsize::new(0),
    };
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let old = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let old_path = PathBuf::from(&old.location.path);
    // A credential blob in the store makes the checkout not reusable: the
    // next row replaces it from a new generation.
    let secret = root.0.join("secret.txt");
    fs::write(&secret, "TOKEN=hunter2\n").expect("secret");
    git(
        &store_dirs(&home)[0],
        &["hash-object", "-w", secret.to_str().expect("utf8")],
    );

    runner.armed.store(true, Ordering::SeqCst);
    let error = clone_pinned(&fixture, &query(), &context).expect_err("the move fails");
    assert_eq!(error.code, ErrorCode::GitFailed, "{error:?}");
    assert!(runner.failed.load(Ordering::SeqCst) > 0);
    assert_eq!(
        git_output(&old_path, &["rev-parse", "HEAD"]).trim(),
        old.location.commit_sha,
        "git still reads the restored checkout"
    );
    assert!(old_path.join("README.md").is_file());

    runner.armed.store(false, Ordering::SeqCst);
    let next = clone_pinned(&fixture, &query(), &context).expect("the next row replaces it");
    assert!(Path::new(&next.location.path).join("README.md").is_file());
}

/// The replaced worktree and the stage share a store, as when a checkout
/// that is clean but not reusable is replaced in place: publication moves
/// the replaced one aside and, when placing the stage fails, back. The
/// restored checkout is still registered, so git reads it.
#[test]
fn a_failed_placement_restores_a_same_store_worktree_registered() {
    let fixture = Fixture::shared();
    let (root, home, runner, path) = first_row(&fixture);
    let store = store_dirs(&home).remove(0);
    let stage = home.join("tmp/clone-tmp/placement-stage");
    fs::create_dir_all(stage.parent().expect("parent")).expect("clone-tmp");
    git(
        &store,
        &[
            "worktree",
            "add",
            "--no-checkout",
            "--detach",
            stage.to_str().expect("utf8"),
            &fixture.first_commit,
        ],
    );
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let error = storage::promote(
        &stage,
        &path,
        |from, to| git::move_worktree(&context, &store, from, to),
        || {
            Err(CloneError::new(
                ErrorCode::GitFailed,
                "injected move failure",
            ))
        },
    )
    .expect_err("the placement fails");
    assert_eq!(error.code, ErrorCode::GitFailed);
    assert_eq!(
        git_output(&path, &["rev-parse", "HEAD"]).trim(),
        fixture.first_commit,
        "git still reads the restored checkout"
    );
    assert!(path.join("README.md").is_file());
}
