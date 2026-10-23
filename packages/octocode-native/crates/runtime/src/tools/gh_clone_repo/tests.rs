use super::pin::api_refusal;
use super::validate::{is_sparse_path, repository_url, web_root};
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::policy::path::PathPolicyConfig;
use crate::tools::gh_shared::test_support as gh_shared_support;
use crate::tools::test_support;
use octocode_contract_types::ErrorCode;
use octocode_contract_types::{GhCloneRepoQueryPathArrayItem, GhCloneRepoQueryRef};
use octocode_github::CredentialSource;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

fn api_github_endpoint() -> GitHubEndpoint {
    GitHubEndpoint::new("https://api.github.com/".parse().expect("static URL"))
        .expect("static endpoint")
}

struct RewriteRunner {
    system: SystemGit,
    from: OsString,
    to: OsString,
    seen: Mutex<Vec<String>>,
}

impl RewriteRunner {
    fn new(from: &str, to: &str) -> Self {
        Self::with_git(from, to, SystemGit::default())
    }

    fn with_git(from: &str, to: &str, system: SystemGit) -> Self {
        Self {
            system,
            from: from.into(),
            to: to.into(),
            seen: Mutex::new(vec![]),
        }
    }
}

impl RewriteRunner {
    fn rewrite<'a>(&self, request: &GitRunRequest<'a>) -> GitRunRequest<'a> {
        self.seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(format!("{request:?}"));
        let args = request
            .args
            .iter()
            .map(|argument| {
                if argument == &self.from {
                    self.to.clone()
                } else {
                    argument.clone()
                }
            })
            .collect();
        let request = request.with_args(args);
        // Hermetic guard: the rewrite must leave no network remote for git.
        for argument in &request.args {
            let argument = argument.to_string_lossy();
            assert!(
                !["https://", "http://", "ssh://", "git@"]
                    .iter()
                    .any(|scheme| argument.starts_with(scheme)),
                "git would reach the network: {argument}"
            );
        }
        request
    }
}

impl GitRunner for RewriteRunner {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        self.system.run(&self.rewrite(request), control)
    }

    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError> {
        self.system
            .run_nul_records(&self.rewrite(request), control, record)
    }
}

/// A runner for paths that must refuse before git runs: invoking it fails
/// the test instead of reaching the network.
struct UnreachableGit;

impl GitRunner for UnreachableGit {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        _control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        panic!("git must not run here: {request:?}")
    }

    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        _control: &GitRunControl<'_>,
        _record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError> {
        panic!("git must not run here: {request:?}")
    }
}

struct Fixture {
    _temp: test_support::Fixture,
    work: PathBuf,
    bare_url: String,
    first_commit: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = test_support::Fixture::new();
        let work = temp.0.join("work");
        let bare = temp.0.join("origin.git");
        git(
            &temp.0,
            &[
                "init",
                "--initial-branch=main",
                work.to_str().expect("utf8"),
            ],
        );
        git(&work, &["config", "user.name", "Octocode Test"]);
        git(&work, &["config", "user.email", "test@example.invalid"]);
        fs::create_dir_all(work.join("src/nested")).expect("src");
        fs::create_dir_all(work.join("other")).expect("other");
        fs::write(work.join("README.md"), "fixture\n").expect("readme");
        fs::write(work.join("src/lib.rs"), "pub fn one() {}\n").expect("source");
        fs::write(work.join("src/nested/data.txt"), "data\n").expect("data");
        fs::write(work.join("other/skip.txt"), "skip\n").expect("skip");
        git(&work, &["add", "."]);
        git(&work, &["commit", "-m", "first"]);
        let first_commit = git_output(&work, &["rev-parse", "HEAD"]).trim().to_owned();
        git(&work, &["tag", "v1"]);
        git(
            &temp.0,
            &[
                "clone",
                "--bare",
                work.to_str().expect("utf8"),
                bare.to_str().expect("utf8"),
            ],
        );
        git(
            &work,
            &["remote", "add", "origin", bare.to_str().expect("utf8")],
        );
        let bare_url = url::Url::from_file_path(&bare)
            .expect("file URL")
            .to_string();
        Self {
            _temp: temp,
            work,
            bare_url,
            first_commit,
        }
    }

    /// `query` as `run` hands it to [`execute_clone`]: its ref pinned to the
    /// full commit it names in the fixture's origin (the GitHub API's answer
    /// in production). No ref, or one naming nothing, stays as it is.
    fn pin(&self, query: &GhCloneRepoQuery) -> GhCloneRepoQuery {
        let mut pinned = query.clone();
        if let Some(reference) = query
            .ref_
            .as_deref()
            .filter(|reference| !octocode_github::is_full_sha(reference))
        {
            let output = fixture_git(
                &self.work,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{reference}^{{commit}}"),
                ],
            );
            if output.status.success() {
                let commit = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                pinned.ref_ = Some(self::reference(&commit));
            }
        }
        pinned
    }

    /// The fixture the tests that never commit share: one origin while any
    /// test holds it, removed when the last holder drops it (a test that
    /// commits builds its own with [`Self::new`]).
    fn shared() -> Arc<Self> {
        static SHARED: Mutex<Weak<Fixture>> = Mutex::new(Weak::new());
        let mut slot = SHARED.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(fixture) = slot.upgrade() {
            return fixture;
        }
        let fixture = Arc::new(Self::new());
        *slot = Arc::downgrade(&fixture);
        fixture
    }

    fn push_second(&self) -> String {
        fs::write(self.work.join("README.md"), "fixture two\n").expect("update");
        git(&self.work, &["add", "README.md"]);
        git(&self.work, &["commit", "-m", "second"]);
        git(&self.work, &["push", "origin", "main"]);
        git_output(&self.work, &["rev-parse", "HEAD"])
            .trim()
            .to_owned()
    }
}

fn git(directory: &Path, args: &[&str]) {
    let output = fixture_git(directory, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output(directory: &Path, args: &[&str]) -> String {
    let output = fixture_git(directory, args);
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn fixture_git(directory: &Path, args: &[&str]) -> std::process::Output {
    fixture_git_command(directory, args)
        .output()
        .expect("run fixture git")
}

fn fixture_git_command(directory: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(directory)
        .env_clear()
        .env("HOME", directory)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env(
            "GIT_CONFIG_SYSTEM",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .arg("-c")
        .arg(format!(
            "core.hooksPath={}",
            if cfg!(windows) { "NUL" } else { "/dev/null" }
        ))
        .args(args);
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    command
}

fn setup<'a>(
    runner: &'a dyn GitRunner,
    config: &'a CloneConfig,
    endpoint: &'a GitHubEndpoint,
    policy: &'a PathPolicy,
    cancellation: &'a dyn CancellationCheck,
    credential: Option<&'a ResolvedCredential>,
) -> CloneContext<'a> {
    CloneContext {
        config,
        endpoint,
        credential,
        cancellation,
        deadline: Instant::now() + Duration::from_secs(30),
        path_policy: policy,
        git: runner,
    }
}

/// A runner that clones the fixture's local bare repository in place of
/// its GitHub URL.
fn fixture_runner(fixture: &Fixture) -> RewriteRunner {
    RewriteRunner::new(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
    )
}

/// Cache metadata for a shallow checkout of the fixture.
fn fixture_main_meta() -> cache::CacheMeta {
    cache::CacheMeta::new(
        &cache::Identity {
            owner: "fixture-owner",
            repo: "fixture-repo",
            commit: &"1".repeat(40),
            sparse_key: None,
            depth: 1,
        },
        &"1".repeat(40),
        Duration::from_secs(60),
        cache::CheckoutFacts::default(),
    )
}

/// A temp root and the existing clone-cache `home` inside it.
fn home_root() -> (test_support::Fixture, PathBuf) {
    let root = test_support::Fixture::new();
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    (root, cache_home)
}

fn query() -> GhCloneRepoQuery {
    parse_query(serde_json::json!({
        "owner": "fixture-owner", "repo": "fixture-repo", "ref": "main",
        "mainGoal": "test", "reasoning": "clone fixture"
    }))
}

/// One sparse path, as a single-string `path`.
fn one(path: &str) -> GhCloneRepoQueryPath {
    GhCloneRepoQueryPath::String(path.parse().expect("valid sparse fixture path"))
}

fn many(paths: &[&str]) -> GhCloneRepoQueryPath {
    GhCloneRepoQueryPath::Array(
        paths
            .iter()
            .map(|path| {
                (*path)
                    .parse::<GhCloneRepoQueryPathArrayItem>()
                    .expect("valid sparse fixture path")
            })
            .collect(),
    )
}

fn reference(value: &str) -> GhCloneRepoQueryRef {
    value.parse().expect("valid fixture branch or commit")
}

fn rejects_sparse_path(path: &str) -> bool {
    let mut row = serde_json::to_value(query()).expect("fixture query JSON");
    row["path"] = serde_json::json!(path);
    serde_json::from_value::<GhCloneRepoQuery>(row).is_err()
}

/// [`execute_clone`] of `query` pinned the way `run` pins it
/// ([`Fixture::pin`]).
fn clone_pinned(
    fixture: &Fixture,
    query: &GhCloneRepoQuery,
    context: &CloneContext<'_>,
) -> Result<CloneResult, CloneError> {
    execute_clone(&fixture.pin(query), context)
}

fn parse_query(value: serde_json::Value) -> GhCloneRepoQuery {
    serde_json::from_value(value).expect("valid ghCloneRepo query")
}

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

#[test]
fn creates_a_new_configured_home_before_authorizing_the_clone_target() {
    let fixture = Fixture::shared();
    let root = test_support::Fixture::new();
    let cache_home = root.0.join("new").join("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(fixture.work.clone()),
        additional_roots: vec![cache_home.clone()],
        ..Default::default()
    })
    .expect("policy");
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    assert!(!cache_home.exists());
    let result = clone_pinned(&fixture, &query(), &context).expect("new home clone");
    assert!(cache_home.is_dir());
    assert_eq!(result.location.commit_sha, fixture.first_commit);
    assert!(Path::new(&result.location.local_path).starts_with(cache_home));
}

/// The runtime allows only the checkout trees under the Octocode home
/// (`<home>/tmp/clone`), never the home itself (credentials, logs): a clone
/// must authorize its clones root, not the whole home.
#[test]
fn clones_with_only_the_clones_root_allowed_under_an_existing_home() {
    let fixture = Fixture::shared();
    let (_root, cache_home) = home_root();
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(fixture.work.clone()),
        additional_roots: vec![cache::clones_root(&cache_home)],
        ..Default::default()
    })
    .expect("policy");
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let result = clone_pinned(&fixture, &query(), &context).expect("clone under the clones root");
    assert_eq!(result.location.commit_sha, fixture.first_commit);
    assert!(
        Path::new(&result.location.local_path).starts_with(cache::clones_root(&cache_home)),
        "{}",
        result.location.local_path
    );
}

#[test]
fn denied_cache_home_is_not_created() {
    let root = test_support::Fixture::new();
    let outside = test_support::Fixture::new();
    let cache_home = outside.0.join("home");
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = UnreachableGit;
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let error = execute_clone(&query(), &context).expect_err("deny cache home");
    // The policy's own flat code, as the local tools report it.
    assert_eq!(error.code, ErrorCode::OutsideAllowedRoots);
    assert!(!cache_home.exists(), "denied home must not be created");
}

#[test]
fn policy_denials_use_the_codes_the_local_tools_share() {
    use crate::policy::PolicyErrorCode;
    for (code, expected) in [
        (PolicyErrorCode::IgnoredPath, ErrorCode::PathPolicyDenied),
        (PolicyErrorCode::NotFound, ErrorCode::PathNotFound),
        (PolicyErrorCode::InputTooLarge, ErrorCode::FileTooLarge),
        (
            PolicyErrorCode::OutsideAllowedRoots,
            ErrorCode::OutsideAllowedRoots,
        ),
        (
            PolicyErrorCode::SymlinkLoop,
            ErrorCode::PathValidationFailed,
        ),
    ] {
        let error = policy_denied(PolicyError::new(code, "denied"));
        assert_eq!(error.code, expected, "{code:?}");
        assert_eq!(error.message, "denied");
    }
}

#[test]
fn withheld_cache_home_is_denied_as_path_policy() {
    let root = test_support::Fixture::new();
    let cache_home = root.0.join(".ssh").join("home");
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = UnreachableGit;
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);

    let error = execute_clone(&query(), &context).expect_err("withheld cache home");
    assert_eq!(error.code, ErrorCode::PathPolicyDenied, "{error:?}");
    assert!(!cache_home.exists(), "a withheld home must not be created");
}

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
            path: Some(one("src")),
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
            path: Some(one("src")),
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
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
fn clone_eviction_preserves_dirty_expired_and_over_budget_checkouts() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&cache_home);
    config.max_clone_count = 1;
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    let checkout = PathBuf::from(fresh.location.local_path);
    fs::write(checkout.join(".git/info/exclude"), "ignored-evidence.txt\n").expect("exclude");
    let evidence = checkout.join("ignored-evidence.txt");
    fs::write(&evidence, "preserve through eviction\n").expect("owned evidence");
    // Every ref here names one commit: each history depth is its own entry.
    clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: Some(reference("v1")),
            history_depth: std::num::NonZeroU64::new(2).expect("depth"),
            ..query()
        },
        &context,
    )
    .expect("budget clone");
    assert_eq!(
        fs::read_to_string(&evidence).expect("budget preserved"),
        "preserve through eviction\n"
    );
    let meta_path = checkout.join(cache::META_FILE);
    let mut meta: serde_json::Value =
        serde_json::from_slice(&fs::read(&meta_path).expect("metadata")).expect("parse meta");
    meta["expiresAt"] = "1970-01-01T00:00:00.000Z".into();
    fs::write(
        &meta_path,
        serde_json::to_vec(&meta).expect("serialize meta"),
    )
    .expect("expire");
    clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            ref_: Some(reference(&fixture.first_commit)),
            history_depth: std::num::NonZeroU64::new(3).expect("depth"),
            ..query()
        },
        &context,
    )
    .expect("expiry cleanup clone");
    assert_eq!(
        fs::read_to_string(&evidence).expect("expired preserved"),
        "preserve through eviction\n"
    );
}

#[test]
fn metadata_write_preserves_repository_pid_temp_filename() {
    let stage = test_support::Fixture::new();
    let evidence = stage
        .0
        .join(format!("{}.tmp-{}", cache::META_FILE, std::process::id()));
    fs::write(&evidence, "repository temp-name evidence\n").expect("source evidence");
    let meta = fixture_main_meta();
    cache::write_meta(&stage.0, &meta).expect("new stage metadata");
    assert_eq!(
        fs::read_to_string(&evidence).expect("source filename still exists"),
        "repository temp-name evidence\n"
    );
    let written: cache::CacheMeta =
        serde_json::from_slice(&fs::read(stage.0.join(cache::META_FILE)).expect("metadata"))
            .expect("complete metadata");
    assert_eq!(written.commit_sha, meta.commit_sha);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(stage.0.join(cache::META_FILE))
                .expect("metadata permissions")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn metadata_write_rejects_existing_source_without_overwrite() {
    let stage = test_support::Fixture::new();
    let destination = stage.0.join(cache::META_FILE);
    fs::write(&destination, "source arrived after inspection\n").expect("source evidence");
    let meta = fixture_main_meta();
    let error = cache::write_meta(&stage.0, &meta).expect_err("exclusive metadata creation");
    assert_eq!(error.code, ErrorCode::CacheUnavailable);
    assert!(error.message.contains(cache::META_FILE));
    assert!(
        error
            .hints
            .iter()
            .any(|hint| hint.contains("ghGetFileContent"))
    );
    assert_eq!(
        fs::read_to_string(destination).expect("preserved source"),
        "source arrived after inspection\n"
    );
}

#[test]
fn repository_metadata_filename_conflict_never_replaces_an_existing_checkout() {
    struct CollisionRunner {
        inner: RewriteRunner,
        collide: AtomicBool,
        stages: Mutex<Vec<PathBuf>>,
    }
    impl GitRunner for CollisionRunner {
        fn run(
            &self,
            request: &GitRunRequest<'_>,
            control: &GitRunControl<'_>,
        ) -> Result<GitOutput, CloneError> {
            let output = self.inner.run(request, control)?;
            if request.label == "check out requested commit" && self.collide.load(Ordering::SeqCst)
            {
                let stage = request
                    .args
                    .iter()
                    .position(|argument| argument == "-C")
                    .and_then(|at| request.args.get(at + 1))
                    .map(PathBuf::from)
                    .expect("checkout stage argument");
                fs::write(
                    stage.join(cache::META_FILE),
                    "reserved repository evidence\n",
                )
                .expect("write owned source fixture");
                self.stages.lock().expect("stage lock").push(stage);
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
    let runner = CollisionRunner {
        inner: fixture_runner(&fixture),
        collide: AtomicBool::new(true),
        stages: Mutex::new(Vec::new()),
    };
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh_error =
        clone_pinned(&fixture, &query(), &context).expect_err("reject fresh reserved source");
    assert_eq!(fresh_error.code, ErrorCode::CacheUnavailable);
    assert!(fresh_error.message.contains(cache::META_FILE));
    assert!(fresh_error.message.contains(&fixture.first_commit));
    assert!(
        fresh_error
            .hints
            .iter()
            .any(|hint| hint.contains("ghGetFileContent") && hint.contains(cache::META_FILE))
    );
    assert!(
        runner
            .stages
            .lock()
            .expect("stage lock")
            .iter()
            .all(|stage| !stage.exists()),
        "failed tool-owned staging is cleaned up"
    );
    runner.collide.store(false, Ordering::SeqCst);
    let initial = clone_pinned(&fixture, &query(), &context).expect("clean initial checkout");
    let checkout = PathBuf::from(initial.location.local_path);
    let original_meta = fs::read(checkout.join(cache::META_FILE)).expect("original metadata");
    runner.collide.store(true, Ordering::SeqCst);
    let refresh_error = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            force_refresh: Some(true),
            ..query()
        },
        &context,
    )
    .expect_err("reject reserved source before refresh");
    assert_eq!(refresh_error.code, ErrorCode::CacheUnavailable);
    assert_eq!(
        fs::read(checkout.join(cache::META_FILE)).expect("unchanged metadata"),
        original_meta
    );
    assert_eq!(
        fs::read_to_string(checkout.join("README.md")).expect("unchanged source"),
        "fixture\n"
    );
    assert_eq!(
        git_output(&checkout, &["rev-parse", "HEAD"]).trim(),
        fixture.first_commit
    );
}

/// A runner whose `ls-tree` fails: the sparse scope cannot be resolved.
struct FailingLsTree(RewriteRunner);

impl GitRunner for FailingLsTree {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        if request.args.iter().any(|argument| argument == "ls-tree") {
            return Err(CloneError::new(ErrorCode::GitFailed, "ls-tree failed"));
        }
        self.0.run(request, control)
    }
    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError> {
        self.0.run_nul_records(request, control, record)
    }
}

/// A sparse path whose type `ls-tree` cannot resolve fails the clone with
/// that error instead of guessing cone mode, which would check out the
/// whole folder beside a requested file.
#[test]
fn an_unresolvable_sparse_path_type_fails_the_clone() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = FailingLsTree(fixture_runner(&fixture));
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let error = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            path: Some(one("README.md")),
            ..query()
        },
        &context,
    )
    .expect_err("unresolved sparse scope");
    assert_eq!(error.code, ErrorCode::GitFailed, "{error:?}");
    assert_eq!(error.message, "ls-tree failed");
}

/// Only the git steps that reach the remote run with the credential: the
/// local `remote add` and `config` steps take no askpass helper and no
/// network permit.
#[test]
fn only_network_git_steps_carry_the_credential() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            path: Some(one("README.md")),
            ..query()
        },
        &context,
    )
    .expect("sparse clone");
    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let step = |label: &str| {
        seen.iter()
            .find(|value| value.contains(label))
            .unwrap_or_else(|| panic!("{label} not run: {seen:?}"))
            .clone()
    };
    for local in [
        "configure commit remote",
        "configure sparse promisor",
        "configure sparse filter",
    ] {
        assert!(
            step(local).contains("authorization_url: None"),
            "{}",
            step(local)
        );
    }
    for network in ["fetch requested commit", "set sparse commit paths"] {
        assert!(
            step(network).contains("authorization_url: Some"),
            "{}",
            step(network)
        );
    }
}

#[test]
fn sparse_file_path_checks_out_only_that_file() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    for branch in [Some("main".to_owned()), Some(fixture.first_commit.clone())] {
        for (file, absent) in [
            ("README.md", ["src/lib.rs", "other/skip.txt"]),
            ("src/lib.rs", ["README.md", "src/nested/data.txt"]),
        ] {
            let clone = clone_pinned(
                &fixture,
                &GhCloneRepoQuery {
                    ref_: branch.as_deref().map(reference),
                    path: Some(one(file)),
                    ..query()
                },
                &context,
            )
            .expect("sparse file clone");
            let local = Path::new(&clone.location.local_path);
            assert!(local.join(file).is_file(), "{file} missing ({branch:?})");
            for sibling in absent {
                assert!(
                    !local.join(sibling).exists(),
                    "{sibling} checked out beside {file} ({branch:?})"
                );
            }
        }
    }
}

#[test]
fn corruption_expiry_stale_lock_and_failed_publication_preserve_cache() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(60);
    config.lock_wait = Duration::from_secs(1);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let first = clone_pinned(&fixture, &query(), &context).expect("first clone");
    let clone_path = PathBuf::from(&first.location.local_path);
    fs::write(clone_path.join(cache::META_FILE), "bad").expect("corrupt meta");
    assert!(
        !clone_pinned(&fixture, &query(), &context)
            .expect("rebuild corrupt")
            .location
            .cached
    );

    let mut expired: serde_json::Value = serde_json::from_slice(
        &fs::read(clone_path.join(cache::META_FILE)).expect("read metadata"),
    )
    .expect("parse metadata");
    expired["expiresAt"] = serde_json::Value::String("1970-01-01T00:00:00.000Z".into());
    fs::write(
        clone_path.join(cache::META_FILE),
        serde_json::to_vec(&expired).expect("encode metadata"),
    )
    .expect("expire metadata");
    assert!(
        !clone_pinned(&fixture, &query(), &context)
            .expect("rebuild expired")
            .location
            .cached
    );

    // A holder that ended without unlocking (a crash closes its descriptor)
    // leaves only the lock file: the kernel released the lock, so the next
    // clone takes it at once, with no age or pid heuristics.
    let lock = cache::lock_path(&cache_home, &clone_path);
    let crashed = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock)
        .expect("lock file");
    crashed.try_lock().expect("crashed holder takes the lock");
    drop(crashed);
    assert!(lock.is_file(), "the lock file stays behind");
    assert!(
        clone_pinned(&fixture, &query(), &context)
            .expect("a crashed holder's lock does not block")
            .location
            .cached
    );

    let destination = root.0.join("publication");
    fs::create_dir_all(&destination).expect("old destination");
    fs::write(destination.join("old"), "preserved").expect("old content");
    let missing_stage = root.0.join("missing-stage");
    assert!(cache::promote(&cache_home, &missing_stage, &destination).is_err());
    assert_eq!(
        fs::read_to_string(destination.join("old")).expect("restored"),
        "preserved"
    );
}

#[test]
fn cache_limits_live_lock_and_failed_sparse_refresh_are_bounded() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&cache_home);
    config.max_clone_count = 1;
    config.lock_wait = Duration::from_millis(120);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let main = clone_pinned(&fixture, &query(), &context).expect("main clone");
    let main_path = PathBuf::from(&main.location.local_path);
    // The tag names main's commit: a deeper history is another entry.
    let deep_tag = GhCloneRepoQuery {
        ref_: Some(reference("v1")),
        history_depth: std::num::NonZeroU64::new(2).expect("depth"),
        ..query()
    };
    let tag = clone_pinned(&fixture, &deep_tag, &context).expect("tag clone");
    assert!(!main_path.exists(), "oldest clone should be evicted");
    assert!(Path::new(&tag.location.local_path).is_dir());

    // The eviction must be attributable: one JSONL line in the trail.
    let (evictions, log_bytes) = crate::cache::evictions::recent_evictions(&cache_home, 10);
    let size_limit = evictions
        .iter()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|value| value["reason"] == "size-limit")
        .expect("size-limit eviction recorded in evictions.jsonl");
    assert_eq!(size_limit["path"], main.location.local_path.as_str());
    assert!(size_limit["bytes"].as_u64().is_some());
    assert_eq!(
        size_limit["pid"].as_u64(),
        Some(u64::from(std::process::id()))
    );
    assert!(log_bytes > 0);

    let live_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(cache::lock_path(
            &cache_home,
            Path::new(&tag.location.local_path),
        ))
        .expect("live lock file");
    let held = match octocode_io::lock::try_lock(live_lock).expect("lock") {
        octocode_io::lock::TryLock::Locked(held) => held,
        octocode_io::lock::TryLock::Busy(_) => panic!("the live lock is free"),
    };
    let error = clone_pinned(&fixture, &deep_tag, &context).expect_err("live lock must time out");
    assert_eq!(error.code, ErrorCode::Timeout);
    drop(held);

    let missing = clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            path: Some(one("missing/path")),
            ..query()
        },
        &context,
    )
    .expect_err("missing sparse path");
    assert_eq!(missing.code, ErrorCode::PathNotFound);
    assert!(missing.message.contains("\"missing/path\""), "{missing:?}");
    // The lead, not a hint or the message, names the recovery: nothing near
    // `path` exists at the root, so it lists the root at the commit.
    assert!(missing.hints.is_empty(), "{missing:?}");
    test_support::checks::assert_absent(&serde_json::json!(missing.message), "ghStructure");
    let next = missing.next.as_deref().expect("recovery lead");
    assert_eq!(next["viewTree"]["tool"], "ghStructure", "{next}");
    assert!(Path::new(&tag.location.local_path).is_dir());
}

#[test]
fn missing_branch_endpoint_git_and_portable_paths_fail_closed() {
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let config = CloneConfig::persistent(&cache_home);
    let missing_git = SystemGit::new(root.0.join("does-not-exist/git"));
    let context = setup(
        &missing_git,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );
    let pinned = GhCloneRepoQuery {
        ref_: Some(reference(&"a".repeat(40))),
        ..query()
    };
    let error = execute_clone(&pinned, &context).expect_err("missing git");
    assert_eq!(error.code, ErrorCode::GitUnavailable);

    // Plain http is a valid API base, and clone source, on loopback only.
    let loopback = GitHubEndpoint::new(url::Url::parse("http://127.0.0.1/api/v3").expect("URL"))
        .expect("provider endpoint");
    assert_eq!(
        repository_url(&web_root(&loopback).expect("root"), "owner", "repo")
            .expect("loopback clone URL"),
        "http://127.0.0.1/owner/repo.git"
    );
    assert!(
        GitHubEndpoint::new(url::Url::parse("http://ghe.example.com/api/v3").expect("URL"))
            .is_err(),
        "non-loopback http is rejected before any clone URL exists"
    );
    let encoded =
        GitHubEndpoint::new(url::Url::parse("https://example.test/prefix/api/v3").expect("URL"))
            .expect("GHES endpoint");
    assert_eq!(
        repository_url(
            &web_root(&encoded).expect("root"),
            "owner name",
            "repo#name"
        )
        .expect("encoded URL"),
        "https://example.test/prefix/owner%20name/repo%23name.git"
    );
    for sparse_path in [r"..\escape", r"src\portable"] {
        assert!(
            rejects_sparse_path(sparse_path),
            "contract must reject portable sparse path: {sparse_path}"
        );
    }
}

#[cfg(unix)]
struct FlagCancel(AtomicBool);
#[cfg(unix)]
impl CancellationCheck for FlagCancel {
    fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::SeqCst) {
            Err("cancelled by test".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(unix)]
#[test]
fn system_git_cancellation_kills_and_reaps_process_group() {
    use std::os::unix::fs::PermissionsExt;
    let root = test_support::Fixture::new();
    let script = root.0.join("git-fixture.sh");
    let pid_file = root.0.join("child.pid");
    fs::write(
        &script,
        "#!/bin/sh\nfor pid_file do :; done\nsleep 30 &\nchild=$!\nprintf '%s' \"$child\" > \"$pid_file\"\nwait \"$child\"\n",
    )
    .expect("script");
    let mut permissions = fs::metadata(&script).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script, permissions).expect("permissions");
    let cancellation = Arc::new(FlagCancel(AtomicBool::new(false)));
    let trigger = cancellation.clone();
    let trigger_pid_file = pid_file.clone();
    let deadline = Instant::now() + Duration::from_secs(10);
    let join = std::thread::spawn(move || {
        while Instant::now() < deadline {
            if let Some(pid) = fs::read_to_string(&trigger_pid_file)
                .ok()
                .and_then(|text| text.parse::<i32>().ok())
                .filter(|pid| *pid > 0)
            {
                let cancelled_at = Instant::now();
                trigger.0.store(true, Ordering::SeqCst);
                return Some((pid, cancelled_at));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        None
    });
    let runner = SystemGit::new(&script);
    let result = runner.run(
        &GitRunRequest {
            args: vec![pid_file.as_os_str().to_owned()],
            timeout: Duration::from_secs(10),
            label: "cancellation fixture".into(),
            authorization: None,
            authorization_url: None,
        },
        &GitRunControl {
            cancellation: cancellation.as_ref(),
            deadline,
            cache_home: &root.0,
        },
    );
    let (pid, cancelled_at) = join
        .join()
        .expect("trigger")
        .expect("fixture must publish its child pid within the unchanged execution deadline");
    assert_eq!(result.expect_err("cancel").code, ErrorCode::Cancelled);
    assert!(
        cancelled_at.elapsed() < Duration::from_secs(1),
        "ready process-group cancellation must finish promptly"
    );
    // A killed grandchild can remain briefly visible as a zombie until the OS
    // reaper runs. Poll only for that bounded reaping window; a live sleep would
    // remain for 30 seconds and fail this check.
    let child_gone = (0..100).any(|_| {
        // SAFETY: signal zero only checks whether the fixture child still exists.
        let gone = unsafe { libc::kill(pid, 0) } == -1;
        if !gone {
            std::thread::sleep(Duration::from_millis(10));
        }
        gone
    });
    assert!(
        child_gone,
        "fixture child survived process-group cancellation"
    );
}

#[cfg(unix)]
#[test]
fn system_git_timeout_and_failure_output_are_bounded_and_redacted() {
    let root = test_support::Fixture::new();
    // Installed Git avoids measuring cold loading of a freshly created
    // executable. Test-only aliases still exercise real subprocess stderr,
    // credential scrubbing, deadlines and process-group cleanup.
    let runner = SystemGit::default();
    let control = GitRunControl {
        cancellation: &NeverCancel,
        deadline: Instant::now() + Duration::from_secs(15),
        cache_home: &root.0,
    };
    let token = "highly-sensitive-token";
    let failed = runner
        .run(
            &GitRunRequest {
                args: vec![
                    "-c".into(),
                    "alias.octocode-fixture=!env >&2; printf 'arg:%s\\n' \"$@\" >&2; exit 1".into(),
                    "octocode-fixture".into(),
                ],
                timeout: Duration::from_secs(10),
                label: "redaction fixture".into(),
                authorization: Some(token),
                authorization_url: Some("https://github.com/owner/repo.git"),
            },
            &control,
        )
        .expect_err("failure fixture");
    assert_eq!(failed.code, ErrorCode::GitFailed);
    // The child's environment dump names the helper, never the credential:
    // nothing is left to redact.
    test_support::checks::assert_absent(&serde_json::json!(failed.message), token);
    assert!(
        failed.message.contains("GIT_ASKPASS="),
        "{}",
        failed.message
    );
    test_support::checks::assert_absent(&serde_json::json!(failed.message), "Authorization");

    // A git error names no checkout stage under the Octocode home.
    let stage = root
        .0
        .join("tmp")
        .join("clone-tmp")
        .join("stage-1")
        .join("repo");
    let staged = runner
        .run(
            &GitRunRequest {
                args: vec![
                    "-c".into(),
                    format!(
                        "alias.octocode-fixture=!printf \"fatal: could not create work tree dir '%s'\\n\" '{}' >&2; exit 128",
                        stage.display()
                    )
                    .into(),
                    "octocode-fixture".into(),
                ],
                timeout: Duration::from_secs(10),
                label: "stage fixture".into(),
                authorization: None,
                authorization_url: None,
            },
            &control,
        )
        .expect_err("stage fixture");
    test_support::checks::assert_absent(&serde_json::json!(staged.message), "clone-tmp");
    assert!(
        staged.message.contains("work tree dir"),
        "{}",
        staged.message
    );

    let started = Instant::now();
    let timed_out = runner
        .run(
            &GitRunRequest {
                args: vec![
                    "-c".into(),
                    "alias.octocode-fixture=!sleep 30".into(),
                    "octocode-fixture".into(),
                ],
                timeout: Duration::from_millis(60),
                label: "timeout fixture".into(),
                authorization: None,
                authorization_url: None,
            },
            &control,
        )
        .expect_err("timeout fixture");
    assert_eq!(timed_out.code, ErrorCode::Timeout);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn concurrent_requests_publish_once_and_validation_fails_closed() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = Arc::new(test_support::workspace_policy(&root.0));
    let endpoint = Arc::new(api_github_endpoint());
    let runner = Arc::new(fixture_runner(&fixture));
    let config = Arc::new(CloneConfig::persistent(&cache_home));
    let barrier = Arc::new(Barrier::new(2));
    let pinned = Arc::new(fixture.pin(&query()));
    let mut workers = vec![];
    for _ in 0..2 {
        let pinned = pinned.clone();
        let policy = policy.clone();
        let endpoint = endpoint.clone();
        let runner = runner.clone();
        let config = config.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            execute_clone(
                &pinned,
                &CloneContext {
                    config: &config,
                    endpoint: &endpoint,
                    credential: None,
                    cancellation: &NeverCancel,
                    deadline: Instant::now() + Duration::from_secs(30),
                    path_policy: &policy,
                    git: runner.as_ref(),
                },
            )
            .expect("race clone")
        }));
    }
    let values = workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .collect::<Vec<_>>();
    assert_eq!(
        values.iter().filter(|value| value.location.cached).count(),
        1
    );
    assert_eq!(values[0].location.commit_sha, values[1].location.commit_sha);

    // A traversal owner never reaches the clone: the contract rejects it.
    assert!(
        "../escape"
            .parse::<octocode_contract_types::GhCloneRepoQueryOwner>()
            .is_err(),
        "the contract rejects a traversal owner"
    );
    assert!(
        rejects_sparse_path("../escape"),
        "contract must reject sparse parent traversal before clone"
    );
}

#[cfg(unix)]
#[test]
fn repository_symlinks_are_checked_out_as_plain_files() {
    // A repo symlink pointing at another ~/.octocode file would otherwise be
    // readable through localFetch inside the clone root.
    let fixture = Fixture::new();
    std::os::unix::fs::symlink("../../secret.json", fixture.work.join("escape")).expect("symlink");
    git(&fixture.work, &["add", "escape"]);
    git(&fixture.work, &["commit", "-m", "symlink"]);
    git(&fixture.work, &["push", "origin", "main"]);
    let head = git_output(&fixture.work, &["rev-parse", "HEAD"])
        .trim()
        .to_owned();

    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(60);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    // The same commit by branch then by SHA: the second is a cache hit, which
    // reports the symlinks its meta recorded. A directory scope also holds
    // the root's files (the link among them); a file scope holds only it.
    for (branch, sparse, cached, listed) in [
        ("main", None, false, true),
        (head.as_str(), None, true, true),
        ("main", Some("src"), false, true),
        ("main", Some("src/lib.rs"), false, false),
    ] {
        let clone = clone_pinned(
            &fixture,
            &GhCloneRepoQuery {
                ref_: Some(reference(branch)),
                path: sparse.map(one),
                ..query()
            },
            &context,
        )
        .expect("clone");
        assert_eq!(clone.location.cached, cached, "{branch} {sparse:?}");
        let link = Path::new(&clone.location.local_path).join("escape");
        if listed {
            let meta = fs::symlink_metadata(&link).expect("escape entry");
            assert!(
                !meta.file_type().is_symlink(),
                "{branch}: symlink materialized"
            );
            let row = serde_json::to_value(&clone.location).expect("location");
            assert_eq!(row["symlinks"], serde_json::json!(["escape"]), "{row}");
        } else {
            assert!(!link.exists(), "{branch} {sparse:?}");
            assert_eq!(clone.location.symlinks, None, "{branch} {sparse:?}");
        }
    }
    // An entry whose meta lacks a recorded fact (an older layout) is no
    // cache hit: it is cloned again, and the new meta records every fact.
    for field in ["symlinks", "checkoutBytes", "sizeBytes"] {
        let full = clone_pinned(&fixture, &query(), &context).expect("cached clone");
        assert!(full.location.cached, "{field}");
        let meta_path = Path::new(&full.location.local_path).join(cache::META_FILE);
        let mut meta: serde_json::Value =
            serde_json::from_slice(&fs::read(&meta_path).expect("meta")).expect("meta JSON");
        assert_eq!(meta["symlinks"], serde_json::json!(["escape"]));
        meta.as_object_mut().expect("meta object").remove(field);
        fs::write(&meta_path, serde_json::to_vec(&meta).expect("meta bytes")).expect("old meta");
        let old = clone_pinned(&fixture, &query(), &context).expect("old entry re-cloned");
        assert!(
            !old.location.cached,
            "{field}: an incomplete meta was served"
        );
        assert_eq!(old.location.symlinks, Some(vec!["escape".to_owned()]));
        let meta: serde_json::Value =
            serde_json::from_slice(&fs::read(&meta_path).expect("meta")).expect("meta JSON");
        assert!(meta.get(field).is_some(), "{field} recorded again: {meta}");
    }
    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for step in [
        "initialize commit checkout",
        "fetch requested commit",
        "check out requested commit",
        "set sparse commit paths",
    ] {
        let request = seen
            .iter()
            .find(|value| value.contains(step))
            .unwrap_or_else(|| panic!("{step} not run: {seen:?}"));
        assert!(
            request.contains("core.symlinks=false"),
            "{step} lacks core.symlinks=false: {request}"
        );
    }
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

/// A fixture context whose caller did not resolve a default branch, as the
/// runtime now calls it (no GitHub API before cloning).
struct Bench {
    fixture: Arc<Fixture>,
    /// Holds the temp directory for the bench's lifetime.
    _root: test_support::Fixture,
    policy: PathPolicy,
    endpoint: GitHubEndpoint,
    runner: RewriteRunner,
    config: CloneConfig,
}

impl Bench {
    fn new() -> Self {
        Self::at(api_github_endpoint())
    }

    /// A bench whose GitHub API is `endpoint`; git clones the shared
    /// fixture in place of the repository URL that endpoint derives.
    fn at(endpoint: GitHubEndpoint) -> Self {
        Self::over(endpoint, Fixture::shared())
    }

    /// [`Self::at`] over its own fixture, for a test that commits to it.
    fn committing(endpoint: GitHubEndpoint) -> Self {
        Self::over(endpoint, Arc::new(Fixture::new()))
    }

    fn over(endpoint: GitHubEndpoint, fixture: Arc<Fixture>) -> Self {
        let (root, cache_home) = home_root();
        let policy = test_support::workspace_policy(&root.0);
        let url = repository_url(
            &web_root(&endpoint).expect("root"),
            "fixture-owner",
            "fixture-repo",
        )
        .expect("clone URL");
        let runner = RewriteRunner::new(&url, &fixture.bare_url);
        let mut config = CloneConfig::persistent(&cache_home);
        config.cache_ttl = Duration::from_secs(60);
        Self {
            fixture,
            _root: root,
            policy,
            endpoint,
            runner,
            config,
        }
    }

    fn context(&self) -> CloneContext<'_> {
        self.context_as(None)
    }

    /// [`Self::context`] for the caller holding `credential`.
    fn context_as<'a>(&'a self, credential: Option<&'a ResolvedCredential>) -> CloneContext<'a> {
        CloneContext {
            ..setup(
                &self.runner,
                &self.config,
                &self.endpoint,
                &self.policy,
                &NeverCancel,
                credential,
            )
        }
    }

    fn clones_seen(&self) -> usize {
        self.runner
            .seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .filter(|call| call.contains("\"clone\"") || call.contains("\"fetch\""))
            .count()
    }
}

fn unbranched() -> GhCloneRepoQuery {
    GhCloneRepoQuery {
        ref_: None,
        ..query()
    }
}

/// Several sparse paths check out together; a file among them switches
/// to exact non-cone patterns so nothing else is included.
#[test]
fn multiple_sparse_paths_check_out_every_path() {
    let bench = Bench::new();
    let context = bench.context();
    let both = clone_pinned(
        &bench.fixture,
        &GhCloneRepoQuery {
            path: Some(many(&["src/nested", "other"])),
            ..query()
        },
        &context,
    )
    .expect("multi-directory sparse clone");
    let local = Path::new(&both.location.local_path);
    assert!(local.join("src/nested/data.txt").is_file());
    assert!(local.join("other/skip.txt").is_file());
    assert_eq!(
        both.location.requested_paths.as_deref(),
        Some(&["src/nested".to_owned(), "other".to_owned()][..])
    );
    let row = serde_json::to_value(&both).expect("serialize");
    assert_eq!(
        Path::new(
            row["next"]["exploreClone"]["query"]["queries"][0]["path"]
                .as_str()
                .expect("path")
        ),
        local,
        "several paths explore the checkout root"
    );

    let reordered = clone_pinned(
        &bench.fixture,
        &GhCloneRepoQuery {
            path: Some(many(&["other", "src/nested"])),
            ..query()
        },
        &context,
    )
    .expect("same set, other order");
    assert!(
        reordered.location.cached,
        "request order does not split the cache"
    );

    let mixed = clone_pinned(
        &bench.fixture,
        &GhCloneRepoQuery {
            path: Some(many(&["README.md", "src/nested"])),
            ..query()
        },
        &context,
    )
    .expect("file plus directory sparse clone");
    let local = Path::new(&mixed.location.local_path);
    assert!(local.join("README.md").is_file());
    assert!(local.join("src/nested/data.txt").is_file());
    assert!(!local.join("src/lib.rs").exists());
    assert!(!local.join("other/skip.txt").exists());

    let missing = clone_pinned(
        &bench.fixture,
        &GhCloneRepoQuery {
            path: Some(many(&["src", "nope"])),
            ..query()
        },
        &context,
    )
    .expect_err("one missing path fails the clone");
    assert_eq!(missing.code, ErrorCode::PathNotFound);
    assert!(missing.message.contains("\"nope\""), "{missing:?}");
    test_support::checks::assert_absent(&serde_json::json!(missing.message), "\"src\"");
}

/// A missing sparse path is recovered from the commit's own trees: a unique
/// near-miss sibling at the deepest existing directory gives the same clone
/// with the path corrected; otherwise that directory is listed, pinned to the
/// commit.
#[test]
fn a_missing_sparse_path_leads_to_its_near_miss_or_its_nearest_directory() {
    let bench = Bench::new();
    let context = bench.context();
    let commit = bench.fixture.first_commit.clone();
    let failed = |path: GhCloneRepoQueryPath| {
        let missing = clone_pinned(
            &bench.fixture,
            &GhCloneRepoQuery {
                path: Some(path),
                ..query()
            },
            &context,
        )
        .expect_err("missing sparse path");
        assert_eq!(missing.code, ErrorCode::PathNotFound, "{missing:?}");
        assert!(missing.hints.is_empty(), "{missing:?}");
        *missing.next.expect("recovery lead")
    };

    // One near miss in the deepest existing directory: the clone, corrected.
    let next = failed(one("src/nested/dta.txt"));
    let lead = &next["repair"];
    assert_eq!(lead["tool"], "ghCloneRepo", "{next}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["path"], "src/nested/data.txt", "{next}");
    assert_eq!(row["ref"], commit.as_str(), "{next}");
    assert_eq!(row["owner"], "fixture-owner", "{next}");

    // Only the letter case differs: missing on every filesystem (a commit's
    // paths are case-sensitive), corrected to the commit's spelling.
    let next = failed(one("src/Nested"));
    let row = &next["repair"]["query"]["queries"][0];
    assert_eq!(row["path"], "src/nested", "{next}");

    // A directory is a near miss too, and a list is corrected in place.
    let next = failed(many(&["other", "sr/nested"]));
    let row = &next["repair"]["query"]["queries"][0];
    assert_eq!(
        row["path"],
        serde_json::json!(["other", "src/nested"]),
        "{next}"
    );

    // Nothing near: the deepest directory that exists, at the commit.
    let next = failed(one("src/nested/deep/er/x.rs"));
    let lead = &next["viewTree"];
    assert_eq!(lead["tool"], "ghStructure", "{next}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["path"], "src/nested", "{next}");
    assert_eq!(row["ref"], commit.as_str(), "{next}");
    assert!(next.get("repair").is_none(), "{next}");

    // At the root, the listing names no path.
    let next = failed(one("zzzzzz/a"));
    let row = &next["viewTree"]["query"]["queries"][0];
    assert!(row.get("path").is_none(), "{next}");
    assert_eq!(row["ref"], commit.as_str(), "{next}");

    // One uncorrectable path among several keeps the listing, not a clone.
    let next = failed(many(&["src/nested/dta.txt", "src/qqqqqq/z"]));
    assert_eq!(
        next["viewTree"]["query"]["queries"][0]["path"], "src",
        "{next}"
    );
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

/// The pinning `commits/{ref}` answer refuses a row before git runs: a
/// missing ref or repository is not-found input (exit 3) naming what is
/// missing, a rejected credential keeps its provider error, and any other
/// failure (an outage) is returned unclassified for the provider error.
#[test]
fn api_refusals_name_what_is_missing() {
    use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason};
    let tagged = parse_query(serde_json::json!({
        "owner": "o", "repo": "r", "ref": "nope", "mainGoal": "test", "reasoning": "r"
    }));
    let api = |kind, status: u16, message: &str| ProviderError {
        status: Some(status),
        ..ProviderError::new(kind, message.to_owned())
    };
    let missing_ref = api(
        ProviderErrorKind::Validation,
        422,
        "No commit found for SHA: nope",
    )
    .with_reason(ProviderErrorReason::RefNotFound);
    match api_refusal(&tagged, "nope", missing_ref) {
        Ok(CloneFailure::Clone(error)) => {
            assert_eq!(error.code, ErrorCode::NotFound, "{error:?}");
            assert!(error.message.contains("\"nope\""), "{error:?}");
            assert!(error.hints[0].contains("ghStructure"), "{error:?}");
            let refs = &error.next.as_ref().expect("refs lead")["viewRefs"];
            assert_eq!(refs["tool"], "ghStructure", "{error:?}");
            assert_eq!(
                refs["query"]["queries"][0],
                serde_json::json!({"owner": "o", "repo": "r", "operation": "refs"}),
                "{error:?}"
            );
        }
        other => panic!("a missing ref is input: {other:?}"),
    }
    let missing_repo = api(ProviderErrorKind::NotFound, 404, "Not Found")
        .with_reason(ProviderErrorReason::RepositoryNotFound);
    match api_refusal(&tagged, "nope", missing_repo) {
        Ok(CloneFailure::Clone(error)) => {
            assert_eq!(error.code, ErrorCode::NotFound);
            assert_eq!(error.message, "Repository not found: o/r");
            assert_eq!(
                error.next.as_ref().expect("search lead")["findRepository"]["tool"],
                "ghSearchRepo",
                "{error:?}"
            );
        }
        other => panic!("a missing repository is input: {other:?}"),
    }
    for kind in [
        ProviderErrorKind::Authentication,
        ProviderErrorKind::Permission,
    ] {
        assert!(matches!(
            api_refusal(&tagged, "nope", api(kind, 401, "denied")),
            Ok(CloneFailure::Provider(_))
        ));
    }
    assert!(
        api_refusal(
            &tagged,
            "nope",
            api(ProviderErrorKind::Server, 503, "unavailable")
        )
        .is_err()
    );
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

/// The native sparse-path rule holds on its own, whatever the contract
/// pattern admits: no root, drive, backslash, `.`/`..` segment or blank.
#[test]
fn sparse_path_validation_rejects_traversal_absolute_and_backslash() {
    for path in [
        "../escape",
        "a/../b",
        "a/..",
        "./a",
        "a/./b",
        ".",
        "..",
        "/abs",
        "/",
        "C:/x",
        "c:x",
        "a\\b",
        "\\\\server\\share",
        "",
        "  ",
    ] {
        assert!(!is_sparse_path(path), "{path:?} must be rejected");
    }
    for path in [
        "src",
        "a/b",
        "src/",
        ".github/workflows",
        "a..b",
        "...",
        "docs/C:x",
    ] {
        assert!(is_sparse_path(path), "{path:?} must be accepted");
        // The contract's lookaround-free `path` pattern admits every path
        // the native rule accepts: no valid path is refused at the edge.
        assert!(
            !rejects_sparse_path(path),
            "{path:?} rejected by the contract"
        );
    }
    // The pattern alone already refuses traversal, roots and backslashes.
    for path in ["../escape", "a/../b", "./a", ".", "..", "/abs", "a\\b"] {
        assert!(
            rejects_sparse_path(path),
            "{path:?} admitted by the contract"
        );
    }
}

/// Callers of one runtime that share its clone cache: each holds its own
/// token, as the API and git see it.
const ALICE: &str = "clone-token-alice";
const BOB: &str = "clone-token-bob";

fn credential(token: &str) -> ResolvedCredential {
    ResolvedCredential::new(token, CredentialSource::Override)
}

/// One ghCloneRepo row run as the holder of `token`, through the runtime's
/// entry point (`run`), with that token for both the API and git.
async fn clone_as(
    bench: &Bench,
    provider: &GitHubProvider,
    query: &GhCloneRepoQuery,
    token: &str,
) -> Result<CloneResult, CloneFailure> {
    let credential = credential(token);
    let request = RequestContext::new(
        octocode_io::budget::RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
        Some(credential.clone()),
    );
    run(
        provider,
        query,
        Ok(&request),
        &bench.context_as(Some(&credential)),
    )
    .await
}

/// `commits/{reference}` of the fixture repository answers `token` with
/// `response`.
async fn answer_commit(
    server: &wiremock::MockServer,
    reference: &str,
    token: &str,
    response: wiremock::ResponseTemplate,
) {
    use wiremock::matchers::{header, method, path};
    wiremock::Mock::given(method("GET"))
        .and(path(format!(
            "/api/v3/repos/fixture-owner/fixture-repo/commits/{reference}"
        )))
        .and(header("authorization", format!("Bearer {token}").as_str()))
        .respond_with(response)
        .mount(server)
        .await;
}

fn not_found() -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(404).set_body_json(serde_json::json!({"message": "Not Found"}))
}

/// The API paths `server` was asked for, in order.
async fn api_calls(server: &wiremock::MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect()
}

fn commit(sha: &str) -> wiremock::ResponseTemplate {
    wiremock::ResponseTemplate::new(200).set_body_string(sha)
}

const COMMITS_MAIN: &str = "/api/v3/repos/fixture-owner/fixture-repo/commits/main";

/// A checkout Alice's token cloned is never handed to Bob's token when
/// GitHub denies Bob the repository: Bob gets the not-found a fresh clone
/// would give him, no checkout path, and Alice's checkout stays.
#[tokio::test]
async fn cache_hit_is_not_served_to_a_caller_without_access() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    answer_commit(&server, "main", ALICE, commit(&bench.fixture.first_commit)).await;
    answer_commit(&server, "main", BOB, not_found()).await;

    let alice = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("Alice clones"));
    assert!(!alice.location.cached);
    assert_eq!(alice.location.resolved_ref, "main");
    let clones = bench.clones_seen();

    match clone_as(&bench, &provider, &query(), BOB).await {
        Ok(served) => panic!("Bob was served Alice's checkout: {served:?}"),
        Err(CloneFailure::Clone(error)) => {
            assert_eq!(error.code, ErrorCode::NotFound, "{error:?}");
            assert_eq!(
                error.message,
                "Repository not found: fixture-owner/fixture-repo"
            );
            test_support::checks::assert_absent(
                &serde_json::json!(error),
                &alice.location.local_path,
            );
        }
        Err(CloneFailure::Provider(error)) => panic!("a denied repository is notFound: {error:?}"),
    }
    assert_eq!(bench.clones_seen(), clones, "Bob ran no git");
    assert!(
        Path::new(&alice.location.local_path)
            .join("README.md")
            .is_file(),
        "Alice's checkout is kept"
    );
    // Each caller pins the commit once, with their own token.
    assert_eq!(api_calls(&server).await, [COMMITS_MAIN, COMMITS_MAIN]);
}

/// A caller GitHub grants the repository is served the hit after one
/// `commits/{ref}` read with their own token, with no re-clone.
#[tokio::test]
async fn cache_hit_is_served_after_the_callers_access_check() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    let sha = bench.fixture.first_commit.clone();
    answer_commit(&server, "main", ALICE, commit(&sha)).await;
    answer_commit(&server, "main", BOB, commit(&sha)).await;

    let alice = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("Alice clones"));
    let clones = bench.clones_seen();
    let bob = clone_as(&bench, &provider, &query(), BOB)
        .await
        .unwrap_or_else(|_| panic!("Bob may read the repository"));
    assert!(bob.location.cached, "{bob:?}");
    assert_eq!(bob.location.local_path, alice.location.local_path);
    assert_eq!(bob.location.commit_sha, sha);
    assert_eq!(
        bench.clones_seen(),
        clones,
        "an allowed hit is not re-cloned"
    );
    assert_eq!(api_calls(&server).await, [COMMITS_MAIN, COMMITS_MAIN]);
}

/// A branch that moved since its checkout is checked out again at its new
/// commit, never served stale; the same commit named by its SHA shares the
/// branch's checkout.
#[tokio::test]
async fn checkouts_are_keyed_by_the_commit_the_ref_names_now() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::committing(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    let first = bench.fixture.first_commit.clone();
    answer_commit(&server, "main", ALICE, commit(&first)).await;
    answer_commit(&server, &first, ALICE, commit(&first)).await;
    let before = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("first clone"));
    let by_sha = clone_as(
        &bench,
        &provider,
        &GhCloneRepoQuery {
            ref_: Some(reference(&first)),
            ..query()
        },
        ALICE,
    )
    .await
    .unwrap_or_else(|_| panic!("clone by SHA"));
    assert!(by_sha.location.cached, "{by_sha:?}");
    assert_eq!(by_sha.location.local_path, before.location.local_path);
    assert_eq!(by_sha.location.resolved_ref, first);

    let second = bench.fixture.push_second();
    server.reset().await;
    answer_commit(&server, "main", ALICE, commit(&second)).await;
    let after = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("moved branch"));
    assert!(
        !after.location.cached,
        "a moved branch is re-cloned: {after:?}"
    );
    assert_eq!(after.location.commit_sha, second);
    assert_ne!(after.location.local_path, before.location.local_path);
}

/// An unbranched request pins the default branch the repository names, and
/// a renamed repository is checked out under its canonical name: the old
/// name's request follows GitHub's redirect.
#[tokio::test]
async fn the_default_branch_and_canonical_name_pin_the_checkout() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    let redirect = |to: &str| {
        wiremock::ResponseTemplate::new(301).insert_header(
            "location",
            format!("{}/api/v3/repositories/1{to}", server.uri()),
        )
    };
    for (from, to) in [("", ""), ("/commits/HEAD", "/commits/HEAD")] {
        wiremock::Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/old-owner/fixture-repo{from}")))
            .respond_with(redirect(to))
            .mount(&server)
            .await;
    }
    wiremock::Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"default_branch":"main","full_name":"fixture-owner/fixture-repo"}),
        ))
        .mount(&server)
        .await;
    wiremock::Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/HEAD"))
        .respond_with(commit(&bench.fixture.first_commit))
        .mount(&server)
        .await;
    let renamed = GhCloneRepoQuery {
        owner: "old-owner".parse().expect("owner"),
        ..unbranched()
    };
    let cloned = clone_as(&bench, &provider, &renamed, ALICE)
        .await
        .unwrap_or_else(|failure| panic!("renamed clone: {failure:?}"));
    assert_eq!(cloned.location.resolved_ref, "main");
    assert_eq!(cloned.location.commit_sha, bench.fixture.first_commit);
    assert!(
        cloned.location.local_path.contains("fixture-owner"),
        "{cloned:?}"
    );
    assert_eq!(
        cloned.warnings,
        [
            "Repository old-owner/fixture-repo was renamed to fixture-owner/fixture-repo; leads use the canonical name."
        ],
        "the rename is stated, as the other GitHub tools state it"
    );
}

/// GitHub owner and repository names are case-insensitive: a request that
/// spells them in another case shares the checkout of the same commit.
#[tokio::test]
async fn owner_and_repo_case_variants_share_one_checkout() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    let first = bench.fixture.first_commit.clone();
    answer_commit(&server, "main", ALICE, commit(&first)).await;
    wiremock::Mock::given(method("GET"))
        .and(path(
            "/api/v3/repos/Fixture-Owner/FIXTURE-REPO/commits/main",
        ))
        .respond_with(commit(&first))
        .mount(&server)
        .await;
    let lower = clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|failure| panic!("first clone: {failure:?}"));
    let upper = clone_as(
        &bench,
        &provider,
        &GhCloneRepoQuery {
            owner: "Fixture-Owner".parse().expect("owner"),
            repo: "FIXTURE-REPO".parse().expect("repo"),
            ..query()
        },
        ALICE,
    )
    .await
    .unwrap_or_else(|failure| panic!("case variant: {failure:?}"));
    assert!(upper.location.cached, "{upper:?}");
    assert_eq!(upper.location.local_path, lower.location.local_path);
    assert!(upper.warnings.is_empty(), "{upper:?}");
}

/// An unbranched request checks the caller's access to the default branch
/// it would serve.
#[tokio::test]
async fn the_default_branch_requires_access() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    wiremock::Mock::given(method("GET"))
        .and(path("/api/v3/repos/fixture-owner/fixture-repo"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"default_branch":"main","full_name":"fixture-owner/fixture-repo"}),
        ))
        .mount(&server)
        .await;
    answer_commit(&server, "HEAD", ALICE, commit(&bench.fixture.first_commit)).await;
    answer_commit(&server, "HEAD", BOB, not_found()).await;

    let alice = clone_as(&bench, &provider, &unbranched(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("Alice clones the default branch"));
    assert_eq!(alice.location.resolved_ref, "main");
    match clone_as(&bench, &provider, &unbranched(), BOB).await {
        Ok(served) => panic!("Bob was served Alice's checkout: {served:?}"),
        Err(CloneFailure::Clone(error)) => assert_eq!(error.code, ErrorCode::NotFound, "{error:?}"),
        Err(CloneFailure::Provider(error)) => panic!("a denied repository is notFound: {error:?}"),
    }
}

/// A check that cannot pass refuses the hit: a caller's spent rate limit
/// never unlocks another caller's checkout.
#[tokio::test]
async fn access_check_failure_fails_closed() {
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::at(gh_shared_support::mock_endpoint(&server));
    let provider = gh_shared_support::one_shot_provider(&server);
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
        + 3600;
    answer_commit(&server, "main", ALICE, commit(&bench.fixture.first_commit)).await;
    answer_commit(
        &server,
        "main",
        BOB,
        wiremock::ResponseTemplate::new(403)
            .insert_header("x-ratelimit-remaining", "0")
            .insert_header("x-ratelimit-reset", reset.to_string().as_str())
            .insert_header("x-ratelimit-resource", "core")
            .set_body_json(serde_json::json!({"message": "API rate limit exceeded"})),
    )
    .await;

    clone_as(&bench, &provider, &query(), ALICE)
        .await
        .unwrap_or_else(|_| panic!("Alice clones"));
    match clone_as(&bench, &provider, &query(), BOB).await {
        Ok(served) => panic!("a rate-limited check served the hit: {served:?}"),
        Err(CloneFailure::Provider(error)) => {
            assert_eq!(error.kind, octocode_github::ProviderErrorKind::RateLimited);
        }
        Err(CloneFailure::Clone(error)) => {
            panic!("a rate limit keeps its provider error: {error:?}")
        }
    }
}

/// An API that never answers refuses the row before git runs.
#[tokio::test]
async fn an_unreachable_api_refuses_the_row_before_git() {
    // An API endpoint nothing listens on: the port is released before use.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free port")
        .port();
    let endpoint = GitHubEndpoint::new(
        url::Url::parse(&format!("http://127.0.0.1:{port}/api/v3")).expect("URL"),
    )
    .expect("endpoint");
    let bench = Bench::at(endpoint.clone());
    let provider = gh_shared_support::one_shot_provider_at(endpoint);
    match clone_as(&bench, &provider, &query(), ALICE).await {
        Ok(served) => panic!("an unreachable API pinned a clone: {served:?}"),
        Err(CloneFailure::Provider(error)) => assert!(
            matches!(
                error.kind,
                octocode_github::ProviderErrorKind::Transport
                    | octocode_github::ProviderErrorKind::Timeout
            ),
            "{error:?}"
        ),
        Err(CloneFailure::Clone(error)) => {
            panic!("a network failure is a provider error: {error:?}")
        }
    }
    assert_eq!(bench.clones_seen(), 0, "no git ran");
}

/// A published entry records its whole size (checkout and `.git`), which
/// eviction weighs it by instead of walking it; a file beside the entries
/// (a retired layout's default-branch memo) is swept by the next eviction
/// pass; rows for two entries run at once and both publish.
#[test]
fn entries_record_their_size_orphans_are_swept_and_distinct_rows_run_at_once() {
    let fixture = Fixture::shared();
    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&cache_home);
    let repo_dir = cache::clones_root(&cache_home)
        .join("fixture-owner")
        .join("fixture-repo");
    fs::create_dir_all(&repo_dir).expect("repo dir");
    let orphan = repo_dir.join(".default-branch__host_0123456789abcdef.json");
    fs::write(&orphan, "{}").expect("orphan");
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let fresh = clone_pinned(&fixture, &query(), &context).expect("fresh clone");
    let local = Path::new(&fresh.location.local_path);
    let meta: cache::CacheMeta =
        serde_json::from_slice(&fs::read(local.join(cache::META_FILE)).expect("meta"))
            .expect("meta json");
    let checkout = meta.checkout_bytes;
    assert_eq!(checkout, fresh.bytes);
    assert!(
        meta.size_bytes > checkout,
        "the entry's size counts .git too: {meta:?}"
    );
    assert!(!orphan.exists(), "an orphaned file is swept");
    assert!(repo_dir.is_dir(), "entry directories stay");

    let rows = std::thread::scope(|scope| {
        let workers = ["README.md", "src/lib.rs"].map(|path| {
            let (fixture, runner, config) = (&fixture, &runner, &config);
            let (endpoint, policy) = (&endpoint, &policy);
            scope.spawn(move || {
                let context = setup(runner, config, endpoint, policy, &NeverCancel, None);
                clone_pinned(
                    fixture,
                    &GhCloneRepoQuery {
                        path: Some(one(path)),
                        ..query()
                    },
                    &context,
                )
            })
        });
        workers.map(|worker| worker.join().expect("worker"))
    });
    for row in rows {
        let row = row.expect("concurrent distinct clone");
        assert!(!row.location.cached);
        assert!(Path::new(&row.location.local_path).is_dir());
    }
}

#[test]
fn large_indexes_clone_whole_repositories_and_sparse_trees_without_exposing_symlinks() {
    use std::io::Write;
    use std::process::Stdio;

    let fixture = Fixture::new();
    let blob = git_output(&fixture.work, &["rev-parse", "HEAD:README.md"]);
    let target = "../../secret.json";
    let target_file = fixture.work.join("link-target.txt");
    fs::write(&target_file, target).expect("link target bytes");
    let link_blob = git_output(&fixture.work, &["hash-object", "-w", "link-target.txt"]);
    fs::remove_file(target_file).expect("remove unstaged helper");
    // Git output is capped at 64 KiB here (5 MiB in production), so about
    // 800 files carry the index past the cap.
    let cap = 64 * 1024;
    let mut entries = Vec::new();
    let mut files = 0;
    while entries.len() <= cap {
        write!(
            &mut entries,
            "100644 {}\tbulk/file-{files:06}-with spaces.txt\0",
            blob.trim()
        )
        .expect("index record");
        files += 1;
    }
    write!(&mut entries, "120000 {}\tescape\0", link_blob.trim()).expect("symlink index record");
    let mut child = fixture_git_command(&fixture.work, &["update-index", "-z", "--index-info"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("index writer");
    child
        .stdin
        .take()
        .expect("index stdin")
        .write_all(&entries)
        .expect("write index records");
    let indexed = child.wait_with_output().expect("index writer exit");
    assert!(
        indexed.status.success(),
        "{}",
        String::from_utf8_lossy(&indexed.stderr)
    );
    git(&fixture.work, &["commit", "-m", "large generated index"]);
    git(&fixture.work, &["push", "origin", "main"]);
    let index_bytes = fixture_git(&fixture.work, &["ls-files", "--stage", "-z"])
        .stdout
        .len();
    assert!(
        index_bytes > cap,
        "fixture must exceed collected output cap"
    );

    let (root, cache_home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = RewriteRunner::with_git(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
        SystemGit::default().with_max_output(cap),
    );
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(120);
    let mut context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    context.deadline = Instant::now() + Duration::from_secs(120);
    for scope in [None, Some("README.md"), Some("src")] {
        let query = GhCloneRepoQuery {
            path: scope.map(one),
            ..query()
        };
        let cloned = clone_pinned(&fixture, &query, &context).expect("large-index clone");
        let checkout = Path::new(&cloned.location.local_path);
        if scope.is_none() {
            assert_eq!(
                fs::read_dir(checkout.join("bulk"))
                    .expect("bulk checkout")
                    .count(),
                files
            );
        } else {
            assert!(
                !checkout.join("bulk").exists(),
                "sparse checkout must not widen"
            );
        }
        let link = checkout.join("escape");
        if scope == Some("README.md") {
            assert!(!link.exists());
            assert_eq!(cloned.location.symlinks, None);
        } else {
            assert!(
                !fs::symlink_metadata(&link)
                    .expect("target-text file")
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read_to_string(&link).expect("target text"), target);
            assert_eq!(cloned.location.symlinks, Some(vec!["escape".into()]));
        }
        let cached = clone_pinned(&fixture, &query, &context).expect("large-index cache hit");
        assert!(cached.location.cached);
        assert_eq!(cached.location.symlinks, cloned.location.symlinks);
    }
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
