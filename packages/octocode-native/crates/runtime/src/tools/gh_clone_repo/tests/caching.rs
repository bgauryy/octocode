//! Caching and refresh: cached, refreshed, sparse, tag and commit clones, and dirty checkouts preserved.
use super::*;

#[test]
fn clones_caches_refreshes_sparse_tag_and_commit_without_token_argv() {
    let fixture = Fixture::new();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let derived = "https://github.com/fixture-owner/fixture-repo.git";
    let runner = RewriteRunner::new(derived, &fixture.bare_url);
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(60);
    let credential = ResolvedCredential::new("fixture-token", CredentialSource::Override);
    let context = setup(
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        Some(&credential),
    );

    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    assert!(!fresh.location.cached);
    assert_eq!(fresh.location.commit_sha, fixture.first_commit);
    assert_eq!(
        fs::read_to_string(Path::new(&fresh.location.local_path).join("README.md"))
            .expect("read clone"),
        "fixture\n"
    );
    assert!(fresh.bytes > 0);
    let cached = clone_pinned(&fixture, &query(), &context).expect("cached clone");
    assert!(cached.location.cached);
    assert_eq!(cached.location.local_path, fresh.location.local_path);
    // Editing the cached working tree must not replace the user's bytes.
    let readme = Path::new(&cached.location.local_path).join("README.md");
    fs::write(&readme, "tampered\n").expect("tamper cached checkout");
    let error = clone_pinned(&fixture, &query(), &context).expect_err("preserve dirty checkout");
    assert_eq!(error.code, ErrorCode::CheckoutDirty);
    assert_eq!(
        fs::read_to_string(&readme).expect("read preserved checkout"),
        "tampered\n"
    );
    fs::write(&readme, "fixture\n").expect("restore owned fixture");
    // `run` pins every row to a ref first: without one there is nothing to
    // check out.
    let unpinned = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: None,
            ..query()
        },
        &context,
    )
    .expect_err("an unpinned row");
    assert_eq!(unpinned.code, ErrorCode::InvalidInput);

    let second = fixture.push_second();
    let refreshed = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            force_refresh: Some(true),
            ..query()
        },
        &context,
    )
    .expect("refresh");
    assert_eq!(refreshed.location.commit_sha, second);
    assert_eq!(
        fs::read_to_string(Path::new(&refreshed.location.local_path).join("README.md"))
            .expect("read refreshed clone"),
        "fixture two\n"
    );

    let sparse = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("src"),
            ..query()
        },
        &context,
    )
    .expect("sparse clone");
    assert_eq!(sparse.location.kind, "tree");
    assert!(
        Path::new(&sparse.location.local_path)
            .join("src/lib.rs")
            .is_file()
    );
    assert!(
        !Path::new(&sparse.location.local_path)
            .join("other/skip.txt")
            .exists()
    );

    for branch in ["v1".to_owned(), fixture.first_commit.clone()] {
        let pinned = clone_pinned(
            &fixture,
            &GhCloneRepoQuery {
                ref_: Some(reference(&branch)),
                ..query()
            },
            &context,
        )
        .expect("pinned clone");
        assert_eq!(pinned.location.commit_sha, fixture.first_commit);
    }
    let pinned_sparse = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: Some(reference(&fixture.first_commit)),
            paths: one("src"),
            ..query()
        },
        &context,
    )
    .expect("pinned sparse clone");
    assert_eq!(pinned_sparse.location.commit_sha, fixture.first_commit);
    assert!(
        Path::new(&pinned_sparse.location.local_path)
            .join("src/lib.rs")
            .is_file()
    );
    assert!(
        !Path::new(&pinned_sparse.location.local_path)
            .join("other/skip.txt")
            .exists()
    );

    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    test_support::checks::assert_absent(&serde_json::json!(*seen), "fixture-token");
    assert!(seen.iter().any(|value| value.contains("[REDACTED]")));
}

#[test]
fn dirty_checkout_preserves_tracked_untracked_ignored_and_user_metadata_names() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    let checkout = Path::new(&fresh.location.local_path);
    for (name, ignored) in [
        ("README.md", false),
        ("untracked.txt", false),
        ("ignored.txt", true),
        (".octocode-user-notes", false),
        ("nested/.octocode-clone-meta.json", false),
        ("notes\n中.txt", false),
    ] {
        let file = checkout.join(name);
        fs::create_dir_all(file.parent().expect("file parent")).expect("marker parent");
        if ignored {
            fs::write(
                checkout.join(".git/info/exclude"),
                format!("ignored.txt\n/{}\n", cache::META_FILE),
            )
            .expect("ignore fixture file");
        }
        fs::write(&file, "user evidence\n").expect("edit owned fixture");
        for force_refresh in [false, true] {
            let request = GhCloneRepoQuery {
                force_refresh: Some(force_refresh),
                ..query()
            };
            // Ignored bytes do not change the verified revision, but replacing
            // its checkout must still preserve them.
            if ignored && !force_refresh {
                assert!(
                    clone_pinned(&fixture, &request, &context)
                        .expect("ignored reuse")
                        .location
                        .cached
                );
            } else {
                let error =
                    clone_pinned(&fixture, &request, &context).expect_err("preserve user file");
                assert_eq!(
                    error.code,
                    ErrorCode::CheckoutDirty,
                    "{name}, refresh={force_refresh}"
                );
                assert!(!error.hints.is_empty(), "actionable recovery");
            }
            assert_eq!(
                fs::read_to_string(&file).expect("preserved file"),
                "user evidence\n"
            );
            assert_eq!(
                git_output(checkout, &["rev-parse", "HEAD"]).trim(),
                fixture.first_commit
            );
        }
        if name == "README.md" {
            fs::write(&file, "fixture\n").expect("restore owned tracked file");
        } else {
            fs::remove_file(&file).expect("remove owned marker");
        }
    }
}

#[test]
fn refresh_rechecks_user_changes_written_while_fetching() {
    struct LateWriteRunner {
        inner: RewriteRunner,
        target: Mutex<Option<PathBuf>>,
    }
    impl GitRunner for LateWriteRunner {
        fn run(
            &self,
            request: &GitRunRequest<'_>,
            control: &GitRunControl<'_>,
        ) -> Result<GitOutput, CloneError> {
            let output = self.inner.run(request, control)?;
            if request.label == "read checkout HEAD"
                && let Some(target) = self.target.lock().expect("target lock").take()
            {
                fs::write(
                    target.join("during-fetch.txt"),
                    "concurrent user evidence\n",
                )
                .expect("write owned late marker");
            }
            Ok(output)
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
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = LateWriteRunner {
        inner: fixture_runner(&fixture),
        target: Mutex::new(None),
    };
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    let checkout = PathBuf::from(fresh.location.local_path);
    *runner.target.lock().expect("target lock") = Some(checkout.clone());
    let error = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            force_refresh: Some(true),
            ..query()
        },
        &context,
    )
    .expect_err("preserve late write");
    assert_eq!(error.code, ErrorCode::CheckoutDirty);
    assert_eq!(
        fs::read_to_string(checkout.join("during-fetch.txt")).expect("late marker"),
        "concurrent user evidence\n"
    );
    assert_eq!(
        git_output(&checkout, &["rev-parse", "HEAD"]).trim(),
        fixture.first_commit
    );
}

#[test]
fn uppercase_commit_refs_share_the_cache_and_mismatched_meta_is_a_miss() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(60);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let lower = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: Some(reference(&fixture.first_commit)),
            ..query()
        },
        &context,
    )
    .expect("lowercase sha clone");
    let upper = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: Some(reference(&fixture.first_commit.to_ascii_uppercase())),
            ..query()
        },
        &context,
    )
    .expect("uppercase sha clone");
    assert!(upper.location.cached, "uppercase SHA missed the cache");
    assert_eq!(upper.location.local_path, lower.location.local_path);

    // Tamper the persisted meta so it describes a different checkout: the
    // next request must re-clone rather than trust the directory.
    let meta_path = Path::new(&lower.location.local_path).join(cache::META_FILE);
    let mut meta: serde_json::Value =
        serde_json::from_slice(&fs::read(&meta_path).expect("meta")).expect("meta json");
    meta["repo"] = serde_json::json!("some-other-repo");
    fs::write(&meta_path, serde_json::to_vec(&meta).expect("json")).expect("write meta");
    let again = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: Some(reference(&fixture.first_commit)),
            ..query()
        },
        &context,
    )
    .expect("re-clone");
    assert!(!again.location.cached, "mismatched meta served from cache");
}
