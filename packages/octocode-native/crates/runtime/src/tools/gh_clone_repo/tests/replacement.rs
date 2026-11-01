//! Fresh replacement, sparse scopes, refs and preservation of local changes.
use super::bench::Bench;
use super::*;

#[test]
fn fresh_replacements_preserve_sparse_refs_and_keep_tokens_out_of_argv() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let derived = "https://github.com/fixture-owner/fixture-repo.git";
    let runner = RewriteRunner::new(derived, &fixture.bare_url);
    let config = CloneConfig::persistent(&home);
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
    assert_eq!(fresh.location.commit_sha, fixture.first_commit);
    assert_eq!(
        fs::read_to_string(Path::new(&fresh.location.local_path).join("README.md"))
            .expect("read clone"),
        "fixture\n"
    );
    assert!(fresh.bytes > 0);
    let replaced = clone_pinned(&fixture, &query(), &context).expect("replacement clone");
    assert!(
        !Path::new(&replaced.location.local_path)
            .join(".octocode-clone-meta.json")
            .exists()
    );
    assert_eq!(replaced.location.local_path, fresh.location.local_path);
    // Editing the existing working tree must not replace the user's bytes.
    let readme = Path::new(&replaced.location.local_path).join("README.md");
    fs::write(&readme, "tampered\n").expect("tamper existing checkout");
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
    let refreshed =
        clone_pinned(&fixture, &GhCloneRepoQuery { ..query() }, &context).expect("refresh");
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
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
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
            fs::write(checkout.join(".git/info/exclude"), "ignored.txt\n")
                .expect("ignore fixture file");
        }
        fs::write(&file, "user evidence\n").expect("edit owned fixture");
        let error = clone_pinned(&fixture, &query(), &context).expect_err("preserve user file");
        assert_eq!(error.code, ErrorCode::CheckoutDirty, "{name}");
        assert!(!error.hints.is_empty());
        assert_eq!(fs::read_to_string(&file).unwrap(), "user evidence\n");
        assert_eq!(
            git_output(checkout, &["rev-parse", "HEAD"]).trim(),
            fixture.first_commit
        );
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
                && request
                    .args
                    .iter()
                    .any(|argument| argument.to_string_lossy().contains("clone-tmp"))
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
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = LateWriteRunner {
        inner: fixture_runner(&fixture),
        target: Mutex::new(None),
    };
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    let checkout = PathBuf::from(fresh.location.local_path);
    *runner.target.lock().expect("target lock") = Some(checkout.clone());
    let error = clone_pinned(&fixture, &GhCloneRepoQuery { ..query() }, &context)
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
fn uppercase_commit_refs_replace_the_same_destination() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
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
    assert_eq!(upper.location.local_path, lower.location.local_path);
}

#[test]
fn local_commits_and_refs_are_preserved_before_replacement() {
    for kind in ["tag", "custom-ref", "commit", "branch", "stash"] {
        let bench = Bench::at(api_github_endpoint());
        let context = bench.context();
        let first = clone_pinned(&bench.fixture, &query(), &context).unwrap();
        let path = Path::new(&first.location.local_path);
        assert!(git_output(path, &["for-each-ref"]).is_empty());
        match kind {
            "tag" => git(path, &["tag", "local-work"]),
            "custom-ref" => git(path, &["update-ref", "refs/notes/local-work", "HEAD"]),
            "commit" => {
                fs::write(path.join("README.md"), "local committed evidence\n").unwrap();
                git(path, &["add", "README.md"]);
                git(
                    path,
                    &[
                        "-c",
                        "user.name=Fixture",
                        "-c",
                        "user.email=fixture@example.test",
                        "commit",
                        "-m",
                        "local evidence",
                    ],
                );
            }
            "branch" => git(path, &["branch", "local-work"]),
            "stash" => {
                fs::write(path.join("README.md"), "stashed evidence\n").unwrap();
                git(
                    path,
                    &[
                        "-c",
                        "user.name=Fixture",
                        "-c",
                        "user.email=fixture@example.test",
                        "stash",
                        "push",
                        "-m",
                        "local evidence",
                    ],
                );
            }
            _ => unreachable!(),
        }
        let head = git_output(path, &["rev-parse", "HEAD"]);
        let refs = git_output(path, &["for-each-ref"]);
        let error = clone_pinned(&bench.fixture, &query(), &context)
            .expect_err("local Git work must survive");
        assert_eq!(error.code, ErrorCode::CheckoutDirty, "{kind}");
        assert_eq!(git_output(path, &["rev-parse", "HEAD"]), head, "{kind}");
        assert_eq!(git_output(path, &["for-each-ref"]), refs, "{kind}");
    }
}
