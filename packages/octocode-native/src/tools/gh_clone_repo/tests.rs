use super::*;
use crate::policy::path::PathPolicyConfig;
use crate::providers::github::CredentialSource;
use crate::tools::local_fetch::NeverCancel;
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
    GhCloneRepoQuery {
        owner: "fixture-owner".into(),
        repo: "fixture-repo".into(),
        branch: Some("main".into()),
        sparse_path: None,
        force_refresh: false,
    }
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
    assert!(!cached.location.verified);
    assert_eq!(cached.location.local_path, fresh.location.local_path);
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
            force_refresh: true,
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
            sparse_path: Some("src".into()),
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
            sparse_path: Some("src".into()),
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
    assert!(cache::promote(&missing_stage, &destination).is_err());
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
            sparse_path: Some("missing/path".into()),
            ..query()
        },
        &context,
    )
    .expect_err("missing sparse path");
    assert_eq!(missing.code, "clone.sparsePath.notFound");
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
    .expect_err("missing branch");
    assert_eq!(error.code, "clone.defaultBranchUnavailable");
    context.resolved_default_branch = Some("main");
    let error = execute_clone(&query(), &context).expect_err("missing git");
    assert_eq!(error.code, "clone.git.unavailable");

    let insecure = GitHubEndpoint::new(url::Url::parse("http://example.test/api/v3").expect("URL"))
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
                sparse_path: Some(sparse_path.into()),
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
            owner: "../escape".into(),
            ..query()
        },
        GhCloneRepoQuery {
            sparse_path: Some("../escape".into()),
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
