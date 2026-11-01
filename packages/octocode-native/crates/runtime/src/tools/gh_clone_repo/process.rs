mod askpass;

use super::CloneError;
use crate::execution::cancel::CancellationCheck;
use crate::tools::bounded_process::{BoundedError, run_bounded, run_bounded_records};
use askpass::AskPass;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use octocode_contract_types::ErrorCode;
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub(super) const MAX_OUTPUT_BYTES: usize = 5 * 1024 * 1024;

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
            Ok(_) => Ok(()),
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
}

/// The header value git builds from the helper's answers: HTTP Basic with the
/// `x-access-token` user.
fn basic_credential(token: &str) -> String {
    STANDARD.encode(format!("x-access-token:{token}"))
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
            .arg("-c")
            .arg(format!("core.hooksPath={}", null_device()))
            .args(&request.args);
        command.envs(self.passthrough.iter().map(|(name, value)| (name, value)));
        if let Some(askpass) = askpass {
            // The path to a helper, never the token: the child's environment
            // and argv are readable by other processes of the same user.
            command.env("GIT_ASKPASS", &askpass.script);
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

fn scrub(text: &str, token: Option<&str>, home: &Path) -> String {
    let mut result = text.to_owned();
    if let Some(token) = token {
        // The base64 header value git builds first: it does not contain the
        // token, and git can echo it without the `Authorization:` prefix.
        result = result
            .replace(&basic_credential(token), "[REDACTED]")
            .replace(token, "[REDACTED]");
    }
    // A checkout stage is an internal path under the Octocode home; the
    // error names what failed, not where it was staged.
    let stages = [Some(home.to_path_buf()), home.canonicalize().ok()];
    for home in stages.into_iter().flatten() {
        let stage = home.join("tmp").join("clone-tmp");
        let pattern = format!(r"{}[^\s'\x22]*", regex::escape(&stage.to_string_lossy()));
        if let Ok(stage) = regex::Regex::new(&pattern) {
            result = stage.replace_all(&result, "<checkout>").into_owned();
        }
    }
    // Redact any authorization header regardless of scheme. Git sends the
    // helper's answers as `Authorization: Basic <base64(x-access-token:…)>`;
    // the base64 form does NOT contain the plaintext token, so this header
    // pattern — not the token replace above — stops a reversible credential
    // from leaking if git echoes a request.
    static AUTH: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        #[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
        regex::Regex::new(r"(?i)Authorization:\s*(Bearer|token|Basic)\s+\S+")
            .expect("static authorization regex")
    });
    AUTH.replace_all(&result, "Authorization: [REDACTED]")
        .into_owned()
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
