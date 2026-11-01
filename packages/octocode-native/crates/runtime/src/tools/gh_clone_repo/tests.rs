use super::pin::api_refusal;
use super::validate::{is_sparse_path, repository_url, web_root};
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::policy::path::PathPolicyConfig;
use crate::tools::gh_shared::test_support::{
    mock_endpoint, one_shot_provider, one_shot_provider_at,
};
use crate::tools::test_support;
use octocode_contract_types::{ErrorCode, GhCloneRepoQueryPathsItem, GhCloneRepoQueryRef};
use octocode_github::CredentialSource;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex, PoisonError, Weak};
use std::time::{Duration, Instant};

mod access;
mod bench;
mod concurrency;
mod git_process;
mod home;
mod lifecycle;
mod pinning;
mod replacement;
mod rows;
mod sparse;

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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(format!("{request:?}"));
        let args = request
            .args
            .iter()
            .map(|argument| {
                if argument
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&self.from.to_string_lossy())
                {
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

/// A temp root and the existing clone-output `home` inside it.
fn home_root() -> (test_support::Fixture, PathBuf) {
    let root = test_support::Fixture::new();
    let home = root.0.join("home");
    fs::create_dir_all(&home).expect("home");
    (root, home)
}

fn query() -> GhCloneRepoQuery {
    parse_query(serde_json::json!({
        "owner": "fixture-owner", "repo": "fixture-repo", "ref": "main"}))
}

fn one(path: &str) -> Vec<GhCloneRepoQueryPathsItem> {
    many(&[path])
}

fn many(paths: &[&str]) -> Vec<GhCloneRepoQueryPathsItem> {
    paths
        .iter()
        .map(|path| (*path).parse().expect("valid sparse fixture path"))
        .collect()
}

fn reference(value: &str) -> GhCloneRepoQueryRef {
    value.parse().expect("valid fixture branch or commit")
}

fn rejects_sparse_path(path: &str) -> bool {
    let mut row = serde_json::to_value(query()).expect("fixture query JSON");
    row["paths"] = serde_json::json!([path]);
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

fn credential(token: &str) -> ResolvedCredential {
    ResolvedCredential::new(token, CredentialSource::Override)
}
