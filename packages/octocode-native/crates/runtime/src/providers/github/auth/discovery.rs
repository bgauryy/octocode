//! Optional GitHub CLI discovery, using the runtime's explicit environment.
use super::super::ProviderError;
use super::resolver::{bounded, resolution_stopped};
use crate::providers::RequestBudget;
use secrecy::SecretString;
use std::{collections::BTreeMap, ffi::OsString, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

const MAX_TOKEN_BYTES: u64 = 8192;
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
    #[cfg(unix)]
    command.process_group(0);
    let Ok(mut child) = command.spawn() else {
        return Ok(None);
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
    // Cancellation, oversized output, timeout and read errors all reap the child.
    #[cfg(unix)]
    if let Some(pid) = child.id().and_then(|pid| i32::try_from(pid).ok()) {
        // SAFETY: this unreaped child leads the process group created above.
        // Its PID cannot be recycled until wait(), so only our group is targeted.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    if child.id().is_some() {
        let _ = child.kill().await;
    }
    let _ = child.wait().await;
    result
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
    #[cfg(unix)]
    #[tokio::test]
    async fn request_deadline_and_cancellation_stop_discovery() {
        let budget = RequestBudget::with_timeout(Duration::from_millis(50), 1);
        let start = std::time::Instant::now();
        let result = run_command(shell("exec sleep 30"), &budget).await;
        assert_eq!(
            result.unwrap_err().kind,
            super::super::super::ProviderErrorKind::Timeout
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
            super::super::super::ProviderErrorKind::Cancelled
        );
    }
}
