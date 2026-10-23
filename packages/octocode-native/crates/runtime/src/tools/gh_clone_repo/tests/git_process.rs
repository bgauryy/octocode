//! System git: the credential on network steps only, cancellation, bounded redacted output, symlinks as plain files.
use super::*;

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
