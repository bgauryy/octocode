use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::{
    MatchRange, REGEX_WORKER_PROTOCOL_VERSION, RegexError, RegexErrorCode, WorkerOperation,
    WorkerRequest, WorkerResponse, error,
};

#[derive(Clone, Copy, Debug)]
pub struct IsolatedRegexLimits {
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_matches: usize,
    pub deadline: Duration,
    pub max_memory_bytes: usize,
    pub max_cpu_seconds: u64,
}

impl Default for IsolatedRegexLimits {
    fn default() -> Self {
        Self {
            max_request_bytes: 64 * 1024,
            max_response_bytes: 256 * 1024,
            max_matches: 10_000,
            // This is a process-level budget: it includes spawn, resource attachment,
            // stdin/stdout transfer, and matching. A 200ms default was flaky under
            // concurrent CLI load even when the helper completed in single-digit
            // milliseconds. Explicit callers can still select a tighter bound.
            deadline: Duration::from_millis(1_000),
            max_memory_bytes: 128 * 1024 * 1024,
            max_cpu_seconds: 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct IsolatedRegexEngine {
    worker_path: PathBuf,
    limits: IsolatedRegexLimits,
    shutdown: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryConfinement {
    HardAddressSpace,
    SampledRss,
    WindowsJob,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionMetadata {
    pub memory_confinement: MemoryConfinement,
    pub peak_rss_bytes: Option<u64>,
    pub poll_interval: Duration,
}

impl IsolatedRegexEngine {
    pub fn new(worker_path: impl Into<PathBuf>, limits: IsolatedRegexLimits) -> Self {
        Self {
            worker_path: worker_path.into(),
            limits,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    pub fn worker_path(&self) -> &Path {
        &self.worker_path
    }

    pub fn find_ranges(
        &self,
        source: &str,
        flags: &str,
        input: &str,
    ) -> Result<Vec<MatchRange>, RegexError> {
        self.find_ranges_with_metadata(source, flags, input)
            .map(|(value, _)| value)
    }

    pub fn find_ranges_with_metadata(
        &self,
        source: &str,
        flags: &str,
        input: &str,
    ) -> Result<(Vec<MatchRange>, ExecutionMetadata), RegexError> {
        let (response, metadata) = self.execute(WorkerRequest {
            version: REGEX_WORKER_PROTOCOL_VERSION,
            source: source.to_owned(),
            flags: flags.to_owned(),
            input: input.to_owned(),
            operation: WorkerOperation::Find {
                max_matches: self.limits.max_matches,
            },
        })?;
        let ranges = response.ranges.ok_or_else(|| {
            error(
                RegexErrorCode::InvalidPattern,
                response
                    .error
                    .unwrap_or_else(|| "Regex worker omitted match ranges".into()),
            )
        })?;
        Ok((ranges, metadata))
    }

    pub fn replace_literal(
        &self,
        source: &str,
        flags: &str,
        input: &str,
        replacement: &str,
    ) -> Result<String, RegexError> {
        let (response, _) = self.execute(WorkerRequest {
            version: REGEX_WORKER_PROTOCOL_VERSION,
            source: source.to_owned(),
            flags: flags.to_owned(),
            input: input.to_owned(),
            operation: WorkerOperation::ReplaceLiteral {
                replacement: replacement.to_owned(),
            },
        })?;
        response.replaced.ok_or_else(|| {
            error(
                RegexErrorCode::InvalidPattern,
                response
                    .error
                    .unwrap_or_else(|| "Regex worker omitted replacement".into()),
            )
        })
    }

    fn execute(
        &self,
        request: WorkerRequest,
    ) -> Result<(WorkerResponse, ExecutionMetadata), RegexError> {
        if self.shutdown.load(Ordering::Acquire) {
            return Err(error(
                RegexErrorCode::RequiresIsolatedEngine,
                "Regex worker service is shut down",
            ));
        }
        let payload = serde_json::to_vec(&request)
            .map_err(|failure| error(RegexErrorCode::InvalidPattern, failure.to_string()))?;
        if payload.len() > self.limits.max_request_bytes {
            return Err(error(
                RegexErrorCode::InputTooLarge,
                "Regex worker request exceeds its byte limit",
            ));
        }
        let mut child = Command::new(&self.worker_path)
            .env(
                "OCTOCODE_REGEX_MAX_MEMORY_BYTES",
                self.limits.max_memory_bytes.to_string(),
            )
            .env(
                "OCTOCODE_REGEX_MAX_CPU_SECONDS",
                self.limits.max_cpu_seconds.to_string(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|failure| {
                error(
                    RegexErrorCode::RequiresIsolatedEngine,
                    format!("Unable to start regex worker: {failure}"),
                )
            })?;
        let mut stdin = child.stdin.take().ok_or_else(|| {
            error(
                RegexErrorCode::RequiresIsolatedEngine,
                "Regex worker stdin unavailable",
            )
        })?;
        let started = Instant::now();
        let input_writer = thread::spawn(move || stdin.write_all(&payload));
        let stdout = child.stdout.take().ok_or_else(|| {
            error(
                RegexErrorCode::RequiresIsolatedEngine,
                "Regex worker stdout unavailable",
            )
        })?;
        let output_limit = self.limits.max_response_bytes;
        let output_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout
                .take((output_limit + 1) as u64)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let mut peak_rss_bytes = None;
        loop {
            if self.shutdown.load(Ordering::Acquire) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error(
                    RegexErrorCode::RequiresIsolatedEngine,
                    "Regex worker cancelled during shutdown",
                ));
            }
            #[cfg(target_os = "macos")]
            {
                let rss = darwin_rss(child.id()).map_err(|failure| {
                    let _ = child.kill();
                    let _ = child.wait();
                    error(RegexErrorCode::RequiresIsolatedEngine, failure)
                })?;
                peak_rss_bytes = Some(peak_rss_bytes.map_or(rss, |peak: u64| peak.max(rss)));
                if rss > self.limits.max_memory_bytes as u64 {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error(
                        RegexErrorCode::RequiresIsolatedEngine,
                        format!("Regex worker exceeded sampled RSS limit (peak {rss} bytes)"),
                    ));
                }
            }
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() < self.limits.deadline => {
                    thread::sleep(Duration::from_millis(1))
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error(
                        RegexErrorCode::RequiresIsolatedEngine,
                        "Regex worker exceeded its execution deadline",
                    ));
                }
                Err(failure) => {
                    return Err(error(
                        RegexErrorCode::RequiresIsolatedEngine,
                        format!("Unable to wait for regex worker: {failure}"),
                    ));
                }
            }
        }
        input_writer
            .join()
            .map_err(|_| {
                error(
                    RegexErrorCode::RequiresIsolatedEngine,
                    "Regex worker input writer panicked",
                )
            })?
            .map_err(|failure| {
                error(
                    RegexErrorCode::RequiresIsolatedEngine,
                    format!("Unable to write regex worker request: {failure}"),
                )
            })?;
        let output = output_reader
            .join()
            .map_err(|_| {
                error(
                    RegexErrorCode::RequiresIsolatedEngine,
                    "Regex worker output reader panicked",
                )
            })?
            .map_err(|failure| {
                error(
                    RegexErrorCode::RequiresIsolatedEngine,
                    format!("Unable to read regex worker output: {failure}"),
                )
            })?;
        if output.len() > self.limits.max_response_bytes {
            return Err(error(
                RegexErrorCode::InputTooLarge,
                "Regex worker response exceeds its byte limit",
            ));
        }
        let response = serde_json::from_slice(&output).map_err(|failure| {
            error(
                RegexErrorCode::InvalidPattern,
                format!("Invalid regex worker response: {failure}"),
            )
        })?;
        Ok((
            response,
            ExecutionMetadata {
                memory_confinement: platform_memory_confinement(),
                peak_rss_bytes,
                poll_interval: Duration::from_millis(1),
            },
        ))
    }
}

const fn platform_memory_confinement() -> MemoryConfinement {
    #[cfg(target_os = "macos")]
    {
        MemoryConfinement::SampledRss
    }
    #[cfg(windows)]
    {
        MemoryConfinement::WindowsJob
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        MemoryConfinement::HardAddressSpace
    }
}

#[cfg(target_os = "macos")]
fn darwin_rss(pid: u32) -> Result<u64, String> {
    // SAFETY: proc_pid_rusage initializes the correctly sized V2 structure for
    // the child PID. A failed sample is fail-closed by the supervisor.
    let mut usage: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    let status = unsafe {
        libc::proc_pid_rusage(
            pid as libc::c_int,
            libc::RUSAGE_INFO_V2,
            (&raw mut usage).cast(),
        )
    };
    if status == 0 {
        Ok(usage.ri_phys_footprint.max(usage.ri_resident_size))
    } else {
        Err(format!(
            "Unable to sample regex worker RSS: {}",
            std::io::Error::last_os_error()
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn kills_and_reaps_a_worker_that_exceeds_the_hard_deadline() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let script = std::env::temp_dir().join(format!("octocode-regex-hang-{nonce}.sh"));
        std::fs::write(&script, "#!/bin/sh\ncat >/dev/null\nsleep 10\n")
            .expect("write worker fixture");
        let mut permissions = std::fs::metadata(&script)
            .expect("fixture metadata")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).expect("executable fixture");
        let engine = IsolatedRegexEngine::new(
            &script,
            IsolatedRegexLimits {
                deadline: Duration::from_millis(20),
                ..Default::default()
            },
        );
        let started = Instant::now();
        let failure = engine
            .find_ranges("(a+)+$", "", &format!("{}!", "a".repeat(10_000)))
            .expect_err("hung worker must be killed");
        assert_eq!(failure.code, RegexErrorCode::RequiresIsolatedEngine);
        assert!(started.elapsed() < Duration::from_secs(1));
        std::fs::remove_file(script).expect("remove worker fixture");
    }

    #[test]
    fn shutdown_rejects_future_helpers_without_spawning() {
        let engine = IsolatedRegexEngine::new("missing-worker", IsolatedRegexLimits::default());
        engine.shutdown();
        let failure = engine
            .find_ranges("x", "", "x")
            .expect_err("shutdown is terminal");
        assert_eq!(failure.code, RegexErrorCode::RequiresIsolatedEngine);
        assert!(failure.message.contains("shut down"));
    }
}
