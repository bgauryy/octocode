//! System git: the credential on network steps only, cancellation, bounded redacted output, symlinks as plain files.
use super::*;
use octocode_test_support::json_fixture;

/// Only the git steps that reach the remote run with the credential: the
/// local store and worktree steps take no askpass helper and no network
/// permit. (A cold clone writes the origin into the store's config and has
/// no worktree to prune, so neither step runs.)
#[test]
fn only_network_git_steps_carry_the_credential() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("README.md"),
            ..query()
        },
        &context,
    )
    .expect("sparse clone");
    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let step = |label: &str| {
        seen.iter()
            .find(|value| value.contains(label))
            .unwrap_or_else(|| panic!("{label} not run: {seen:?}"))
            .clone()
    };
    for local in ["initialize shared store", "add worktree", "move worktree"] {
        assert!(
            step(local).contains("authorization_url: None"),
            "{}",
            step(local)
        );
    }
    for network in [
        "fetch requested commit",
        "set sparse commit paths",
        "check out requested commit",
    ] {
        assert!(
            step(network).contains("authorization_url: Some"),
            "{}",
            step(network)
        );
    }
}

/// A transfer ends when it stalls or the caller's deadline passes, never at
/// a multiple of the network timeout: every step may run until the caller's
/// deadline, and each network step makes git itself abort a stalled
/// transfer after the network timeout.
#[test]
fn network_steps_end_on_a_stall_or_the_callers_deadline() {
    let fixture = Fixture::shared();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let stall = Duration::from_secs(2);
    let config = GitConfig::persistent(&home).with_network_timeout(stall);
    let mut context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    context.deadline = Instant::now() + Duration::from_secs(600);
    clone_pinned(
        &fixture,
        &GhCloneRepoQuery {
            paths: one("README.md"),
            ..query()
        },
        &context,
    )
    .expect("sparse clone");
    let steps = runner
        .steps
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(!steps.is_empty());
    for (label, timeout, args) in steps.iter() {
        assert!(
            *timeout > stall.saturating_mul(4),
            "{label} is capped at {timeout:?}, below the caller's deadline"
        );
        let limited = args
            .iter()
            .any(|argument| argument == "http.lowSpeedLimit=1")
            && args
                .iter()
                .any(|argument| argument == &format!("http.lowSpeedTime={}", stall.as_secs()));
        let network = [
            "fetch requested commit",
            "set sparse commit paths",
            "check out requested commit",
        ]
        .contains(&label.as_str());
        if network {
            assert!(limited, "{label} has no stall limit: {args:?}");
        }
        if label == "initialize shared store" || label == "read checkout HEAD" {
            assert!(!limited, "{label} is local: {args:?}");
        }
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
            stdin: None,
        },
        &GitRunControl {
            cancellation: cancellation.as_ref(),
            deadline,
            home: &root.0,
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
        home: &root.0,
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
                stdin: None,
            },
            &control,
        )
        .expect_err("failure fixture");
    assert_eq!(failed.code, ErrorCode::GitFailed);
    // The child's environment dump names the helper and holds the token the
    // helper reads; the scrubbed error carries the helper, never the token.
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
                stdin: None,
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

    // Nor does it name the shared object store: git reports the common
    // directory of a linked worktree.
    let store = root
        .0
        .join("clone-store")
        .join("owner")
        .join("repo__host_0123456789abcdef__g1.git");
    let stored = runner
        .run(
            &GitRunRequest {
                args: vec![
                    "-c".into(),
                    format!(
                        "alias.octocode-fixture=!printf \"fatal: cannot lock ref in '%s'\\n\" '{}' >&2; exit 128",
                        store.display()
                    )
                    .into(),
                    "octocode-fixture".into(),
                ],
                timeout: Duration::from_secs(10),
                label: "store fixture".into(),
                authorization: None,
                authorization_url: None,
                stdin: None,
            },
            &control,
        )
        .expect_err("store fixture");
    test_support::checks::assert_absent(&serde_json::json!(stored.message), "clone-store");
    assert!(
        stored.message.contains("<store>") && stored.message.contains("cannot lock ref"),
        "{}",
        stored.message
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
                stdin: None,
            },
            &control,
        )
        .expect_err("timeout fixture");
    assert_eq!(timed_out.code, ErrorCode::Timeout);
    assert!(started.elapsed() < Duration::from_secs(2));
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

    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = GitConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    // The same commit by branch then by SHA: both calls fetch fresh and
    // reports the symlinks its meta recorded. A directory scope also holds
    // the root's files (the link among them); a file scope holds only it.
    for (branch, sparse, listed) in [
        ("main", None, true),
        (head.as_str(), None, true),
        ("main", Some("src"), true),
        ("main", Some("src/lib.rs"), false),
    ] {
        let clone = clone_pinned(
            &fixture,
            &GhCloneRepoQuery {
                ref_: Some(reference(branch)),
                paths: sparse.map(one).unwrap_or_default(),
                ..query()
            },
            &context,
        )
        .expect("clone");
        let link = Path::new(&clone.location.path).join("escape");
        // Git run in the checkout as any local step runs it (no
        // `-c core.symlinks=false`) sees nothing changed: the checkout's
        // own config records that its symlinks are plain files.
        let status = git_output(Path::new(&clone.location.path), &["status", "--porcelain"]);
        assert_eq!(
            status.trim(),
            "",
            "{branch} {sparse:?}: a fresh clone is clean"
        );
        if listed {
            let meta = fs::symlink_metadata(&link).expect("escape entry");
            assert!(
                !meta.file_type().is_symlink(),
                "{branch}: symlink materialized"
            );
            let row = serde_json::to_value(&clone.location).expect("location");
            assert_eq!(row["symlinks"], json_fixture(r#"["escape"]"#), "{row}");
        } else {
            assert!(!link.exists(), "{branch} {sparse:?}");
            assert_eq!(clone.location.symlinks, None, "{branch} {sparse:?}");
        }
    }
    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for step in [
        "initialize shared store",
        "add worktree",
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
