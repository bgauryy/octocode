//! Optional GitHub CLI discovery, using the runtime's explicit environment.
use super::resolver::{bounded, resolution_stopped};
use octocode_github::ProviderError;
use octocode_io::budget::RequestBudget;
use secrecy::{ExposeSecret, SecretString};
use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    path::PathBuf,
    process::Stdio,
    sync::{LazyLock, Mutex, PoisonError},
    time::{Duration, SystemTime},
};

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

/// Where gh keeps its logins: `hosts.yml` in its configuration directory
/// (`GH_CONFIG_DIR`, else `XDG_CONFIG_HOME/gh`, else `%AppData%/GitHub CLI`
/// on Windows, else `~/.config/gh`), as gh itself resolves it. gh rewrites it
/// on every login, logout, account switch and token change it stores.
fn hosts_file(env: &BTreeMap<&str, &str>) -> Option<PathBuf> {
    let set = |name: &str| env.get(name).copied().filter(|value| !value.is_empty());
    let dir = if let Some(dir) = set("GH_CONFIG_DIR") {
        PathBuf::from(dir)
    } else if let Some(xdg) = set("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("gh")
    } else if cfg!(windows)
        && let Some(app) = set("AppData").or_else(|| set("APPDATA"))
    {
        PathBuf::from(app).join("GitHub CLI")
    } else {
        PathBuf::from(set(HOME_VAR)?).join(".config").join("gh")
    };
    Some(dir.join("hosts.yml"))
}

/// The gh login state a token was read under: the host, the environment gh
/// ran with (by digest), and its `hosts.yml` as last written. Any change of
/// them asks gh again.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct GhAsked {
    host: String,
    env: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GhStamp {
    modified: SystemTime,
    len: u64,
}

impl GhAsked {
    fn of(host: &str, env: &BTreeMap<&str, &str>) -> (Self, Option<GhStamp>) {
        let mut bytes = Vec::new();
        for (name, value) in env {
            bytes.extend_from_slice(name.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(value.as_bytes());
            bytes.push(0);
        }
        let stamp = hosts_file(env)
            .and_then(|file| std::fs::metadata(file).ok())
            .and_then(|meta| {
                Some(GhStamp {
                    modified: meta.modified().ok()?,
                    len: meta.len(),
                })
            });
        let asked = Self {
            host: host.to_owned(),
            env: octocode_engine::digest::sha256(&bytes),
        };
        (asked, stamp)
    }
}

/// Tokens gh answered, by what they were asked under: a repeat in this
/// process reuses one while gh's `hosts.yml` is unchanged, instead of
/// spawning gh for every GitHub call (100-220 ms each, measured
/// 2026-11-13). One entry per host and environment: a newer answer replaces
/// it.
static REMEMBERED: LazyLock<Mutex<HashMap<GhAsked, (GhStamp, SecretString)>>> =
    LazyLock::new(Mutex::default);

fn remembered(asked: &GhAsked, stamp: &GhStamp) -> Option<SecretString> {
    let held = REMEMBERED.lock().unwrap_or_else(PoisonError::into_inner);
    held.get(asked)
        .filter(|(kept, _)| kept == stamp)
        .map(|(_, token)| token.clone())
}

fn remember(asked: GhAsked, stamp: GhStamp, token: &SecretString) {
    REMEMBERED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(asked, (stamp, token.clone()));
}

/// The token gh holds for `host`. One gh answered before under the same
/// login state is reused, unless `refused` (GitHub rejected it): a token
/// rotated in the keyring without a `hosts.yml` write is asked for again.
pub(super) async fn gh_token<'a>(
    host: &str,
    env: impl Iterator<Item = (&'a str, &'a str)>,
    budget: &RequestBudget,
    refused: &(dyn Fn(&str) -> bool + Sync),
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
    let (asked, stamp) = GhAsked::of(host, &env);
    if let Some(stamp) = &stamp
        && let Some(token) = remembered(&asked, stamp)
        && !refused(token.expose_secret())
    {
        return Ok(Some(token));
    }
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
    let token = run_command(command, budget).await?;
    if let (Some(stamp), Some(token)) = (stamp, &token) {
        remember(asked, stamp, token);
    }
    Ok(token)
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
    let Ok(mut job) = octocode_engine::process::Job::attach_async(&child, None) else {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Ok(None);
    };
    let result = bounded(budget, async {
        let operation = async {
            let stdout = child.stdout.take()?;
            let bytes = octocode_io::read::read_limited_async(stdout, MAX_TOKEN_BYTES)
                .await
                .ok()??;
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
