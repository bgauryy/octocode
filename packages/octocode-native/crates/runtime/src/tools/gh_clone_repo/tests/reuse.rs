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
    assert_eq!(second.location.local_path, first.location.local_path);
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
    let readme = Path::new(&first.location.local_path).join("README.md");
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

    // A removed checkout is cloned fresh.
    fs::remove_dir_all(&first.location.local_path).expect("remove checkout");
    let again = clone_pinned(&fixture, &by_sha, &context).expect("fresh after removal");
    assert_eq!(fetches(&runner), 3);
    assert_eq!(again.location.commit_sha, fixture.first_commit);
    assert!(
        Path::new(&again.location.local_path)
            .join("README.md")
            .is_file()
    );
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
    assert_eq!(second.location.local_path, first.location.local_path);
    assert_eq!(
        second.location.requested_paths,
        first.location.requested_paths
    );

    // The full checkout is a different destination.
    clone_pinned(&fixture, &query(), &context).expect("full clone");
    assert_eq!(fetches(&runner), 2);

    // A deleted tracked file in the scope is a local change.
    fs::remove_file(Path::new(&first.location.local_path).join("src/lib.rs")).expect("delete");
    let error = clone_pinned(&fixture, &sparse, &context).expect_err("deleted file");
    assert_eq!(error.code, ErrorCode::CheckoutDirty);
    assert_eq!(fetches(&runner), 2);
}
