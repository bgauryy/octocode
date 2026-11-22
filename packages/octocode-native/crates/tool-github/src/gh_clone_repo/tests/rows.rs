//! Clone rows: missing repositories, local exploration, checkout sizes and history depth.
use super::bench::*;
use super::*;
use octocode_test_support::json_fixture;

#[test]
fn missing_repository_names_the_repository_with_a_recovery_hint() {
    let error = repository_not_found(&query());
    assert_eq!(error.code, ErrorCode::NotFound);
    // The message names the repository and the token's access, as on every
    // GitHub read.
    assert_eq!(
        error.message,
        octocode_tools_shared::gh_shared::repository_not_found_message(
            "fixture-owner",
            "fixture-repo"
        )
    );
    assert!(error.message.contains("token"), "{}", error.message);
    // The shared leads: a search for the name and the owner's listing,
    // with the shared hint that labels the name search's rows.
    assert_eq!(
        error.hints,
        [octocode_tools_shared::gh_shared::REPOSITORY_SEARCH_HINT.to_owned()]
    );
    let calls = error.calls.as_deref().expect("recovery leads");
    assert_eq!(
        calls,
        &octocode_tools_shared::gh_shared::repository_recovery(
            Some("fixture-owner"),
            "fixture-repo"
        )
    );
    assert_eq!(
        calls["findRepository"]["input"]["queries"][0]["keywords"],
        json_fixture(r#"["fixture-repo"]"#),
        "{calls}"
    );
    // Errors without recovery serialize exactly as before.
    let plain = GitError::new(ErrorCode::GitFailed, "failed");
    assert_eq!(
        serde_json::to_value(plain).expect("serialize"),
        json_fixture(r#"{"code": "gitFailed", "message": "failed"}"#)
    );
}

/// A clone row names its checkout (and a sparse checkout's `paths` scope)
/// and the time of every fetch; it adds no related request: its path is the
/// next step the description names.
#[test]
fn clone_rows_name_their_checkout_scope_and_time() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let row = |result: &CloneResult| {
        let data = serde_json::to_value(result).expect("serialize");
        octocode_runtime_base::test_support::validate_sealed(
            "ghCloneRepo",
            &serde_json::json!({"results":[{"index":0,"data":data.clone()}]}),
        )
        .expect("clone row satisfies the output contract");
        data
    };
    let fresh = row(&clone_pinned(&fixture, &query(), &context).expect("fresh clone"));
    assert!(fresh["location"].get("paths").is_none(), "{fresh}");
    assert!(fresh.get("related").is_none(), "{fresh}");
    assert!(Path::new(fresh["location"]["path"].as_str().expect("path")).is_dir());
    assert!(fresh["location"]["clonedAt"].is_string(), "{fresh}");
    for absent in ["owner", "repo"] {
        assert!(fresh.get(absent).is_none(), "{fresh}");
    }
    for absent in ["source", "verified", "complete", "cached", "expiresAt"] {
        assert!(fresh["location"].get(absent).is_none(), "{fresh}");
    }

    let replaced = row(&clone_pinned(&fixture, &query(), &context).expect("fresh replacement"));
    assert!(
        replaced["location"]["clonedAt"]
            .as_str()
            .unwrap()
            .ends_with('Z')
    );
    assert_eq!(replaced["location"]["path"], fresh["location"]["path"]);

    let sparse = row(&clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("src"),
            ..query()
        },
        &context,
    )
    .expect("sparse clone"));
    let local = sparse["location"]["path"].as_str().expect("path");
    assert!(
        sparse["location"].get("requestedPaths").is_none(),
        "{sparse}"
    );
    assert!(Path::new(local).join("src").is_dir(), "{sparse}");
    assert_eq!(sparse["location"]["paths"], json_fixture(r#"["src"]"#));
    assert!(sparse.get("related").is_none(), "{sparse}");

    // A single checked-out file: the scope names the file.
    let file = row(&clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("src/lib.rs"),
            ..query()
        },
        &context,
    )
    .expect("sparse file clone"));
    let local = file["location"]["path"].as_str().expect("path");
    assert!(file["location"].get("requestedPaths").is_none(), "{file}");
    assert!(Path::new(local).join("src/lib.rs").is_file(), "{file}");
    assert_eq!(file["location"]["paths"], json_fixture(r#"["src/lib.rs"]"#));
    assert!(file.get("related").is_none(), "{file}");
}

/// Reusing a checkout probes Git, fetches nothing and reports its size.
#[test]
fn reuse_probes_git_and_reports_the_checked_out_size() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    runner
        .seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
    let replaced = clone_pinned(&fixture, &query(), &context).expect("replacement clone");
    assert_eq!(replaced.bytes, fresh.bytes);
    assert!(fresh.bytes > 0);
    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    assert!(seen.iter().any(|request| request.contains("--version")));
    assert!(
        seen.iter()
            .any(|request| request.contains("check checkout cleanliness"))
    );
    assert!(
        !seen
            .iter()
            .any(|request| request.contains("fetch requested commit"))
    );
}

/// Distinct requested depths keep their destinations while sharing full history.
#[test]
fn requested_depths_share_history_across_checkout_destinations() {
    let bench = Bench::committing(api_github_endpoint());
    let context = bench.context();
    bench.fixture.push_second();
    let shallow = clone_pinned(&bench.fixture, &query(), &context).expect("shallow");
    let deep = clone_pinned(
        &bench.fixture,
        &GhCloneRepoQuery {
            history_depth: std::num::NonZeroU64::new(2).expect("depth"),
            ..query()
        },
        &context,
    )
    .expect("depth 2");
    assert_ne!(shallow.location.path, deep.location.path);
    assert_eq!(deep.location.history_depth, Some(2));
    assert_eq!(shallow.location.history_depth, None);
    let count = |path: &str| {
        git_output(Path::new(path), &["rev-list", "--count", "HEAD"])
            .trim()
            .to_owned()
    };
    assert_eq!(count(&shallow.location.path), count(&deep.location.path));
    assert_eq!(count(&deep.location.path), "2");
}

#[test]
fn clone_timeout_defaults_to_the_runtime_network_limit() {
    assert_eq!(
        GitConfig::persistent("/h").network_timeout,
        octocode_runtime_base::execution::RuntimeLimits::DEFAULT.network_timeout
    );
    assert_eq!(
        GitConfig::persistent("/h")
            .with_network_timeout(Duration::from_secs(9))
            .network_timeout,
        Duration::from_secs(9)
    );
}

/// `clonedAt` is the completion time of the fetch that made the checkout:
/// a reuse reports the very time the fresh clone did, never the time the
/// stage directory was created while the fetch was still running.
#[test]
fn reuse_reports_the_fresh_clones_completion_time() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    std::thread::sleep(std::time::Duration::from_millis(20));
    let reused = clone_pinned(&fixture, &query(), &context).expect("reuse");
    assert!(fresh.location.cloned_at.is_some());
    assert_eq!(fresh.location.cloned_at, reused.location.cloned_at);
    // Both receipts state whether the checkout was reused.
    let receipt = |result: &CloneResult| serde_json::to_value(result).expect("receipt");
    assert_eq!(
        receipt(&fresh)["location"]["reused"],
        false,
        "{}",
        receipt(&fresh)
    );
    assert_eq!(
        receipt(&reused)["location"]["reused"],
        true,
        "{}",
        receipt(&reused)
    );
}

fn at_depth(commit: &str, depth: u64) -> GhCloneRepoQuery {
    GhCloneRepoQuery {
        ref_: Some(reference(commit)),
        history_depth: std::num::NonZeroU64::new(depth).expect("depth"),
        ..query()
    }
}

/// Refs requested out of order reuse the full history of the first fetch.
#[test]
fn default_rows_share_the_full_commit_graph() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let commits = [
        fixture.first_commit.clone(),
        fixture.push_commit("two.txt"),
        fixture.push_commit("three.txt"),
    ];
    // Out of order, so a later fetch is an ancestor of an earlier one.
    let rows = [2, 0, 1].map(|index| {
        clone_pinned(&fixture, &at_depth(&commits[index], 1), &context).expect("depth 1 row")
    });
    assert_eq!(store_dirs(&home).len(), 1, "{:?}", store_dirs(&home));
    for (row, expected) in rows.iter().zip([3, 1, 2]) {
        assert_eq!(history(&row.location.path), expected, "{row:?}");
    }
}

/// A second tip can traverse the same objects without cutting the first
/// checkout's ancestry. A branch or tag is available before its own checkout.
#[test]
fn deeper_rows_share_one_store_and_keep_each_tips_ancestry() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    fixture.push_commit("two.txt");
    let third = fixture.push_commit("three.txt");
    let fourth = fixture.push_commit("four.txt");

    let first = clone_pinned(&fixture, &at_depth(&third, 3), &context).expect("C3");
    assert_eq!(
        git_output(
            Path::new(&first.location.path),
            &["rev-parse", "refs/octocode/tags/v1^{commit}"]
        )
        .trim(),
        fixture.first_commit
    );
    assert_eq!(history(&first.location.path), 3);
    let second = clone_pinned(&fixture, &at_depth(&fourth, 3), &context).expect("C4");
    assert_eq!(history(&second.location.path), 4);
    assert_eq!(
        history(&first.location.path),
        3,
        "a descendant's fetch shortened an earlier row"
    );
    assert_eq!(store_dirs(&home).len(), 1, "{:?}", store_dirs(&home));
    assert_eq!(steps(&runner, "fetch requested commit"), 1);
    assert_eq!(
        git_output(
            Path::new(&first.location.path),
            &["rev-parse", "refs/remotes/origin/main"]
        )
        .trim(),
        fourth
    );
}

#[test]
fn a_shallow_remote_cannot_supply_a_full_history_receipt() {
    let bench = Bench::committing(api_github_endpoint());
    let tip = bench.fixture.push_commit("second.txt");
    let remote = url::Url::parse(&bench.fixture.bare_url)
        .unwrap()
        .to_file_path()
        .unwrap();
    fs::write(remote.join("shallow"), format!("{tip}\n")).unwrap();
    let error = clone_pinned(&bench.fixture, &at_depth(&tip, 1), &bench.context())
        .expect_err("incomplete ancestry");
    assert_eq!(error.code, ErrorCode::GitFailed, "{error:?}");
    assert!(error.message.contains("shallow"), "{error:?}");
}

/// A repository with fewer commits than `historyDepth` asks for is complete
/// in the store: a second row of the same commit and depth (another scope)
/// finds the root commit and does not fetch again.
#[test]
fn a_history_shorter_than_the_depth_is_not_fetched_again() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let deep = |paths: Vec<_>| GhCloneRepoQuery {
        history_depth: std::num::NonZeroU64::new(5).expect("depth"),
        paths,
        ..query()
    };
    clone_pinned(&fixture, &deep(Vec::new()), &context).expect("whole");
    clone_pinned(&fixture, &deep(one("src")), &context).expect("scoped");
    assert_eq!(steps(&runner, "fetch requested commit"), 1, "one fetch");
}

/// Fresh and reused checkouts disclose full history for every requested depth.
#[test]
fn every_depth_discloses_full_ancestry() {
    let bench = Bench::committing(api_github_endpoint());
    let context = bench.context();
    bench.fixture.push_second();
    bench.fixture.push_commit("three.txt");
    let origin: usize = git_output(&bench.fixture.work, &["rev-list", "--count", "HEAD"])
        .trim()
        .parse()
        .expect("count");
    for depth in [2, 3, 50] {
        for _ in 0..2 {
            let clone = clone_pinned(
                &bench.fixture,
                &GhCloneRepoQuery {
                    history_depth: std::num::NonZeroU64::new(depth).expect("depth"),
                    ..query()
                },
                &context,
            )
            .expect("deep clone");
            let whole = history(&clone.location.path) == origin;
            assert_eq!(
                clone.location.full_history, whole,
                "depth {depth}: {clone:?}"
            );
            let served = serde_json::to_value(&clone).expect("serialize");
            assert_eq!(
                served["location"].get("fullHistory").is_some(),
                whole,
                "{served}"
            );
        }
    }
    let shallow = clone_pinned(&bench.fixture, &query(), &context).expect("shallow");
    assert!(shallow.location.full_history, "{shallow:?}");
}
