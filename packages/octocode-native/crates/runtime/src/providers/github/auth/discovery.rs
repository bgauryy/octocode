//! Optional GitHub CLI discovery, using the runtime's explicit environment.
use super::resolver::{bounded, resolution_stopped};
use octocode_github::ProviderError;
use octocode_io::budget::RequestBudget;
use secrecy::SecretString;
use std::{collections::BTreeMap, ffi::OsString, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

const MAX_TOKEN_BYTES: u64 = 8192;
/// The variable gh (Go's `os.UserHomeDir`) reads its home from. Its login and
/// state live there; without it gh has no login to return and writes its state
/// (`.local/state/gh`) relative to the working directory.
const HOME_VAR: &str = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
const COMMON_GH_PATHS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/home/linuxbrew/.linuxbrew/bin",
];

fn discovery_path(path: Option<&str>) -> OsString {
    let existing: Vec<_> = std::env::split_paths(path.unwrap_or("")).collect();
    let mut dirs = existing.clone();
    if cfg!(unix) {
        for common in COMMON_GH_PATHS {
            let common = std::path::PathBuf::from(common);
            if !existing.contains(&common) {
                dirs.push(common);
            }
        }
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| OsString::from(path.unwrap_or("")))
}

pub(super) async fn gh_token<'a>(
    host: &str,
    env: impl Iterator<Item = (&'a str, &'a str)>,
    budget: &RequestBudget,
) -> Result<Option<SecretString>, ProviderError> {
    budget.check().map_err(resolution_stopped)?;
    let mut env: BTreeMap<_, _> = env.collect();
    if env.get(HOME_VAR).is_none_or(|home| home.is_empty()) {
        return Ok(None);
    }
    // Environment selection already happened with host checks. gh must consult
    // its own host-scoped store, not reinterpret an off-host environment token.
    for key in crate::config::ENV_TOKEN_VARS {
        env.remove(key);
    }
    env.remove("GH_ENTERPRISE_TOKEN");
    env.remove("GITHUB_ENTERPRISE_TOKEN");
    let mut command = tokio::process::Command::new("gh");
    command
        .args(["auth", "token", "--hostname", host])
        .env_clear()
        .envs(&env)
        .env("PATH", discovery_path(env.get("PATH").copied()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    run_command(command, budget).await
}

async fn run_command(
    mut command: tokio::process::Command,
    budget: &RequestBudget,
) -> Result<Option<SecretString>, ProviderError> {
    budget.check().map_err(resolution_stopped)?;
    octocode_engine::process::contain(command.as_std_mut());
    let Ok(mut child) = command.spawn() else {
        return Ok(None);
    };
    let mut job = match octocode_engine::process::Job::attach_async(&child, None) {
        Ok(job) => job,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Ok(None);
        }
    };
    let result = bounded(budget, async {
        let operation = async {
            let stdout = child.stdout.take()?;
            let mut bytes = Vec::new();
            stdout
                .take(MAX_TOKEN_BYTES + 1)
                .read_to_end(&mut bytes)
                .await
                .ok()?;
            if bytes.len() > MAX_TOKEN_BYTES as usize {
                return None;
            }
            #[cfg(unix)]
            while !octocode_engine::process::exited_without_reaping(child.id()?).ok()? {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            #[cfg(not(unix))]
            if !child.wait().await.ok()?.success() {
                return None;
            }
            let text = String::from_utf8(bytes).ok()?;
            let token = text.trim();
            (!token.is_empty()).then(|| SecretString::from(token))
        };
        Ok(tokio::time::timeout(Duration::from_secs(5), operation)
            .await
            .ok()
            .flatten())
    })
    .await;
    // Sweep before reaping so a pid-addressed kill cannot reach another child.
    if let Some(pid) = child.id() {
        octocode_engine::process::kill_tree_blocking(pid).await;
    }
    job.terminate();
    if child.id().is_some() {
        let _ = child.kill().await;
    }
    let status = child.wait().await;
    match result {
        Ok(token) => Ok(token.filter(|_| status.is_ok_and(|status| status.success()))),
        Err(error) => Err(error),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
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
        let path = dir.path().to_str().unwrap();
        let env = [
            ("PATH", path),
            (HOME_VAR, path),
            ("GH_TOKEN", "off-host"),
            ("GITHUB_TOKEN", "off-host"),
            ("GH_ENTERPRISE_TOKEN", "off-host"),
        ];
        let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1);
        let token = gh_token("enterprise.example", env.into_iter(), &budget)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(token.expose_secret(), "discovered");
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
            let token = gh_token("github.com", env.into_iter(), &budget).await;
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
}

/// Cancelling a lookup takes down the whole process tree it started: the
/// group kill on Unix, the job on Windows (the fixture is described in
/// `process_fixture`).
#[cfg(test)]
mod containment_tests {
    use super::*;
    use crate::process_fixture::{Holder, Leader, Tree, gone_within, kill};

    #[tokio::test]
    async fn cancelling_a_token_lookup_leaves_no_descendant() {
        let (command, tree) = Tree::new(Leader::Sleep, Holder::Silent, false);
        let mut command = tokio::process::Command::from(command);
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let budget = RequestBudget::with_timeout(Duration::from_secs(30), 1);
        let cancel = budget.cancellation.clone();
        let (result, pid) = tokio::join!(run_command(command, &budget), async {
            let pid = tokio::task::spawn_blocking(move || tree.holder_pid())
                .await
                .expect("pid");
            cancel.cancel();
            pid
        });
        let gone = gone_within(pid, Duration::from_secs(5));
        kill(pid);
        assert_eq!(
            result.unwrap_err().kind,
            octocode_github::ProviderErrorKind::Cancelled
        );
        assert!(gone, "descendant {pid} survived token cancellation");
    }
}
