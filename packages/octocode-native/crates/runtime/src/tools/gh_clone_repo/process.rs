use super::CloneError;
use crate::tools::local_fetch::CancellationCheck;
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

impl<'a> GitRunRequest<'a> {
    /// Preserve private auth transport while allowing an injected runner to
    /// rewrite only argv for a hermetic fixture.
    pub fn with_args(&self, args: Vec<OsString>) -> Self {
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
            Err(error)
                if matches!(
                    error.code.as_str(),
                    "clone.execution.cancelled" | "clone.execution.timeout"
                ) =>
            {
                Err(error)
            }
            Err(_) => Err(CloneError::new(
                "clone.git.unavailable",
                "git is not installed or not on PATH. The ghCloneRepo tool requires git to be available.",
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SystemGit {
    executable: PathBuf,
}

impl Default for SystemGit {
    fn default() -> Self {
        Self {
            executable: PathBuf::from("git"),
        }
    }
}

impl SystemGit {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
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
            .map_err(|message| CloneError::new("clone.execution.cancelled", message))?;
        if Instant::now() >= control.deadline {
            return Err(CloneError::new(
                "clone.execution.timeout",
                "Clone request deadline elapsed before Git execution.",
            ));
        }
        let git_home = control.cache_home.join("tmp").join("git-home");
        std::fs::create_dir_all(&git_home).map_err(|error| {
            CloneError::new(
                "clone.git.environment",
                format!("Failed to create isolated Git home: {error}"),
            )
        })?;
        let mut command = Command::new(&self.executable);
        configure_process_group(&mut command);
        command
            .env_clear()
            .env("HOME", &git_home)
            .env("XDG_CONFIG_HOME", &git_home)
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
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        #[cfg(windows)]
        if let Some(root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", root);
        }
        if let (Some(token), Some(url)) = (request.authorization, request.authorization_url) {
            command
                .env("GIT_CONFIG_COUNT", "1")
                .env("GIT_CONFIG_KEY_0", format!("http.{url}.extraHeader"))
                .env(
                    "GIT_CONFIG_VALUE_0",
                    format!("Authorization: Bearer {token}"),
                );
        }
        let mut child = command.spawn().map_err(|error| {
            CloneError::new(
                "clone.git.spawnFailed",
                format!("git {} failed to start: {error}", request.label),
            )
        })?;
        let mut process_tree = ProcessTreeGuard::attach(&child)?;
        let stdout = child.stdout.take().ok_or_else(|| {
            CloneError::new("clone.git.spawnFailed", "Git stdout pipe was unavailable")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            CloneError::new("clone.git.spawnFailed", "Git stderr pipe was unavailable")
        })?;
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
                return Err(CloneError::new("clone.execution.cancelled", message));
            }
            if started.elapsed() >= timeout || Instant::now() >= control.deadline {
                terminate_and_reap(&mut child, &mut process_tree);
                join_reader(stdout_reader)?;
                join_reader(stderr_reader)?;
                return Err(CloneError::new(
                    "clone.execution.timeout",
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
                        "clone.git.waitFailed",
                        format!("git {} could not be reaped: {error}", request.label),
                    ));
                }
            }
        };
        let (stdout, stdout_truncated) = join_reader(stdout_reader)?;
        let (stderr, stderr_truncated) = join_reader(stderr_reader)?;
        if stdout_truncated || stderr_truncated {
            return Err(CloneError::new(
                "clone.git.outputLimit",
                format!("git {} exceeded the 5MB output limit", request.label),
            ));
        }
        let stdout = String::from_utf8_lossy(&stdout).into_owned();
        let stderr = scrub(&String::from_utf8_lossy(&stderr), request.authorization);
        if !status.success() {
            let suffix = if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr.trim())
            };
            return Err(CloneError::new(
                "clone.git.failed",
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
        .map_err(|_| CloneError::new("clone.git.outputFailed", "Git output reader failed"))?
        .map_err(|error| {
            CloneError::new(
                "clone.git.outputFailed",
                format!("Git output could not be read: {error}"),
            )
        })
}

fn scrub(text: &str, token: Option<&str>) -> String {
    let mut result = text.to_owned();
    if let Some(token) = token {
        result = result.replace(token, "[REDACTED]");
    }
    let bearer = regex::Regex::new(r"(?i)Authorization:\s*(Bearer|token)\s+\S+")
        .expect("static authorization regex");
    bearer
        .replace_all(&result, "Authorization: Bearer [REDACTED]")
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
                "clone.git.spawnFailed",
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
                "clone.git.spawnFailed",
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
