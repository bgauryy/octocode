//! Credential-named files are never written to a checkout, sparse or full, and are listed in `location.skipped`.
use super::*;
use octocode_runtime_base::policy::discovery::is_sensitive_repository_file;

const SECRETS: [&str; 2] = [".env", "examples/auth0/.env.local.example"];

/// The fixture origin plus a second commit that adds credential-named files
/// at the root and nested, each beside an ordinary file.
pub(super) fn fixture_with_secrets() -> Fixture {
    let fixture = Fixture::new();
    for secret in SECRETS {
        assert!(
            is_sensitive_repository_file(secret),
            "{secret} not sensitive"
        );
    }
    fs::create_dir_all(fixture.work.join("examples/auth0")).expect("examples");
    fs::write(fixture.work.join(".env"), "TOKEN=hunter2\n").expect("env");
    fs::write(
        fixture.work.join("examples/auth0/.env.local.example"),
        "AUTH0=hunter2\n",
    )
    .expect("nested env");
    fs::write(fixture.work.join("examples/auth0/app.js"), "ok\n").expect("app");
    git(&fixture.work, &["add", "."]);
    git(&fixture.work, &["commit", "-m", "secrets"]);
    git(&fixture.work, &["push", "origin", "main"]);
    fixture
}

fn clone_with(fixture: &Fixture, query: GhCloneRepoQuery) -> CloneResult {
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let first = clone_pinned(fixture, &query, &context).expect("clone");
    // The second clone reuses the verified checkout and still withholds.
    let again = clone_pinned(fixture, &query, &context).expect("reused clone");
    assert!(again.location.reused, "{again:?}");
    assert_eq!(again.location.skipped, first.location.skipped);
    // Keep the temp root alive until here, then hand back the first result.
    drop(root);
    first
}

fn skipped(result: &CloneResult) -> Vec<&str> {
    let mut files = result
        .location
        .skipped
        .iter()
        .flatten()
        .map(String::as_str)
        .collect::<Vec<_>>();
    files.sort_unstable();
    files
}

#[test]
fn a_full_clone_leaves_out_credential_named_files() {
    let fixture = fixture_with_secrets();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let result = clone_pinned(&fixture, &query(), &context).expect("full clone");
    let local = Path::new(&result.location.path);
    for secret in SECRETS {
        assert!(!local.join(secret).exists(), "{secret} was written");
    }
    assert!(local.join("README.md").is_file());
    assert!(local.join("examples/auth0/app.js").is_file());
    assert_eq!(skipped(&result), SECRETS);
}

#[test]
fn a_sparse_directory_clone_leaves_out_credential_named_files_in_scope() {
    let fixture = fixture_with_secrets();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let result = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("examples"),
            ..query()
        },
        &context,
    )
    .expect("sparse clone");
    let local = Path::new(&result.location.path);
    assert!(local.join("examples/auth0/app.js").is_file());
    for secret in SECRETS {
        assert!(!local.join(secret).exists(), "{secret} was written");
    }
    // Only the credential-named files inside the requested scope are listed.
    assert_eq!(skipped(&result), ["examples/auth0/.env.local.example"]);
}

#[test]
fn an_explicitly_requested_credential_file_is_skipped_not_missing() {
    let fixture = fixture_with_secrets();
    let result = clone_with(
        &fixture,
        GhCloneRepoQuery {
            paths: many(&[".env", "README.md"]),
            ..query()
        },
    );
    assert_eq!(skipped(&result), [".env"]);
}

/// Every object the checkout's repository holds, as text.
fn stored_objects(local: &Path) -> String {
    let output = fixture_git(local, &["cat-file", "--batch-all-objects", "--batch"]);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A credential-named file's content never reaches the object store, whether
/// the clone is full or sparse: its blob is not fetched.
#[test]
fn credential_content_is_never_downloaded() {
    let fixture = fixture_with_secrets();
    for paths in [None, Some("examples")] {
        let (root, home) = home_root();
        let policy = test_support::workspace_policy(&root.0);
        let endpoint = api_github_endpoint();
        let runner = fixture_runner(&fixture);
        let config = GitConfig::persistent(&home);
        let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
        let result = clone_pinned(
            &fixture,
            &GhCloneRepoQuery {
                paths: paths.map(one).unwrap_or_default(),
                ..query()
            },
            &context,
        )
        .expect("clone");
        let objects = stored_objects(Path::new(&result.location.path));
        assert!(!objects.contains("hunter2"), "secret stored ({paths:?})");
        // The ordinary file beside it was fetched, so the probe sees blobs.
        assert!(objects.contains("ok\n"), "no blob stored ({paths:?})");
    }
}

/// A remote that ignores the blob filter would send the credential blob
/// anyway: a clone that withholds it refuses instead of storing it.
#[test]
fn a_remote_that_ignores_the_blob_filter_is_refused() {
    let fixture = fixture_with_secrets();
    let bare = url::Url::parse(&fixture.bare_url)
        .expect("url")
        .to_file_path()
        .expect("path");
    git(&bare, &["config", "uploadpack.allowFilter", "false"]);
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let error = clone_pinned(&fixture, &query(), &context).expect_err("refused");
    assert_eq!(error.code, ErrorCode::GitFailed, "{error:?}");
    assert!(error.message.contains("ignored"), "{error:?}");
}

/// A credential blob that reached the store (a rule set that withheld less
/// once) is never served again: the next row materializes in a fresh store
/// generation, and the checkout that uses the old one keeps working.
#[test]
fn a_store_holding_a_credential_blob_is_replaced_by_a_new_generation() {
    let fixture = fixture_with_secrets();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let old = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let stores = store_dirs(&home);
    assert_eq!(stores.len(), 1, "{stores:?}");
    let secret = root.0.join("secret.txt");
    fs::write(&secret, "TOKEN=hunter2\n").expect("secret");
    git(
        &stores[0],
        &["hash-object", "-w", secret.to_str().expect("utf8")],
    );

    let sparse = GhCloneRepoQuery {
        paths: many(&[".env", "README.md"]),
        ..query()
    };
    let next = clone_pinned(&fixture, &sparse, &context).expect("row after the blob arrived");
    let stores = store_dirs(&home);
    assert_eq!(stores.len(), 2, "{stores:?}");
    assert!(stores[1].to_string_lossy().ends_with("__g2.git"));
    assert!(Path::new(&next.location.path).join("README.md").is_file());
    let old_path = Path::new(&old.location.path);
    assert_eq!(
        git_output(old_path, &["rev-parse", "HEAD"]).trim(),
        old.location.commit_sha
    );
    assert!(
        old_path.join("README.md").is_file(),
        "the old checkout works"
    );
}

/// The same store, asked for a commit it does not hold yet: the fetch brings
/// the commit, the check finds the credential blob an earlier rule set stored,
/// and the row still succeeds on its first try, in a new generation. Only a
/// store this call made may blame the remote for the blob.
#[test]
fn a_poisoned_store_serves_a_new_commit_from_a_new_generation_on_the_first_try() {
    let fixture = fixture_with_secrets();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    clone_pinned(&fixture, &query(), &context).expect("first clone");
    let stores = store_dirs(&home);
    let secret = root.0.join("secret.txt");
    fs::write(&secret, "TOKEN=hunter2\n").expect("secret");
    git(
        &stores[0],
        &["hash-object", "-w", secret.to_str().expect("utf8")],
    );
    fixture.push_commit("two.txt");

    let next = clone_pinned(&fixture, &query(), &context).expect("a new commit, first try");
    assert!(Path::new(&next.location.path).join("two.txt").is_file());
    let stores = store_dirs(&home);
    assert_eq!(stores.len(), 2, "{stores:?}");
    assert!(stores[1].to_string_lossy().ends_with("__g2.git"));
}

/// A credential-named file whose content another, ordinary file of the
/// commit also holds (an empty `.env` beside a `.gitkeep`, `KEY=x` in both
/// `.env` and `unsafe.txt`): the checkout writes the ordinary file, so its
/// blob is in the store legitimately. That checkout is clean and is reused,
/// full or sparse, in the same store generation, and still withholds the
/// credential file; a real local edit is still refused as `checkoutDirty`.
#[test]
fn a_withheld_file_sharing_content_with_a_written_one_is_reused() {
    let fixture = Fixture::new();
    fs::create_dir_all(fixture.work.join("config")).expect("config");
    for (name, content) in [
        ("config/.env", "KEY=unsafe\n"),
        ("config/unsafe.txt", "KEY=unsafe\n"),
        ("config/empty/.env", ""),
        ("config/empty/.gitkeep", ""),
    ] {
        let path = fixture.work.join(name);
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(path, content).expect("write");
    }
    git(&fixture.work, &["add", "."]);
    git(&fixture.work, &["commit", "-m", "shared content"]);
    git(&fixture.work, &["push", "origin", "main"]);
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    for paths in [None, Some("config")] {
        let query = GhCloneRepoQuery {
            paths: paths.map(one).unwrap_or_default(),
            ..query()
        };
        let first = clone_pinned(&fixture, &query, &context).expect("clone");
        assert_eq!(skipped(&first), ["config/.env", "config/empty/.env"]);
        let local = Path::new(&first.location.path);
        assert!(local.join("config/unsafe.txt").is_file());
        assert!(!local.join("config/.env").exists());
        let again = clone_pinned(&fixture, &query, &context).expect("second clone");
        assert!(again.location.reused, "{paths:?}: {again:?}");
        assert_eq!(again.location.cloned_at, first.location.cloned_at);
        assert_eq!(skipped(&again), skipped(&first));
        assert_eq!(store_dirs(&home).len(), 1, "{:?}", store_dirs(&home));
    }
    let full = clone_pinned(&fixture, &query(), &context).expect("reuse");
    let edited = Path::new(&full.location.path).join("config/unsafe.txt");
    fs::write(&edited, "edited\n").expect("edit");
    let error = clone_pinned(&fixture, &query(), &context).expect_err("edited checkout");
    assert_eq!(error.code, ErrorCode::CheckoutDirty, "{error:?}");
    assert_eq!(fs::read_to_string(&edited).expect("read"), "edited\n");
}
