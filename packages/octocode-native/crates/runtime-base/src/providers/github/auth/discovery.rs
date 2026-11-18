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
mod tests;

/// Cancelling a lookup takes down the whole process tree it started: the
/// group kill on Unix, the job on Windows (the fixture is described in
/// `process_fixture`).
#[cfg(test)]
mod containment_tests;
