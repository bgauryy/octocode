use super::*;
use crate::policy::path::PathPolicyConfig;
use crate::providers::github::CredentialSource;
use crate::tools::cancel::NeverCancel;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);
impl Temp {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "octocode-clone-{label}-{}-{nonce}-{}",
            std::process::id(),
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create fixture root");
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RewriteRunner {
    system: SystemGit,
    from: OsString,
    to: OsString,
    seen: Mutex<Vec<String>>,
}

impl RewriteRunner {
    fn new(from: &str, to: &str) -> Self {
        Self {
            system: SystemGit::default(),
            from: from.into(),
            to: to.into(),
            seen: Mutex::new(vec![]),
        }
    }
}

impl GitRunner for RewriteRunner {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
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
        self.system.run(&request.with_args(args), control)
    }
}

struct Fixture {
    _temp: Temp,
    work: PathBuf,
    bare_url: String,
    first_commit: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = Temp::new("repo");
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
    command.output().expect("run fixture git")
}

fn setup<'a>(
    root: &'a Path,
    runner: &'a dyn GitRunner,
    config: &'a CloneConfig,
    endpoint: &'a GitHubEndpoint,
    policy: &'a PathPolicy,
    cancellation: &'a dyn CancellationCheck,
    credential: Option<&'a ResolvedCredential>,
) -> CloneContext<'a> {
    let _ = root;
    CloneContext {
        config,
        endpoint,
        credential,
        resolved_default_branch: Some("main"),
        cancellation,
        deadline: Instant::now() + Duration::from_secs(30),
        path_policy: policy,
        git: runner,
    }
}

fn query() -> GhCloneRepoQuery {
    parse_query(serde_json::json!({
        "owner": "fixture-owner", "repo": "fixture-repo", "branch": "main",
        "goal": "test", "reasoning": "clone fixture"
    }))
}

/// One sparse path, as a single-string `sparsePath`.
fn one(path: &str) -> GhCloneRepoQuerySparsePath {
    GhCloneRepoQuerySparsePath::String(path.to_owned())
}

fn parse_query(value: serde_json::Value) -> GhCloneRepoQuery {
    serde_json::from_value(value).expect("valid ghCloneRepo query")
}

#[test]
fn missing_repository_names_the_repository_with_a_recovery_hint() {
    let error = repository_not_found(&query());
    assert_eq!(error.code, "clone.repositoryNotFound");
    assert_eq!(
        error.message,
        "Repository not found: fixture-owner/fixture-repo"
    );
    assert_eq!(error.hints.len(), 1);
    // D8: the hint fits the 120-char guidance cap whole (the message names
    // the repository), so the response stage never cuts it mid-sentence.
    let long = repository_not_found(&parse_query(serde_json::json!({
        "owner": "o".repeat(39), "repo": "r".repeat(100),
        "goal": "test", "reasoning": "long names"
    })));
    for hint in [&error.hints[0], &long.hints[0]] {
        assert!(hint.chars().count() <= 120, "{hint}");
        assert!(hint.ends_with('.'), "{hint}");
        assert!(hint.contains("token"), "{hint}");
    }
    // Errors without recovery serialize exactly as before.
    let plain = CloneError::new("clone.failed", "failed");
    assert_eq!(
        serde_json::to_value(plain).expect("serialize"),
        serde_json::json!({"code": "clone.failed", "message": "failed"})
    );
}

/// D8: a clone row continues into the local tools on its checkout (the
/// sparse subtree when one was requested), and a cache hit names its age.
#[test]
fn clone_rows_continue_into_local_tools_and_name_the_cache_age() {
    let fixture = Fixture::new();
    let root = Temp::new("next");
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let runner = RewriteRunner::new(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
    );
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(
        &root.0,
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );
    let row = |result: &CloneResult| {
        let data = serde_json::to_value(result).expect("serialize");
        // The runtime gives every continuation its row's brief.
        let mut briefed = data.clone();
        for call in briefed["next"]
            .as_object_mut()
            .into_iter()
            .flat_map(|next| next.values_mut())
        {
            call["query"]["goal"] = serde_json::json!("test");
            call["query"]["reasoning"] = serde_json::json!("clone fixture");
        }
        crate::contracts::validate_output(
            "ghCloneRepo",
            &serde_json::json!({"results":[{"index":0,"data":briefed}]}),
        )
        .expect("clone row satisfies the output contract");
        data
    };
    let fresh = row(&execute_clone(&query(), &context).expect("fresh clone"));
    let explore = &fresh["next"]["exploreClone"];
    assert_eq!(explore["tool"], "structureSearch", "{fresh}");
    assert_eq!(explore["query"]["path"], fresh["location"]["localPath"]);
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

    let cached = row(&execute_clone(&query(), &context).expect("cached clone"));
    assert_eq!(cached["location"]["cached"], true);
    let cloned_at = cached["location"]["clonedAt"].as_str().expect("clonedAt");
    let expires_at = cached["location"]["expiresAt"].as_str().expect("expiresAt");
    assert!(
        cloned_at.ends_with('Z') && expires_at > cloned_at,
        "{cached}"
    );

    let sparse = row(&execute_clone(
        &GhCloneRepoQuery {
            sparse_path: Some(one("src")),
            ..query()
        },
        &context,
    )
    .expect("sparse clone"));
    let local = sparse["location"]["localPath"].as_str().expect("localPath");
    let path = sparse["next"]["exploreClone"]["query"]["path"]
        .as_str()
        .expect("path");
    assert_eq!(Path::new(path), Path::new(local).join("src"), "{sparse}");
    assert_eq!(sparse["next"]["exploreClone"]["tool"], "structureSearch");

    // A single checked-out file is read, not listed as a directory.
    let file = row(&execute_clone(
        &GhCloneRepoQuery {
            sparse_path: Some(one("src/lib.rs")),
            ..query()
        },
        &context,
    )
    .expect("sparse file clone"));
    let local = file["location"]["localPath"].as_str().expect("localPath");
    let explore = &file["next"]["exploreClone"];
    assert_eq!(explore["tool"], "localFetch", "{file}");
    let path = explore["query"]["path"].as_str().expect("path");
    assert_eq!(
        Path::new(path),
        Path::new(local).join("src/lib.rs"),
        "{file}"
    );
    assert!(Path::new(path).is_file(), "{file}");
}

#[test]
fn creates_a_new_configured_home_before_authorizing_the_clone_target() {
    let fixture = Fixture::new();
    let root = Temp::new("new-home");
    let cache_home = root.0.join("new").join("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(fixture.work.clone()),
        additional_roots: vec![cache_home.clone()],
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let runner = RewriteRunner::new(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
    );
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(
        &root.0,
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );

    assert!(!cache_home.exists());
    let result = execute_clone(&query(), &context).expect("new home clone");
    assert!(cache_home.is_dir());
    assert_eq!(result.location.commit_sha, fixture.first_commit);
    assert!(Path::new(&result.location.local_path).starts_with(cache_home));
}

#[test]
fn denied_cache_home_is_not_created() {
    let root = Temp::new("allowed");
    let outside = Temp::new("denied");
    let cache_home = outside.0.join("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let runner = SystemGit::default();
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(
        &root.0,
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );

    let error = execute_clone(&query(), &context).expect_err("deny cache home");
    assert_eq!(error.code, "clone.policy.denied");
    assert!(!cache_home.exists(), "denied home must not be created");
}

#[test]
fn clones_caches_refreshes_sparse_tag_and_commit_without_token_argv() {
    let fixture = Fixture::new();
    let root = Temp::new("cache");
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let derived = "https://github.com/fixture-owner/fixture-repo.git";
    let runner = RewriteRunner::new(derived, &fixture.bare_url);
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(60);
    let credential = ResolvedCredential::new("fixture-token", CredentialSource::Override);
    let context = setup(
        &root.0,
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        Some(&credential),
    );

    let fresh = execute_clone(&query(), &context).expect("fresh clone");
    assert!(!fresh.location.cached);
    assert!(fresh.location.verified);
    assert_eq!(fresh.location.commit_sha, fixture.first_commit);
    assert_eq!(
        fs::read_to_string(Path::new(&fresh.location.local_path).join("README.md"))
            .expect("read clone"),
        "fixture\n"
    );
    assert!(fresh.total_size > 0);
    let cached = execute_clone(&query(), &context).expect("cached clone");
    assert!(cached.location.cached);
    // A cache hit reports the verification state persisted at clone time, not a
    // bare false: this checkout was verified when created, so the warm read
    // surfaces verified:true rather than a false negative.
    assert!(cached.location.verified);
    assert_eq!(cached.location.local_path, fresh.location.local_path);
    // Editing the cached working tree invalidates it: the next call re-clones
    // instead of serving modified bytes as the verified revision.
    let readme = Path::new(&cached.location.local_path).join("README.md");
    fs::write(&readme, "tampered\n").expect("tamper cached checkout");
    let recloned = execute_clone(&query(), &context).expect("reclone after tamper");
    assert!(!recloned.location.cached, "a dirty cache must not be a hit");
    assert!(recloned.location.verified);
    assert_eq!(
        fs::read_to_string(Path::new(&recloned.location.local_path).join("README.md"))
            .expect("read reclone"),
        "fixture\n"
    );
    let defaulted = execute_clone(
        &GhCloneRepoQuery {
            branch: None,
            ..query()
        },
        &context,
    )
    .expect("provider-resolved default branch");
    assert_eq!(defaulted.location.resolved_branch, "main");
    assert!(defaulted.location.cached);

    let second = fixture.push_second();
    let refreshed = execute_clone(
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

    let sparse = execute_clone(
        &GhCloneRepoQuery {
            sparse_path: Some(one("src")),
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
        let pinned = execute_clone(
            &GhCloneRepoQuery {
                branch: Some(branch),
                ..query()
            },
            &context,
        )
        .expect("pinned clone");
        assert_eq!(pinned.location.commit_sha, fixture.first_commit);
    }
    let pinned_sparse = execute_clone(
        &GhCloneRepoQuery {
            branch: Some(fixture.first_commit.clone()),
            sparse_path: Some(one("src")),
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
    assert!(seen.iter().all(|value| !value.contains("fixture-token")));
    assert!(seen.iter().any(|value| value.contains("[REDACTED]")));
}

#[test]
fn sparse_file_path_checks_out_only_that_file() {
    let fixture = Fixture::new();
    let root = Temp::new("sparse-file");
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let runner = RewriteRunner::new(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
    );
    let config = CloneConfig::persistent(&cache_home);
    let context = setup(
        &root.0,
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );
    for branch in [Some("main".to_owned()), Some(fixture.first_commit.clone())] {
        for (file, absent) in [
            ("README.md", ["src/lib.rs", "other/skip.txt"]),
            ("src/lib.rs", ["README.md", "src/nested/data.txt"]),
        ] {
            let clone = execute_clone(
                &GhCloneRepoQuery {
                    branch: branch.clone(),
                    sparse_path: Some(one(file)),
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
    let fixture = Fixture::new();
    let root = Temp::new("recovery");
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let runner = RewriteRunner::new(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
    );
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(60);
    config.lock_wait = Duration::from_secs(1);
    let context = setup(
        &root.0,
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );
    let first = execute_clone(&query(), &context).expect("first clone");
    let clone_path = PathBuf::from(&first.location.local_path);
    fs::write(clone_path.join(cache::META_FILE), "bad").expect("corrupt meta");
    assert!(
        !execute_clone(&query(), &context)
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
        !execute_clone(&query(), &context)
            .expect("rebuild expired")
            .location
            .cached
    );

    let lock = cache::lock_dir(&cache_home, &clone_path);
    fs::create_dir_all(&lock).expect("stale lock");
    fs::write(
        lock.join(cache::LOCK_META_FILE),
        r#"{"pid":4294967295,"createdAt":0}"#,
    )
    .expect("lock meta");
    assert!(
        execute_clone(&query(), &context)
            .expect("recover stale")
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
    let fixture = Fixture::new();
    let root = Temp::new("limits");
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let runner = RewriteRunner::new(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
    );
    let mut config = CloneConfig::persistent(&cache_home);
    config.max_clone_count = 1;
    config.lock_wait = Duration::from_millis(120);
    let context = setup(
        &root.0,
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );
    let main = execute_clone(&query(), &context).expect("main clone");
    let main_path = PathBuf::from(&main.location.local_path);
    let tag = execute_clone(
        &GhCloneRepoQuery {
            branch: Some("v1".into()),
            ..query()
        },
        &context,
    )
    .expect("tag clone");
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

    let live_lock = cache::lock_dir(&cache_home, Path::new(&tag.location.local_path));
    fs::create_dir_all(&live_lock).expect("live lock");
    fs::write(
        live_lock.join(cache::LOCK_META_FILE),
        serde_json::json!({"pid": std::process::id(), "createdAt": 0}).to_string(),
    )
    .expect("live lock metadata");
    let error = execute_clone(
        &GhCloneRepoQuery {
            branch: Some("v1".into()),
            ..query()
        },
        &context,
    )
    .expect_err("live lock must time out");
    assert_eq!(error.code, "clone.cache.lockTimeout");
    fs::remove_dir_all(live_lock).expect("remove live lock");

    let missing = execute_clone(
        &GhCloneRepoQuery {
            sparse_path: Some(one("missing/path")),
            ..query()
        },
        &context,
    )
    .expect_err("missing sparse path");
    assert_eq!(missing.code, "clone.sparsePath.notFound");
    assert!(missing.message.contains("\"missing/path\""), "{missing:?}");
    assert_eq!(missing.hints.len(), 1, "{missing:?}");
    assert!(
        !missing.message.contains("ghStructure"),
        "the hint, not the message, names the recovery: {missing:?}"
    );
    assert!(Path::new(&tag.location.local_path).is_dir());
}

#[test]
fn missing_branch_endpoint_git_and_portable_paths_fail_closed() {
    let root = Temp::new("negative");
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let config = CloneConfig::persistent(&cache_home);
    let missing_git = SystemGit::new(root.0.join("does-not-exist/git"));
    let mut context = setup(
        &root.0,
        &missing_git,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );
    context.resolved_default_branch = None;
    let error = execute_clone(
        &GhCloneRepoQuery {
            branch: None,
            ..query()
        },
        &context,
    )
    .expect_err("missing git");
    // An unresolved default branch is git's to resolve; without git the
    // request fails as git-unavailable, never as an unresolved branch.
    assert_eq!(error.code, "clone.git.unavailable");
    context.resolved_default_branch = Some("main");
    let error = execute_clone(&query(), &context).expect_err("missing git");
    assert_eq!(error.code, "clone.git.unavailable");

    // Plain http is only a valid API base on loopback; clone still refuses it.
    let insecure = GitHubEndpoint::new(url::Url::parse("http://127.0.0.1/api/v3").expect("URL"))
        .expect("provider endpoint");
    assert_eq!(
        repository_url(&insecure, "owner", "repo")
            .expect_err("HTTP clone endpoint")
            .code,
        "clone.endpoint.unsupported"
    );
    let encoded =
        GitHubEndpoint::new(url::Url::parse("https://example.test/prefix/api/v3").expect("URL"))
            .expect("GHES endpoint");
    assert_eq!(
        repository_url(&encoded, "owner name", "repo#name").expect("encoded URL"),
        "https://example.test/prefix/owner%20name/repo%23name.git"
    );
    for sparse_path in [r"..\escape", r"src\portable"] {
        let error = execute_clone(
            &GhCloneRepoQuery {
                sparse_path: Some(one(sparse_path)),
                ..query()
            },
            &context,
        )
        .expect_err("portable sparse traversal");
        assert_eq!(error.code, "clone.input.invalid");
    }
}

struct FlagCancel(AtomicBool);
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
    let root = Temp::new("cancel");
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
    let join = std::thread::spawn(move || {
        for _ in 0..200 {
            if trigger_pid_file.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        trigger.0.store(true, Ordering::SeqCst);
    });
    let runner = SystemGit::new(&script);
    let started = Instant::now();
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
            deadline: Instant::now() + Duration::from_secs(10),
            cache_home: &root.0,
        },
    );
    join.join().expect("trigger");
    assert_eq!(
        result.expect_err("cancel").code,
        "clone.execution.cancelled"
    );
    // The trigger itself allows two seconds for the fixture to publish its pid, so
    // leave scheduler margin while still proving cancellation beats both deadlines.
    assert!(started.elapsed() < Duration::from_secs(3));
    let pid: i32 = fs::read_to_string(pid_file)
        .expect("pid")
        .parse()
        .expect("integer pid");
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
    use std::os::unix::fs::PermissionsExt;
    let root = Temp::new("process-errors");
    let script = root.0.join("git-fixture.sh");
    fs::write(
        &script,
        "#!/bin/sh\nif [ \"$3\" = timeout ]; then sleep 30; fi\nenv >&2\nprintf 'arg:%s\\n' \"$@\" >&2\nexit 1\n",
    )
    .expect("script");
    let mut permissions = fs::metadata(&script).expect("metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script, permissions).expect("permissions");
    let runner = SystemGit::new(&script);
    let control = GitRunControl {
        cancellation: &NeverCancel,
        deadline: Instant::now() + Duration::from_secs(15),
        cache_home: &root.0,
    };
    let token = "highly-sensitive-token";
    let failed = runner
        .run(
            &GitRunRequest {
                args: vec!["fail".into()],
                timeout: Duration::from_secs(10),
                label: "redaction fixture".into(),
                authorization: Some(token),
                authorization_url: Some("https://github.com/owner/repo.git"),
            },
            &control,
        )
        .expect_err("failure fixture");
    assert_eq!(failed.code, "clone.git.failed");
    assert!(!failed.message.contains(token));
    assert!(failed.message.contains("[REDACTED]"));

    let started = Instant::now();
    let timed_out = runner
        .run(
            &GitRunRequest {
                args: vec!["timeout".into()],
                timeout: Duration::from_millis(60),
                label: "timeout fixture".into(),
                authorization: None,
                authorization_url: None,
            },
            &control,
        )
        .expect_err("timeout fixture");
    assert_eq!(timed_out.code, "clone.execution.timeout");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn concurrent_requests_publish_once_and_validation_fails_closed() {
    let fixture = Fixture::new();
    let root = Temp::new("race");
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    let policy = Arc::new(
        PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.0.clone()),
            ..Default::default()
        })
        .expect("policy"),
    );
    let endpoint = Arc::new(GitHubEndpoint::github_com());
    let runner = Arc::new(RewriteRunner::new(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
    ));
    let config = Arc::new(CloneConfig::persistent(&cache_home));
    let barrier = Arc::new(Barrier::new(2));
    let mut workers = vec![];
    for _ in 0..2 {
        let policy = policy.clone();
        let endpoint = endpoint.clone();
        let runner = runner.clone();
        let config = config.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            execute_clone(
                &query(),
                &CloneContext {
                    config: &config,
                    endpoint: &endpoint,
                    credential: None,
                    resolved_default_branch: Some("main"),
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

    for invalid in [
        GhCloneRepoQuery {
            owner: "../escape".parse().expect("owner"),
            ..query()
        },
        GhCloneRepoQuery {
            sparse_path: Some(one("../escape")),
            ..query()
        },
    ] {
        let context = CloneContext {
            config: &config,
            endpoint: &endpoint,
            credential: None,
            resolved_default_branch: Some("main"),
            cancellation: &NeverCancel,
            deadline: Instant::now() + Duration::from_secs(30),
            path_policy: &policy,
            git: runner.as_ref(),
        };
        assert_eq!(
            execute_clone(&invalid, &context).expect_err("invalid").code,
            "clone.input.invalid"
        );
    }
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

    let root = Temp::new("symlinks");
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let runner = RewriteRunner::new(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
    );
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(60);
    let context = setup(
        &root.0,
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );
    for (branch, sparse) in [("main", None), (head.as_str(), None), ("main", Some("src"))] {
        let clone = execute_clone(
            &GhCloneRepoQuery {
                branch: Some(branch.into()),
                sparse_path: sparse.map(one),
                ..query()
            },
            &context,
        )
        .expect("clone");
        let link = Path::new(&clone.location.local_path).join("escape");
        if sparse.is_none() {
            let meta = fs::symlink_metadata(&link).expect("escape entry");
            assert!(
                !meta.file_type().is_symlink(),
                "{branch}: symlink materialized"
            );
        }
    }
    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for step in [
        "full clone",
        "sparse clone",
        "fetch requested commit",
        "check out requested commit",
        "set sparse checkout paths",
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
    let fixture = Fixture::new();
    let root = Temp::new("sha-case");
    let cache_home = root.0.join("home");
    fs::create_dir_all(&cache_home).expect("home");
    let policy = PathPolicy::new(PathPolicyConfig {
        workspace_root: Some(root.0.clone()),
        ..Default::default()
    })
    .expect("policy");
    let endpoint = GitHubEndpoint::github_com();
    let runner = RewriteRunner::new(
        "https://github.com/fixture-owner/fixture-repo.git",
        &fixture.bare_url,
    );
    let mut config = CloneConfig::persistent(&cache_home);
    config.cache_ttl = Duration::from_secs(60);
    let context = setup(
        &root.0,
        &runner,
        &config,
        &endpoint,
        &policy,
        &NeverCancel,
        None,
    );
    let lower = execute_clone(
        &GhCloneRepoQuery {
            branch: Some(fixture.first_commit.clone()),
            ..query()
        },
        &context,
    )
    .expect("lowercase sha clone");
    let upper = execute_clone(
        &GhCloneRepoQuery {
            branch: Some(fixture.first_commit.to_ascii_uppercase()),
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
    let again = execute_clone(
        &GhCloneRepoQuery {
            branch: Some(fixture.first_commit.clone()),
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
    fixture: Fixture,
    root: Temp,
    policy: PathPolicy,
    endpoint: GitHubEndpoint,
    runner: RewriteRunner,
    config: CloneConfig,
}

impl Bench {
    fn new(label: &str) -> Self {
        let fixture = Fixture::new();
        let root = Temp::new(label);
        let cache_home = root.0.join("home");
        fs::create_dir_all(&cache_home).expect("home");
        let policy = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.0.clone()),
            ..Default::default()
        })
        .expect("policy");
        let runner = RewriteRunner::new(
            "https://github.com/fixture-owner/fixture-repo.git",
            &fixture.bare_url,
        );
        let mut config = CloneConfig::persistent(&cache_home);
        config.cache_ttl = Duration::from_secs(60);
        Self {
            fixture,
            root,
            policy,
            endpoint: GitHubEndpoint::github_com(),
            runner,
            config,
        }
    }

    fn context(&self) -> CloneContext<'_> {
        CloneContext {
            resolved_default_branch: None,
            ..setup(
                &self.root.0,
                &self.runner,
                &self.config,
                &self.endpoint,
                &self.policy,
                &NeverCancel,
                None,
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
        branch: None,
        ..query()
    }
}

/// An unbranched clone lets git resolve the default branch, and the
/// next unbranched call is a cache hit through the recorded alias — neither
/// needs the repository metadata API.
#[test]
fn unbranched_clone_resolves_the_default_branch_with_git_and_hits_by_alias() {
    let bench = Bench::new("default-branch");
    let context = bench.context();
    let fresh = execute_clone(&unbranched(), &context).expect("default-branch clone");
    assert_eq!(fresh.location.resolved_branch, "main");
    assert!(!fresh.location.cached);
    assert_eq!(fresh.location.commit_sha, bench.fixture.first_commit);
    let network = bench.clones_seen();
    assert_eq!(network, 1, "one git clone");

    let hit = execute_clone(&unbranched(), &context).expect("aliased hit");
    assert!(
        hit.location.cached,
        "the alias finds the default-branch entry"
    );
    assert_eq!(hit.location.local_path, fresh.location.local_path);
    assert_eq!(bench.clones_seen(), network, "a hit runs no network git");

    // The explicit branch shares the entry.
    let named = execute_clone(&query(), &context).expect("named hit");
    assert!(named.location.cached);
    assert_eq!(named.location.local_path, fresh.location.local_path);

    // forceRefresh ignores the alias and re-resolves through git.
    let second = bench.fixture.push_second();
    let refreshed = execute_clone(
        &GhCloneRepoQuery {
            force_refresh: Some(true),
            ..unbranched()
        },
        &context,
    )
    .expect("refresh");
    assert!(!refreshed.location.cached);
    assert_eq!(refreshed.location.commit_sha, second);
}

/// Several sparse paths check out together; a file among them switches
/// to exact non-cone patterns so nothing else is included.
#[test]
fn multiple_sparse_paths_check_out_every_path() {
    let bench = Bench::new("multi-sparse");
    let context = bench.context();
    let both = execute_clone(
        &GhCloneRepoQuery {
            sparse_path: Some(GhCloneRepoQuerySparsePath::Array(vec![
                "src/nested".into(),
                "other".into(),
            ])),
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
    assert_eq!(both.location.requested_path, None);
    let row = serde_json::to_value(&both).expect("serialize");
    assert_eq!(
        Path::new(
            row["next"]["exploreClone"]["query"]["path"]
                .as_str()
                .expect("path")
        ),
        local,
        "several paths explore the checkout root"
    );

    let reordered = execute_clone(
        &GhCloneRepoQuery {
            sparse_path: Some(GhCloneRepoQuerySparsePath::Array(vec![
                "other".into(),
                "src/nested".into(),
            ])),
            ..query()
        },
        &context,
    )
    .expect("same set, other order");
    assert!(
        reordered.location.cached,
        "request order does not split the cache"
    );

    let mixed = execute_clone(
        &GhCloneRepoQuery {
            sparse_path: Some(GhCloneRepoQuerySparsePath::Array(vec![
                "README.md".into(),
                "src/nested".into(),
            ])),
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

    let missing = execute_clone(
        &GhCloneRepoQuery {
            sparse_path: Some(GhCloneRepoQuerySparsePath::Array(vec![
                "src".into(),
                "nope".into(),
            ])),
            ..query()
        },
        &context,
    )
    .expect_err("one missing path fails the clone");
    assert_eq!(missing.code, "clone.sparsePath.notFound");
    assert!(missing.message.contains("\"nope\""), "{missing:?}");
    assert!(!missing.message.contains("\"src\""), "{missing:?}");
}

/// `depth` fetches that many commits and keys its own cache entry.
#[test]
fn history_depth_fetches_that_many_commits() {
    let bench = Bench::new("depth");
    let context = bench.context();
    bench.fixture.push_second();
    let shallow = execute_clone(&query(), &context).expect("shallow");
    let deep = execute_clone(
        &GhCloneRepoQuery {
            depth: std::num::NonZeroU64::new(2),
            ..query()
        },
        &context,
    )
    .expect("depth 2");
    assert_ne!(shallow.location.local_path, deep.location.local_path);
    assert_eq!(deep.location.depth, Some(2));
    assert_eq!(shallow.location.depth, None);
    let count = |path: &str| {
        git_output(Path::new(path), &["rev-list", "--count", "HEAD"])
            .trim()
            .to_owned()
    };
    assert_eq!(count(&shallow.location.local_path), "1");
    assert_eq!(count(&deep.location.local_path), "2");
}
