use super::CloneError;
use crate::tools::cancel::CancellationCheck;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_OUTPUT_BYTES: usize = 5 * 1024 * 1024;

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
    pub cache_home: &'a Path,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
}

pub trait GitRunner: Send + Sync {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
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
            Err(error) if matches!(error.code.as_str(), "cancelled" | "timeout") => Err(error),
            Err(_) => Err(CloneError::new(
                "gitUnavailable",
                "git is not installed or not on PATH. The ghCloneRepo tool requires git to be available.",
            )),
        }
    }
}

/// Runtime environment names the isolated git keeps: its PATH plus the proxy
/// and trust-store settings a corporate network needs. Nothing that weakens
/// TLS (`GIT_SSL_NO_VERIFY`) or carries a credential passes.
const PASSTHROUGH_ENV: &[&str] = &[
    "PATH",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "GIT_SSL_CAINFO",
    "GIT_SSL_CAPATH",
    "CURL_CA_BUNDLE",
    #[cfg(windows)]
    "SystemRoot",
];

#[derive(Clone, Debug)]
pub struct SystemGit {
    executable: PathBuf,
    /// [`PASSTHROUGH_ENV`] values, read once from the runtime environment.
    passthrough: Vec<(String, OsString)>,
}

impl Default for SystemGit {
    /// The process environment, for callers without a runtime env map.
    fn default() -> Self {
        Self {
            executable: PathBuf::from("git"),
            passthrough: PASSTHROUGH_ENV
                .iter()
                .filter_map(|name| Some(((*name).to_owned(), std::env::var_os(name)?)))
                .collect(),
        }
    }
}

impl SystemGit {
    /// Git with the [`PASSTHROUGH_ENV`] subset of the runtime `env` (the
    /// resolved config env, not the process's). PATH falls back to the
    /// process's when `env` has none, so git can still be found.
    pub fn with_env(env: &std::collections::BTreeMap<String, String>) -> Self {
        let mut passthrough: Vec<(String, OsString)> = PASSTHROUGH_ENV
            .iter()
            .filter_map(|name| {
                let value = env.get(*name).filter(|value| !value.is_empty())?;
                Some(((*name).to_owned(), OsString::from(value)))
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
        }
    }

    #[cfg(test)]
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            ..Self::default()
        }
    }
}

/// The header value git sends: HTTP Basic with the `x-access-token` user.
fn basic_credential(token: &str) -> String {
    STANDARD.encode(format!("x-access-token:{token}"))
}

impl SystemGit {
    /// An isolated git invocation: no user or system config, no prompts or
    /// hooks, and the credential (when any) as an HTTP header for one URL.
    fn command(&self, request: &GitRunRequest<'_>, git_home: &Path) -> Command {
        let mut command = Command::new(&self.executable);
        configure_process_group(&mut command);
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
            .args(&request.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.envs(self.passthrough.iter().map(|(name, value)| (name, value)));
        if let (Some(token), Some(url)) = (request.authorization, request.authorization_url) {
            // Use HTTP Basic with the `x-access-token` username instead of
            // `Bearer`: GitHub's git-over-HTTPS accepts Basic for every token
            // class (classic / fine-grained PAT AND `gho_` OAuth), whereas a
            // `Bearer gho_…` header 401s — and with prompts disabled that fails
            // the clone even for a public repo that would succeed anonymously.
            let basic = basic_credential(token);
            command
                .env("GIT_CONFIG_COUNT", "1")
                .env("GIT_CONFIG_KEY_0", format!("http.{url}.extraHeader"))
                .env(
                    "GIT_CONFIG_VALUE_0",
                    format!("Authorization: Basic {basic}"),
                );
        }
        command
    }
}

impl GitRunner for SystemGit {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        control
            .cancellation
            .check()
            .map_err(|message| CloneError::new("cancelled", message))?;
        if Instant::now() >= control.deadline {
            return Err(CloneError::new(
                "timeout",
                "Clone request deadline elapsed before Git execution.",
            ));
        }
        let git_home = control.cache_home.join("tmp").join("git-home");
        std::fs::create_dir_all(&git_home).map_err(|error| {
            CloneError::new(
                "gitFailed",
                format!("Failed to create isolated Git home: {error}"),
            )
        })?;
        let mut command = self.command(request, &git_home);
        let mut child = command.spawn().map_err(|error| {
            CloneError::new(
                "gitFailed",
                format!("git {} failed to start: {error}", request.label),
            )
        })?;
        let mut process_tree = ProcessTreeGuard::attach(&child)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CloneError::new("gitFailed", "Git stdout pipe was unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CloneError::new("gitFailed", "Git stderr pipe was unavailable"))?;
        let stdout_reader = thread::spawn(move || read_bounded(stdout));
        let stderr_reader = thread::spawn(move || read_bounded(stderr));
        let started = Instant::now();
        let timeout = request
            .timeout
            .min(control.deadline.saturating_duration_since(started));
        let status = loop {
            if let Err(message) = control.cancellation.check() {
                terminate_and_reap(&mut child, &mut process_tree);
                join_reader(stdout_reader)?;
                join_reader(stderr_reader)?;
                return Err(CloneError::new("cancelled", message));
            }
            if started.elapsed() >= timeout || Instant::now() >= control.deadline {
                terminate_and_reap(&mut child, &mut process_tree);
                join_reader(stdout_reader)?;
                join_reader(stderr_reader)?;
                return Err(CloneError::new(
                    "timeout",
                    format!("git {} exceeded its execution deadline", request.label),
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(error) => {
                    terminate_and_reap(&mut child, &mut process_tree);
                    join_reader(stdout_reader)?;
                    join_reader(stderr_reader)?;
                    return Err(CloneError::new(
                        "gitFailed",
                        format!("git {} could not be reaped: {error}", request.label),
                    ));
                }
            }
        };
        let (stdout, stdout_truncated) = join_reader(stdout_reader)?;
        let (stderr, stderr_truncated) = join_reader(stderr_reader)?;
        if stdout_truncated || stderr_truncated {
            return Err(CloneError::new(
                "gitFailed",
                format!("git {} exceeded the 5MB output limit", request.label),
            ));
        }
        let stdout = String::from_utf8_lossy(&stdout).into_owned();
        let stderr = scrub(
            &String::from_utf8_lossy(&stderr),
            request.authorization,
            control.cache_home,
        );
        if !status.success() {
            let suffix = if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr.trim())
            };
            return Err(CloneError::new(
                "gitFailed",
                format!("git {} failed{suffix}", request.label),
            ));
        }
        Ok(GitOutput { stdout, stderr })
    }
}

fn read_bounded(mut reader: impl Read) -> io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = MAX_OUTPUT_BYTES.saturating_sub(output.len());
        output.extend_from_slice(&buffer[..read.min(remaining)]);
        truncated |= read > remaining;
    }
    Ok((output, truncated))
}

fn join_reader(
    reader: thread::JoinHandle<io::Result<(Vec<u8>, bool)>>,
) -> Result<(Vec<u8>, bool), CloneError> {
    reader
        .join()
        .map_err(|_| CloneError::new("gitFailed", "Git output reader failed"))?
        .map_err(|error| {
            CloneError::new(
                "gitFailed",
                format!("Git output could not be read: {error}"),
            )
        })
}

fn scrub(text: &str, token: Option<&str>, home: &Path) -> String {
    let mut result = text.to_owned();
    if let Some(token) = token {
        // The base64 header value first: it does not contain the token, and
        // git can echo it without the `Authorization:` prefix.
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
    // Redact the injected credential regardless of scheme. The clone token is
    // sent as `Authorization: Basic <base64(x-access-token:…)>`; the base64 form
    // does NOT contain the plaintext token, so this header pattern — not the
    // token replace above — is what stops a reversible credential from leaking
    // if git echoes its environment.
    static AUTH: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        #[allow(clippy::expect_used)]
        regex::Regex::new(r"(?i)Authorization:\s*(Bearer|token|Basic)\s+\S+")
            .expect("static authorization regex")
    });
    AUTH.replace_all(&result, "Authorization: [REDACTED]")
        .into_owned()
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_and_reap(child: &mut Child, process_tree: &mut ProcessTreeGuard) {
    // SAFETY: the child was started as process-group leader; negative pid targets that group.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    process_tree.terminate();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_and_reap(child: &mut Child, process_tree: &mut ProcessTreeGuard) {
    process_tree.terminate();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(windows))]
struct ProcessTreeGuard;

#[cfg(not(windows))]
impl ProcessTreeGuard {
    fn attach(_child: &Child) -> Result<Self, CloneError> {
        Ok(Self)
    }

    fn terminate(&mut self) {}
}

#[cfg(windows)]
struct ProcessTreeGuard(Option<windows_sys::Win32::Foundation::HANDLE>);

#[cfg(windows)]
impl ProcessTreeGuard {
    fn attach(child: &Child) -> Result<Self, CloneError> {
        use std::mem::{size_of, zeroed};
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        // SAFETY: null security/name creates an unnamed job owned by the returned handle.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(CloneError::new(
                "gitFailed",
                format!(
                    "failed to create Windows job object: {}",
                    io::Error::last_os_error()
                ),
            ));
        }
        // SAFETY: the Windows structure is plain data and zero is its documented baseline.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: job is valid and info points to the correct structure for this information class.
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } != 0;
        // SAFETY: child owns a valid process handle while Child is alive.
        let assigned =
            configured && unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as _) } != 0;
        if !assigned {
            // SAFETY: job is owned by this function.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err(CloneError::new(
                "gitFailed",
                format!(
                    "failed to contain Git in a Windows job object: {}",
                    io::Error::last_os_error()
                ),
            ));
        }
        Ok(Self(Some(job)))
    }

    fn terminate(&mut self) {
        if let Some(job) = self.0.take() {
            // Closing a kill-on-close job terminates every descendant before pipe readers join.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessTreeGuard {
    fn drop(&mut self) {
        self.terminate();
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
mod env_and_scrub_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn envs(command: &Command) -> BTreeMap<String, Option<String>> {
        command
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    fn request() -> GitRunRequest<'static> {
        GitRunRequest {
            args: vec![OsString::from("--version")],
            timeout: Duration::from_secs(1),
            label: "test".into(),
            authorization: None,
            authorization_url: None,
        }
    }

    /// M8: the isolated git keeps the runtime's proxy and CA settings (a
    /// corporate network needs them) and its PATH, but nothing else.
    #[test]
    fn isolated_git_keeps_runtime_proxy_ca_and_path_only() {
        let runtime_env = BTreeMap::from(
            [
                ("HTTPS_PROXY", "http://proxy.corp:3128"),
                ("no_proxy", "internal.corp"),
                ("SSL_CERT_FILE", "/etc/corp-ca.pem"),
                ("GIT_SSL_CAINFO", "/etc/corp-ca.pem"),
                ("PATH", "/runtime/bin"),
                ("GIT_SSL_NO_VERIFY", "1"),
                ("GITHUB_TOKEN", "ghp_secret"),
            ]
            .map(|(key, value)| (key.to_owned(), value.to_owned())),
        );
        let git = SystemGit::with_env(&runtime_env);
        let envs = envs(&git.command(&request(), Path::new("/tmp/git-home")));
        let get = |key: &str| envs.get(key).cloned().flatten();
        assert_eq!(
            get("HTTPS_PROXY").as_deref(),
            Some("http://proxy.corp:3128")
        );
        assert_eq!(get("no_proxy").as_deref(), Some("internal.corp"));
        assert_eq!(get("SSL_CERT_FILE").as_deref(), Some("/etc/corp-ca.pem"));
        assert_eq!(get("GIT_SSL_CAINFO").as_deref(), Some("/etc/corp-ca.pem"));
        assert_eq!(get("PATH").as_deref(), Some("/runtime/bin"));
        assert_eq!(get("GIT_SSL_NO_VERIFY"), None);
        assert_eq!(get("GITHUB_TOKEN"), None);
    }

    /// L7: git may echo the injected header value without its
    /// `Authorization:` prefix; the bare base64 form is reversible.
    #[test]
    fn scrub_redacts_the_bare_basic_credential() {
        let basic = basic_credential("ghp_token123");
        let text = format!("fatal: config value {basic} rejected; token ghp_token123");
        let scrubbed = scrub(&text, Some("ghp_token123"), Path::new("/nonexistent-home"));
        assert!(!scrubbed.contains(&basic), "{scrubbed}");
        assert!(!scrubbed.contains("ghp_token123"), "{scrubbed}");
    }
}
