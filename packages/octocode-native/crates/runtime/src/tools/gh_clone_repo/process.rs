mod askpass;
pub(super) use askpass::process_alive;
mod redaction;

use super::CloneError;
use crate::execution::cancel::CancellationCheck;
use crate::tools::bounded_process::{BoundedError, run_bounded, run_bounded_records};
use askpass::AskPass;
use octocode_contract_types::ErrorCode;
#[cfg(test)]
use redaction::basic_credential;
use redaction::scrub;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub(super) const MAX_OUTPUT_BYTES: usize = 5 * 1024 * 1024;

/// The oldest git whose linked worktrees keep their own sparse-checkout
/// settings (`extensions.worktreeConfig`, documented from 2.36): every
/// checkout is a worktree of a shared store, so each sparse set must stay its
/// own.
const GIT_FLOOR: (u64, u64) = (2, 36);

/// `(major, minor)` of a `git --version` line (`git version 2.43.0.windows.1`,
/// `git version 2.39.3 (Apple Git-146)`); `None` when it has no such version.
fn git_version(banner: &str) -> Option<(u64, u64)> {
    let version = banner.trim().strip_prefix("git version ")?;
    let mut parts = version.split_whitespace().next()?.split('.');
    let number = |part: &str| {
        let digits = part
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        digits.parse().ok()
    };
    Some((number(parts.next()?)?, number(parts.next()?)?))
}

pub struct GitRunRequest<'a> {
    pub args: Vec<OsString>,
    pub timeout: Duration,
    pub label: String,
    pub(crate) authorization: Option<&'a str>,
    pub(crate) authorization_url: Option<&'a str>,
}

impl fmt::Debug for GitRunRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitRunRequest")
            .field("args", &self.args)
            .field("timeout", &self.timeout)
            .field("label", &self.label)
            .field("authorization", &self.authorization.map(|_| "[REDACTED]"))
            .field("authorization_url", &self.authorization_url)
            .finish()
    }
}

#[cfg(test)]
impl GitRunRequest<'_> {
    /// Preserve private auth transport while allowing an injected runner to
    /// rewrite only argv for a hermetic fixture.
    pub(crate) fn with_args(&self, args: Vec<OsString>) -> Self {
        Self {
            args,
            timeout: self.timeout,
            label: self.label.clone(),
            authorization: self.authorization,
            authorization_url: self.authorization_url,
        }
    }
}

pub struct GitRunControl<'a> {
    pub cancellation: &'a dyn CancellationCheck,
    pub deadline: Instant,
    pub home: &'a Path,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
}

type RecordConsumer<'a> = dyn FnMut(&[u8]) -> Result<(), CloneError> + Send + 'a;

pub trait GitRunner: Send + Sync {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError>;

    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError>;

    fn assert_available(&self, control: &GitRunControl<'_>) -> Result<(), CloneError> {
        match self.run(
            &GitRunRequest {
                args: vec![OsString::from("--version")],
                timeout: Duration::from_secs(5),
                label: "check Git availability".into(),
                authorization: None,
                authorization_url: None,
            },
            control,
        ) {
            Ok(output) => match git_version(&output.stdout) {
                Some(version) if version >= GIT_FLOOR => Ok(()),
                _ => Err(CloneError::new(
                    ErrorCode::GitUnavailable,
                    format!(
                        "git {}.{} or newer is required: ghCloneRepo keeps one sparse checkout per linked worktree. Found: {}",
                        GIT_FLOOR.0,
                        GIT_FLOOR.1,
                        output.stdout.trim()
                    ),
                )),
            },
            Err(error) if matches!(error.code, ErrorCode::Cancelled | ErrorCode::Timeout) => {
                Err(error)
            }
            Err(_) => Err(CloneError::new(
                ErrorCode::GitUnavailable,
                "git is not installed or not on PATH. The ghCloneRepo tool requires git to be available.",
            )),
        }
    }
}

/// Git's own runtime environment names the isolated git keeps besides the
/// HTTP client's ([`octocode_io::http::NETWORK_ENV`]): its PATH and its own
/// trust-store settings. Nothing that weakens TLS (`GIT_SSL_NO_VERIFY`) or
/// carries a credential passes.
const GIT_ENV: &[&str] = &[
    "PATH",
    "GIT_SSL_CAINFO",
    "GIT_SSL_CAPATH",
    "CURL_CA_BUNDLE",
    #[cfg(windows)]
    "SystemRoot",
];

/// Every runtime environment name the isolated git keeps: [`GIT_ENV`] plus
/// the proxy and trust-store settings a corporate network needs.
fn passthrough_env() -> impl Iterator<Item = &'static str> {
    GIT_ENV
        .iter()
        .chain(octocode_io::http::NETWORK_ENV)
        .copied()
}

#[derive(Clone, Debug)]
pub struct SystemGit {
    executable: PathBuf,
    /// [`passthrough_env`] values, read once from the runtime environment.
    passthrough: Vec<(String, OsString)>,
    /// Collected stdout/stderr bytes per git run ([`MAX_OUTPUT_BYTES`];
    /// tests lower it to reach the cap with a small repository).
    max_output: usize,
    /// Tests clone hermetic fixtures from local `file://` remotes, a
    /// transport production git never gets ([`SystemGit::with_file_transport`]).
    #[cfg(test)]
    file_transport: bool,
}

impl Default for SystemGit {
    /// The process environment, for callers without a runtime env map.
    fn default() -> Self {
        Self {
            executable: PathBuf::from("git"),
            passthrough: passthrough_env()
                .filter_map(|name| Some((name.to_owned(), std::env::var_os(name)?)))
                .collect(),
            max_output: MAX_OUTPUT_BYTES,
            #[cfg(test)]
            file_transport: false,
        }
    }
}

impl SystemGit {
    /// Git with the [`passthrough_env`] subset of the runtime `env` (the
    /// resolved config env, not the process's). PATH falls back to the
    /// process's when `env` has none, so git can still be found.
    pub fn with_env(env: &std::collections::BTreeMap<String, String>) -> Self {
        let mut passthrough: Vec<(String, OsString)> = passthrough_env()
            .filter_map(|name| {
                let value = env.get(name).filter(|value| !value.is_empty())?;
                Some((name.to_owned(), OsString::from(value)))
            })
            .collect();
        if !passthrough.iter().any(|(name, _)| name == "PATH")
            && let Some(path) = std::env::var_os("PATH")
        {
            passthrough.push(("PATH".to_owned(), path));
        }
        Self {
            executable: PathBuf::from("git"),
            passthrough,
            max_output: MAX_OUTPUT_BYTES,
            #[cfg(test)]
            file_transport: false,
        }
    }

    #[cfg(test)]
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            ..Self::default()
        }
    }

    /// This git with collected output capped at `bytes`.
    #[cfg(test)]
    pub(super) fn with_max_output(self, bytes: usize) -> Self {
        Self {
            max_output: bytes,
            ..self
        }
    }

    /// This git also allowed the `file` transport, for fixtures whose
    /// remote is a local bare repository.
    #[cfg(test)]
    pub(super) fn with_file_transport(self) -> Self {
        Self {
            file_transport: true,
            ..self
        }
    }
}

/// The transports git may use for every remote, submodule, promisor fetch
/// and redirect (`GIT_ALLOW_PROTOCOL`, which overrides any
/// `protocol.*.allow` config): HTTPS, plus plain HTTP only when the
/// authorized remote itself is loopback HTTP (the endpoints
/// [`super::validate::web_root`] accepts). `file`, `ssh`, `git` and `ext`
/// never: a hostile repository cannot reach local paths or run a command
/// through a transport.
fn allowed_protocols(authorization_url: Option<&str>) -> &'static str {
    let loopback_http = authorization_url
        .and_then(|url| url::Url::parse(url).ok())
        .is_some_and(|url| url.scheme() == "http" && octocode_github::is_loopback(&url));
    if loopback_http { "https:http" } else { "https" }
}

impl SystemGit {
    /// An isolated git invocation: no user or system config, no prompts or
    /// hooks, and the credential (when any) answered by `askpass` alone.
    fn command(
        &self,
        request: &GitRunRequest<'_>,
        git_home: &Path,
        askpass: Option<&AskPass>,
    ) -> Command {
        let protocols = allowed_protocols(request.authorization_url);
        #[cfg(test)]
        let protocols = if self.file_transport {
            format!("{protocols}:file")
        } else {
            protocols.to_owned()
        };
        let mut command = Command::new(&self.executable);
        command
            .env_clear()
            .env("HOME", git_home)
            .env("XDG_CONFIG_HOME", git_home)
            .env("GIT_CONFIG_GLOBAL", null_device())
            .env("GIT_CONFIG_SYSTEM", null_device())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "Never")
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .env("GIT_ALLOW_PROTOCOL", protocols)
            .arg("-c")
            .arg(format!("core.hooksPath={}", null_device()))
            .args(&request.args);
        command.envs(self.passthrough.iter().map(|(name, value)| (name, value)));
        if let Some(askpass) = askpass {
            // The helper on disk holds no credential: the token and the
            // prompts it may answer reach it through this child's
            // environment alone (never argv, never a file), inherited by
            // the git processes this one starts and gone when they exit.
            command
                .env("GIT_ASKPASS", &askpass.script)
                .envs(askpass.env());
        }
        command
    }
}

impl SystemGit {
    fn execute(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        records: Option<&mut RecordConsumer<'_>>,
    ) -> Result<GitOutput, CloneError> {
        control
            .cancellation
            .check()
            .map_err(|message| CloneError::new(ErrorCode::Cancelled, message))?;
        if Instant::now() >= control.deadline {
            return Err(CloneError::new(
                ErrorCode::Timeout,
                "Clone request deadline elapsed before Git execution.",
            ));
        }
        let git_tmp = control.home.join("tmp");
        let git_home = git_tmp.join("git-home");
        std::fs::create_dir_all(&git_home).map_err(|error| {
            CloneError::new(
                ErrorCode::GitFailed,
                format!("Failed to create isolated Git home: {error}"),
            )
        })?;
        let askpass = match (request.authorization, request.authorization_url) {
            (Some(token), Some(url)) => Some(AskPass::create(&git_tmp, token, url)?),
            _ => None,
        };
        let timeout = request
            .timeout
            .min(control.deadline.saturating_duration_since(Instant::now()));
        let command = self.command(request, &git_home, askpass.as_ref());
        let label = format!("git {}", request.label);
        let output = match records {
            Some(record) => run_bounded_records(
                command,
                &label,
                timeout,
                self.max_output,
                control.cancellation,
                &mut |line| record(line).map_err(|error| error.message),
            ),
            None => run_bounded(
                command,
                &label,
                timeout,
                self.max_output,
                control.cancellation,
            ),
        }
        .map_err(|error| match error {
            BoundedError::Cancelled(message) => CloneError::new(ErrorCode::Cancelled, message),
            BoundedError::TimedOut(_) => CloneError::new(
                ErrorCode::Timeout,
                format!("git {} exceeded its execution deadline", request.label),
            ),
            BoundedError::Failed(message) => process_failure(message),
        })?;
        drop(askpass);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = scrub(
            &String::from_utf8_lossy(&output.stderr),
            request.authorization,
            control.home,
        );
        if !output.status.success() {
            let suffix = if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr.trim())
            };
            return Err(process_failure(format!(
                "git {} failed{suffix}",
                request.label
            )));
        }
        Ok(GitOutput { stdout, stderr })
    }
}

impl GitRunner for SystemGit {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        self.execute(request, control, None)
    }

    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError> {
        self.execute(request, control, Some(record))
    }
}

fn process_failure(message: String) -> CloneError {
    CloneError {
        hints: vec!["Resolve the reported Git process or resource failure before retrying.".into()],
        ..CloneError::new(ErrorCode::GitFailed, message)
    }
}

#[cfg(windows)]
fn null_device() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
fn null_device() -> &'static str {
    "/dev/null"
}

#[cfg(test)]
mod tests;
