//! A verified checkout at the requested commit is reused without a fetch.
use super::*;

/// How many `fetch requested commit` steps the runner has run.
fn fetches(runner: &RewriteRunner) -> usize {
    runner
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|call| call.contains("fetch requested commit"))
        .count()
}

#[test]
fn same_commit_reuses_the_verified_checkout_and_other_states_fetch_fresh() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let first = clone_pinned(&fixture, &query(), &context).expect("first clone");
    assert_eq!(fetches(&runner), 1);

    // Same commit: no git network step, the same facts.
    let second = clone_pinned(&fixture, &query(), &context).expect("reused clone");
    assert_eq!(fetches(&runner), 1, "same SHA must not fetch again");
    assert_eq!(second.location.path, first.location.path);
    assert_eq!(second.location.commit_sha, fixture.first_commit);
    assert_eq!(second.bytes, first.bytes);
    assert_eq!(second.calls, first.calls);

    // The same commit by SHA instead of branch name shares the checkout.
    let by_sha = GhCloneRepoQuery {
        ref_: Some(reference(&fixture.first_commit)),
        ..query()
    };
    clone_pinned(&fixture, &by_sha, &context).expect("reuse by sha");
    assert_eq!(fetches(&runner), 1);

    // A different commit is another destination: fetch.
    let next = fixture.push_second();
    let moved = clone_pinned(&fixture, &query(), &context).expect("moved branch");
    assert_eq!(moved.location.commit_sha, next);
    assert_eq!(fetches(&runner), 2, "a different SHA fetches");

    // A modified file is never reused nor discarded.
    let readme = Path::new(&first.location.path).join("README.md");
    fs::write(&readme, "tampered\n").expect("tamper");
    let error = clone_pinned(&fixture, &by_sha, &context).expect_err("dirty checkout");
    assert_eq!(error.code, ErrorCode::CheckoutDirty);
    assert_eq!(
        fetches(&runner),
        2,
        "a modified checkout is neither reused nor fetched over"
    );
    assert_eq!(fs::read_to_string(&readme).expect("read"), "tampered\n");
    fs::write(&readme, "fixture\n").expect("restore");
    clone_pinned(&fixture, &by_sha, &context).expect("reuse after restore");
    assert_eq!(fetches(&runner), 2, "a restored checkout verifies clean");

    // An untracked file is a local change too.
    fs::write(readme.with_file_name("extra.txt"), "x").expect("untracked");
    let error = clone_pinned(&fixture, &by_sha, &context).expect_err("untracked file");
    assert_eq!(error.code, ErrorCode::CheckoutDirty);
    fs::remove_file(readme.with_file_name("extra.txt")).expect("remove");

    // A removed checkout is made again as a fresh worktree; the shared
    // store still holds its commit, so nothing is fetched.
    fs::remove_dir_all(&first.location.path).expect("remove checkout");
    let again = clone_pinned(&fixture, &by_sha, &context).expect("fresh after removal");
    assert!(!again.location.reused);
    assert_eq!(fetches(&runner), 2, "the store already holds the commit");
    assert_eq!(again.location.commit_sha, fixture.first_commit);
    assert!(Path::new(&again.location.path).join("README.md").is_file());
}

#[test]
fn sparse_checkout_reuse_requires_its_own_destination_and_clean_scope() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let sparse = GhCloneRepoQuery {
        paths: one("src"),
        ..query()
    };

    let first = clone_pinned(&fixture, &sparse, &context).expect("sparse clone");
    let second = clone_pinned(&fixture, &sparse, &context).expect("sparse reuse");
    assert_eq!(fetches(&runner), 1);
    assert_eq!(second.location.path, first.location.path);
    assert_eq!(
        second.location.requested_paths,
        first.location.requested_paths
    );

    // The full checkout is a different destination, a second worktree of
    // the commit the store already holds.
    clone_pinned(&fixture, &query(), &context).expect("full clone");
    assert_eq!(fetches(&runner), 1);

    // A deleted tracked file in the scope is a local change.
    fs::remove_file(Path::new(&first.location.path).join("src/lib.rs")).expect("delete");
    let error = clone_pinned(&fixture, &sparse, &context).expect_err("deleted file");
    assert_eq!(error.code, ErrorCode::CheckoutDirty);
    assert_eq!(fetches(&runner), 1);
}

/// A checkout reads repository symlinks as the plain files it holds: the
/// shared store carries the policy for every worktree, fresh or reused, so
/// any git run there sees a clean tree.
#[test]
fn a_checkout_reads_the_symlink_policy_of_its_store() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let first = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let checkout = Path::new(&first.location.path);
    let policy_of = || git_output(checkout, &["config", "--get", "core.symlinks"]);
    assert_eq!(policy_of().trim(), "false");

    let second = clone_pinned(&fixture, &query(), &context).expect("reused clone");
    assert_eq!(fetches(&runner), 1, "the checkout is reused, not fetched");
    assert!(second.location.reused);
    assert_eq!(policy_of().trim(), "false");
}

/// The root of the checkout's shared git directory, canonical.
fn common_dir(checkout: &str) -> PathBuf {
    let output = git_output(
        Path::new(checkout),
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    );
    fs::canonicalize(output.trim()).expect("common dir")
}

/// Every commit of a repository is a linked worktree of the one shared
/// store: its objects are fetched once, and each checkout keeps its own
/// history view.
#[test]
fn commits_of_one_repository_share_one_store_of_linked_worktrees() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let first = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let next = fixture.push_second();
    let second = clone_pinned(&fixture, &query(), &context).expect("second commit");
    assert_eq!(second.location.commit_sha, next);
    assert_ne!(first.location.path, second.location.path);

    let stores = store_dirs(&home);
    assert_eq!(stores.len(), 1, "{stores:?}");
    assert!(stores[0].to_string_lossy().ends_with("__g1.git"));
    assert_eq!(steps(&runner, "initialize shared store"), 1);
    let store = fs::canonicalize(&stores[0]).expect("store");
    for row in [&first, &second] {
        let checkout = Path::new(&row.location.path);
        assert!(checkout.join(".git").is_file(), "a linked worktree");
        assert_eq!(common_dir(&row.location.path), store);
        assert_eq!(history(&row.location.path), 1);
    }
}

/// A repeat row is verified in place: no store step, no new worktree.
#[test]
fn a_repeat_row_reuses_its_worktree_without_touching_the_store() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let first = clone_pinned(&fixture, &query(), &context).expect("first clone");
    assert!(!first.location.reused);
    let before = runner
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .len();
    let again = clone_pinned(&fixture, &query(), &context).expect("repeat");
    assert!(again.location.reused);
    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    for step in ["fetch requested commit", "worktree", "store", "init"] {
        assert!(
            !seen[before..].iter().any(|call| call.contains(step)),
            "a reuse ran {step}: {:?}",
            &seen[before..]
        );
    }
}
