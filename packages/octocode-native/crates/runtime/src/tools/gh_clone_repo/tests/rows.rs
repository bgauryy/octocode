//! Clone rows: a missing repository, continuations and cache age, recorded size, history depth and runtime limits.
use super::bench::*;
use super::*;

#[test]
fn missing_repository_names_the_repository_with_a_recovery_hint() {
    let error = repository_not_found(&query());
    assert_eq!(error.code, ErrorCode::NotFound);
    assert_eq!(
        error.message,
        "Repository not found: fixture-owner/fixture-repo"
    );
    assert_eq!(error.hints.len(), 1);
    // The hint fits the 120-char guidance cap whole (the message names
    // the repository), so the response stage never cuts it mid-sentence.
    let long = repository_not_found(&parse_query(serde_json::json!({
        "owner": "o".repeat(39), "repo": "r".repeat(100),
        "mainGoal": "test", "reasoning": "long names"
    })));
    for hint in [&error.hints[0], &long.hints[0]] {
        assert!(hint.chars().count() <= 120, "{hint}");
        assert!(hint.ends_with('.'), "{hint}");
        assert!(hint.contains("token"), "{hint}");
    }
    // Beside the hint, a runnable search for the name finds who holds it.
    let find = &error.next.as_ref().expect("search lead")["findRepository"];
    assert_eq!(find["tool"], "ghSearchRepo", "{find}");
    assert_eq!(
        find["query"]["queries"][0]["keywords"],
        serde_json::json!(["fixture-repo"]),
        "{find}"
    );
    // Errors without recovery serialize exactly as before.
    let plain = CloneError::new(ErrorCode::GitFailed, "failed");
    assert_eq!(
        serde_json::to_value(plain).expect("serialize"),
        serde_json::json!({"code": "gitFailed", "message": "failed"})
    );
}

/// A clone row continues into the local tools on its checkout (the
/// sparse subtree when one was requested), and a cache hit names its age.
#[test]
fn clone_rows_continue_into_local_tools_and_name_the_cache_age() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let row = |result: &CloneResult| {
        let data = serde_json::to_value(result).expect("serialize");
        // The runtime gives every continuation its row's brief.
        let mut briefed = data.clone();
        for call in briefed["next"]
            .as_object_mut()
            .into_iter()
            .flat_map(|next| next.values_mut())
        {
            call["query"]["queries"][0]["mainGoal"] = serde_json::json!("test");
            call["query"]["queries"][0]["reasoning"] = serde_json::json!("clone fixture");
        }
        crate::contracts::validate_output(
            "ghCloneRepo",
            &serde_json::json!({"results":[{"index":0,"data":briefed}]}),
        )
        .expect("clone row satisfies the output contract");
        data
    };
    let fresh = row(&clone_pinned(&fixture, &query(), &context).expect("fresh clone"));
    let explore = &fresh["next"]["exploreClone"];
    assert_eq!(explore["tool"], "structureSearch", "{fresh}");
    assert_eq!(
        explore["query"]["queries"][0]["path"],
        fresh["location"]["localPath"]
    );
    // A fresh clone states its age like a hit, and drops what the caller
    // already knows (owner/repo) or what is constant (source, true flags).
    assert!(fresh["location"]["clonedAt"].is_string(), "{fresh}");
    assert!(fresh["location"]["expiresAt"].is_string(), "{fresh}");
    for absent in ["owner", "repo"] {
        assert!(fresh.get(absent).is_none(), "{fresh}");
    }
    for absent in ["source", "verified", "complete"] {
        assert!(fresh["location"].get(absent).is_none(), "{fresh}");
    }

    let cached = row(&clone_pinned(&fixture, &query(), &context).expect("cached clone"));
    assert_eq!(cached["location"]["cached"], true);
    let cloned_at = cached["location"]["clonedAt"].as_str().expect("clonedAt");
    let expires_at = cached["location"]["expiresAt"].as_str().expect("expiresAt");
    assert!(
        cloned_at.ends_with('Z') && expires_at > cloned_at,
        "{cached}"
    );

    let sparse = row(&clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            path: Some(one("src")),
            ..query()
        },
        &context,
    )
    .expect("sparse clone"));
    let local = sparse["location"]["localPath"].as_str().expect("localPath");
    let path = sparse["next"]["exploreClone"]["query"]["queries"][0]["path"]
        .as_str()
        .expect("path");
    assert_eq!(Path::new(path), Path::new(local).join("src"), "{sparse}");
    assert_eq!(sparse["next"]["exploreClone"]["tool"], "structureSearch");

    // A single checked-out file is read, not listed as a directory.
    let file = row(&clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            path: Some(one("src/lib.rs")),
            ..query()
        },
        &context,
    )
    .expect("sparse file clone"));
    let local = file["location"]["localPath"].as_str().expect("localPath");
    let explore = &file["next"]["exploreClone"];
    assert_eq!(explore["tool"], "localFetch", "{file}");
    let path = explore["query"]["queries"][0]["path"]
        .as_str()
        .expect("path");
    assert_eq!(
        Path::new(path),
        Path::new(local).join("src/lib.rs"),
        "{file}"
    );
    assert!(Path::new(path).is_file(), "{file}");
}

/// A cache hit costs no git availability probe and no checkout walk: a
/// failing `rev-parse` already diagnoses a missing git, and the size was
/// recorded when the checkout was published.
#[test]
fn cache_hits_skip_the_git_probe_and_report_the_recorded_size() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    runner
        .seen
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    let cached = clone_pinned(&fixture, &query(), &context).expect("cached clone");
    assert!(cached.location.cached);
    assert_eq!(cached.bytes, fresh.bytes);
    assert!(fresh.bytes > 0);
    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    // A cache hit never probes git.
    test_support::checks::assert_absent(&serde_json::json!(seen), "--version");
}

/// `depth` fetches that many commits and keys its own cache entry.
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

/// A persistent cache runs under the runtime's constant limits; a test
/// runtime's limits replace them field for field.
#[test]
fn persistent_defaults_are_the_runtime_constants_and_limits_apply() {
    let base = CloneConfig::persistent("/h");
    assert_eq!(base.cache_ttl, Duration::from_secs(24 * 60 * 60));
    assert_eq!(base.max_cache_size_bytes, 2 * 1024 * 1024 * 1024);
    assert_eq!(base.max_clone_count, 50);
    assert_eq!(base.network_timeout, Duration::from_secs(30));
    let config = CloneConfig::persistent("/h").with_limits(&crate::execution::CloneLimits {
        ttl: Duration::from_secs(120),
        max_bytes: 1024 * 1024,
        max_clones: 7,
    });
    assert_eq!(config.cache_ttl, Duration::from_secs(120));
    assert_eq!(config.max_clone_count, 7);
    assert_eq!(config.max_cache_size_bytes, 1024 * 1024);
}
