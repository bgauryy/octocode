//! Clone rows: missing repositories, local exploration, checkout sizes and history depth.
use super::bench::*;
use super::*;

#[test]
fn missing_repository_names_the_repository_with_a_recovery_hint() {
    let error = repository_not_found(&query());
    assert_eq!(error.code, ErrorCode::NotFound);
    // The message names the repository and the token's access, as on every
    // GitHub read.
    assert_eq!(
        error.message,
        crate::tools::gh_shared::repository_not_found_message("fixture-owner", "fixture-repo")
    );
    assert!(error.message.contains("token"), "{}", error.message);
    // The shared leads: a search for the name and the owner's listing,
    // with the shared hint that labels the name search's rows.
    assert_eq!(
        error.hints,
        [crate::tools::gh_shared::repository_search_hint(
            Some("fixture-owner"),
            "fixture-repo"
        )]
    );
    let calls = error.calls.as_deref().expect("recovery leads");
    assert_eq!(
        calls,
        &crate::tools::gh_shared::repository_recovery(Some("fixture-owner"), "fixture-repo")
    );
    assert_eq!(
        calls["findRepository"]["input"]["queries"][0]["keywords"],
        serde_json::json!(["fixture-repo"]),
        "{calls}"
    );
    // Errors without recovery serialize exactly as before.
    let plain = CloneError::new(ErrorCode::GitFailed, "failed");
    assert_eq!(
        serde_json::to_value(plain).expect("serialize"),
        serde_json::json!({"code": "gitFailed", "message": "failed"})
    );
}

/// A clone row continues into the local tools on its checkout (the
/// sparse subtree when one was requested), and every fetch reports its completion time.
#[test]
fn clone_rows_continue_into_local_tools_and_name_the_checkout_time() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let row = |result: &CloneResult| {
        let data = serde_json::to_value(result).expect("serialize");
        crate::tools::test_support::validate_sealed(
            "ghCloneRepo",
            &serde_json::json!({"results":[{"index":0,"data":data.clone()}]}),
        )
        .expect("clone row satisfies the output contract");
        data
    };
    let fresh = row(&clone_pinned(&fixture, &query(), &context).expect("fresh clone"));
    let explore =
        crate::tools::test_support::checks::related_call(&fresh, "structureSearch", |_| true);
    assert_eq!(explore["tool"], "structureSearch", "{fresh}");
    assert_eq!(
        explore["input"]["queries"][0]["path"],
        fresh["location"]["localPath"]
    );
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
    assert_eq!(
        replaced["location"]["localPath"],
        fresh["location"]["localPath"]
    );

    let sparse = row(&clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("src"),
            ..query()
        },
        &context,
    )
    .expect("sparse clone"));
    let local = sparse["location"]["localPath"].as_str().expect("localPath");
    let path = crate::tools::test_support::checks::related_call(&sparse, "structureSearch", |_| {
        true
    })["input"]["queries"][0]["path"]
        .as_str()
        .expect("path");
    assert_eq!(Path::new(path), Path::new(local).join("src"), "{sparse}");
    assert_eq!(
        crate::tools::test_support::checks::related_call(&sparse, "structureSearch", |_| true)["tool"],
        "structureSearch"
    );

    // A single checked-out file is read, not listed as a directory.
    let file = row(&clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("src/lib.rs"),
            ..query()
        },
        &context,
    )
    .expect("sparse file clone"));
    let local = file["location"]["localPath"].as_str().expect("localPath");
    let explore = crate::tools::test_support::checks::related_call(&file, "localFetch", |_| true);
    assert_eq!(explore["tool"], "localFetch", "{file}");
    let path = explore["input"]["queries"][0]["path"]
        .as_str()
        .expect("path");
    assert_eq!(
        Path::new(path),
        Path::new(local).join("src/lib.rs"),
        "{file}"
    );
    assert!(Path::new(path).is_file(), "{file}");
}

/// Replacing a checkout probes Git and reports the freshly checked-out size.
#[test]
fn replacements_probe_git_and_report_the_checked_out_size() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
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
            .any(|request| request.contains("fetch requested commit"))
    );
}

/// `depth` fetches that many commits and keys its own destination.
#[test]
fn history_depth_fetches_that_many_commits() {
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
    assert_ne!(shallow.location.local_path, deep.location.local_path);
    assert_eq!(deep.location.history_depth, Some(2));
    assert_eq!(shallow.location.history_depth, None);
    let count = |path: &str| {
        git_output(Path::new(path), &["rev-list", "--count", "HEAD"])
            .trim()
            .to_owned()
    };
    assert_eq!(count(&shallow.location.local_path), "1");
    assert_eq!(count(&deep.location.local_path), "2");
}

#[test]
fn clone_timeout_defaults_to_the_runtime_network_limit() {
    assert_eq!(
        CloneConfig::persistent("/h").network_timeout,
        crate::execution::RuntimeLimits::DEFAULT.network_timeout
    );
    assert_eq!(
        CloneConfig::persistent("/h")
            .with_network_timeout(Duration::from_secs(9))
            .network_timeout,
        Duration::from_secs(9)
    );
}
