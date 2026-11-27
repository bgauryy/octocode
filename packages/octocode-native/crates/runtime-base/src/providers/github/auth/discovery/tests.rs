use super::*;
#[cfg(unix)]
#[test]
fn common_paths_are_added_without_duplicates() {
    let original = "/custom/bin:/opt/homebrew/bin";
    let result: Vec<_> = std::env::split_paths(&discovery_path(Some(original))).collect();
    assert_eq!(
        result
            .iter()
            .filter(|p| p.as_os_str() == "/opt/homebrew/bin")
            .count(),
        1
    );
    assert!(result.iter().any(|p| p.as_os_str() == "/custom/bin"));
}
#[cfg(unix)]
fn shell(script: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("/bin/sh");
    command
        .args(["-c", script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command
}
#[cfg(unix)]
#[tokio::test]
async fn discovery_uses_explicit_path_and_removes_environment_credentials() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("gh");
    std::fs::write(&executable, "#!/bin/sh\n[ -z \"$GH_TOKEN$GITHUB_TOKEN$GH_ENTERPRISE_TOKEN\" ] || exit 2\n[ \"$4\" = 'enterprise.example' ] || exit 3\nprintf discovered\n").unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    // The first exec of a new executable can wait on the system's scan of
    // it (seconds under load), which would spend discovery's own 5 s cap:
    // run it once first so the call below times only discovery.
    let _ = std::process::Command::new(&executable).output();
    let path = dir.path().to_str().unwrap();
    let env = [
        ("PATH", path),
        (HOME_VAR, path),
        ("GH_TOKEN", "off-host"),
        ("GITHUB_TOKEN", "off-host"),
        ("GH_ENTERPRISE_TOKEN", "off-host"),
    ];
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1);
    let token = gh_token("enterprise.example", env.into_iter(), &budget, &|_| false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(token.expose_secret(), "discovered");
}

/// gh answers once per login state: a repeat reuses its token until gh
/// rewrites `hosts.yml` (a login, logout, switch or rotation it stores),
/// and a token GitHub rejected is asked for again.
#[cfg(unix)]
#[tokio::test]
async fn gh_is_asked_again_only_when_its_login_state_changes() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let runs = dir.path().join("runs");
    let executable = dir.path().join("gh");
    let script = format!(
        "#!/bin/sh\necho run >> '{}'\nprintf \"token-$(/usr/bin/wc -l < '{}' | /usr/bin/tr -d ' ')\"\n",
        runs.display(),
        runs.display()
    );
    std::fs::write(&executable, script).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let _ = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg("true")
        .output();
    let config = dir.path().join(".config/gh");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("hosts.yml"), "github.com:\n  user: a\n").unwrap();
    let path = dir.path().to_str().unwrap();
    let env = || [("PATH", path), (HOME_VAR, path)].into_iter();
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1);
    let ran = || std::fs::read_to_string(&runs).map_or(0, |text| text.lines().count());
    let ask = |refused: &'static (dyn Fn(&str) -> bool + Sync)| {
        let budget = &budget;
        async move {
            gh_token("github.com", env(), budget, refused)
                .await
                .unwrap()
                .map(|token| token.expose_secret().to_owned())
        }
    };
    assert_eq!(ask(&|_| false).await.as_deref(), Some("token-1"));
    assert_eq!(ask(&|_| false).await.as_deref(), Some("token-1"));
    assert_eq!(ran(), 1, "an unchanged login state reuses the token");
    std::fs::write(
        config.join("hosts.yml"),
        "github.com:\n  user: someone-else\n",
    )
    .unwrap();
    assert_eq!(ask(&|_| false).await.as_deref(), Some("token-2"));
    assert_eq!(ran(), 2, "a rewritten hosts.yml asks gh again");
    assert_eq!(
        ask(&|token| token == "token-2").await.as_deref(),
        Some("token-3")
    );
    assert_eq!(ran(), 3, "a rejected token is asked for again");
}

#[cfg(unix)]
#[tokio::test]
async fn discovery_without_a_home_never_runs_gh() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let ran = dir.path().join("ran");
    let executable = dir.path().join("gh");
    let script = format!("#!/bin/sh\ntouch '{}'\nprintf discovered\n", ran.display());
    std::fs::write(&executable, script).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().to_str().unwrap();
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1);
    for env in [vec![("PATH", path)], vec![("PATH", path), (HOME_VAR, "")]] {
        let token = gh_token("github.com", env.into_iter(), &budget, &|_| false).await;
        assert!(token.unwrap().is_none());
    }
    assert!(!ran.exists(), "gh ran without a home");
}

#[cfg(unix)]
#[tokio::test]
async fn reads_trimmed_token_and_rejects_failure_or_oversized_output() {
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1);
    assert_eq!(
        run_command(shell("printf ' synthetic-token\\n'"), &budget)
            .await
            .unwrap()
            .unwrap()
            .expose_secret(),
        "synthetic-token"
    );
    assert!(
        run_command(shell("printf secret; exit 1"), &budget)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        run_command(shell("while :; do printf '0123456789'; done"), &budget)
            .await
            .unwrap()
            .is_none()
    );
}
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tokio::test]
async fn token_lookup_kills_a_setsid_descendant() {
    let dir = tempfile::tempdir().expect("dir");
    let pidfile = dir.path().join("escaped.pid");
    let mut command = tokio::process::Command::new("python3");
    // The descendant publishes its pid atomically (write, then rename),
    // so the waiter never reads a created-but-empty file.
    command.args(["-c", "import os,sys,time\nif os.fork()==0:\n os.setsid()\n os.close(1); os.close(2)\n p=sys.argv[1]\n open(p+'.tmp','w').write(str(os.getpid()))\n os.rename(p+'.tmp',p)\n time.sleep(60)\nelse:\n time.sleep(60)\n"]);
    command
        .arg(&pidfile)
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    // Generous bounds: a loaded host may take seconds to start python;
    // the budget must not time out before the descendant exists.
    let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1);
    let cancel = budget.cancellation.clone();
    let (result, ()) = tokio::join!(run_command(command, &budget), async {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !pidfile.exists() && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        cancel.cancel();
    });
    let pid: u32 = std::fs::read_to_string(pidfile)
        .expect("escaped pid")
        .trim()
        .parse()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while crate::process_status::is_alive(pid) && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let survived = crate::process_status::is_alive(pid);
    if survived {
        // SAFETY: this fixture recorded the pid of its own live child.
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
    assert_eq!(
        result.unwrap_err().kind,
        octocode_github::ProviderErrorKind::Cancelled
    );
    assert!(
        !survived,
        "setsid descendant {pid} survived token cancellation"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn request_deadline_and_cancellation_stop_discovery() {
    let budget = RequestBudget::with_timeout(Duration::from_millis(50), 1);
    let start = std::time::Instant::now();
    let result = run_command(shell("exec sleep 30"), &budget).await;
    assert_eq!(
        result.unwrap_err().kind,
        octocode_github::ProviderErrorKind::Timeout
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1);
    let cancel = budget.cancellation.clone();
    let (result, ()) = tokio::join!(run_command(shell("exec sleep 30"), &budget), async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel.cancel();
    });
    assert_eq!(
        result.unwrap_err().kind,
        octocode_github::ProviderErrorKind::Cancelled
    );
}
