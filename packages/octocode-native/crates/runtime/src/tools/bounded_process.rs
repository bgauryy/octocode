//! A bounded helper subprocess: no stdin, capped stdout, a wall-clock limit,
//! cooperative cancellation, and (on Unix) its own process group so a
//! timeout or cancel also kills any grandchildren it started.
//!
//! A descendant that left the group (`setsid`) and kept the output pipes open
//! cannot be found by a tree walk once its parent is gone. It never holds the
//! call: after the command ends, reads drain what is pending, stop at the next
//! quiet moment and drop the pipes (see [`gated::Gated`]).
use crate::execution::cancel::CancellationCheck;
use std::{
    process::{Command, ExitStatus, Stdio},
    sync::{Arc, OnceLock},
    thread,
    time::{Duration, Instant},
};

mod capture;
mod gated;

use capture::{read_capped, read_nul_records, read_stderr};
use gated::Gated;

/// Why a bounded run produced no output. The message names the command.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum BoundedError {
    /// The cancellation check fired, before the spawn or while waiting.
    Cancelled(String),
    /// The wall-clock limit passed; the process group was killed.
    TimedOut(String),
    /// The command could not start or be reaped, or overflowed its output.
    Failed(String),
}

impl From<BoundedError> for String {
    fn from(error: BoundedError) -> Self {
        match error {
            BoundedError::Cancelled(message)
            | BoundedError::TimedOut(message)
            | BoundedError::Failed(message) => message,
        }
    }
}

pub(crate) struct BoundedOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    /// The first [`STDERR_BYTES`] of stderr, for an error message. When
    /// stderr was cut short, a closing `[stderr …]` line says so (see
    /// [`capture::with_cut_note`]), so any message that quotes it carries the note.
    pub stderr: Vec<u8>,
}

const STDERR_BYTES: usize = 64 * 1024;
const POLL: Duration = Duration::from_millis(20);

/// Runs `command` to completion. `label` names it in errors.
pub(crate) fn run_bounded(
    command: Command,
    label: &str,
    timeout: Duration,
    max_stdout: usize,
    cancel: &dyn CancellationCheck,
) -> Result<BoundedOutput, BoundedError> {
    run_bounded_draining(command, label, timeout, max_stdout, DRAIN_GRACE, cancel)
}

/// [`run_bounded`] with stderr's drain grace named, so a test can end stderr
/// at once while stdout keeps [`DRAIN_GRACE`].
fn run_bounded_draining(
    command: Command,
    label: &str,
    timeout: Duration,
    max_stdout: usize,
    stderr_grace: Duration,
    cancel: &dyn CancellationCheck,
) -> Result<BoundedOutput, BoundedError> {
    let (status, (stdout, exceeded), stderr) = run_bounded_with_stdout(
        command,
        label,
        timeout,
        stderr_grace,
        cancel,
        move |stdout| read_capped(stdout, max_stdout),
    )?;
    if exceeded {
        return Err(BoundedError::Failed(format!(
            "{label} exceeded the {} MiB output limit",
            max_stdout / (1024 * 1024)
        )));
    }
    Ok(BoundedOutput {
        status,
        stdout,
        stderr,
    })
}

fn run_bounded_with_stdout<T: Send, F>(
    mut command: Command,
    label: &str,
    timeout: Duration,
    stderr_grace: Duration,
    cancel: &dyn CancellationCheck,
    reader: F,
) -> Result<(ExitStatus, T, Vec<u8>), BoundedError>
where
    F: FnOnce(Option<Gated<std::process::ChildStdout>>) -> Result<T, String> + Send,
{
    cancel.check().map_err(BoundedError::Cancelled)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    octocode_engine::process::contain(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| BoundedError::Failed(format!("cannot run {label}: {error}")))?;
    let mut tree = match octocode_engine::process::Job::attach(&child, None) {
        Ok(tree) => tree,
        Err(message) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BoundedError::Failed(format!(
                "cannot contain {label}: {message}"
            )));
        }
    };
    // Set once the command has ended: readers then drain what is pending and stop.
    let finished: Arc<OnceLock<Instant>> = Arc::new(OnceLock::new());
    let stdout = child
        .stdout
        .take()
        .map(|pipe| Gated::new(pipe, Arc::clone(&finished), DRAIN_GRACE));
    let stderr = child
        .stderr
        .take()
        .map(|pipe| Gated::new(pipe, Arc::clone(&finished), stderr_grace));
    let started = Instant::now();
    let (outcome, status, stdout, stderr) = thread::scope(|scope| {
        let stdout_reader = scope.spawn(|| reader(stdout));
        let stderr_reader = scope.spawn(|| read_stderr(stderr));
        let outcome = loop {
            #[cfg(unix)]
            let completed = octocode_engine::process::exited_without_reaping(child.id());
            #[cfg(not(unix))]
            let completed = child.try_wait().map(|status| status.is_some());
            match completed {
                Ok(true) => break Ok(()),
                Err(error) => {
                    break Err(BoundedError::Failed(format!(
                        "{label} could not be reaped: {error}"
                    )));
                }
                Ok(false) => {}
            }
            if let Err(reason) = cancel.check() {
                break Err(BoundedError::Cancelled(reason));
            }
            if started.elapsed() >= timeout {
                break Err(BoundedError::TimedOut(format!(
                    "{label} timed out after {}s",
                    timeout.as_secs_f64()
                )));
            }
            thread::sleep(POLL);
        };
        // Sweep while the leader is still unreaped: its pid cannot be reused.
        octocode_engine::process::kill_tree(child.id());
        tree.terminate();
        let _ = child.kill();
        let status = child.wait();
        let _ = finished.set(Instant::now());
        let reader_failed =
            |stream: &str| BoundedError::Failed(format!("{label} {stream} reader failed"));
        let stdout = stdout_reader
            .join()
            .map_err(|_| reader_failed("output"))
            .and_then(|value| {
                value.map_err(|message| BoundedError::Failed(format!("{label}: {message}")))
            });
        let stderr = stderr_reader.join().map_err(|_| reader_failed("error"));
        (outcome, status, stdout, stderr)
    });
    outcome?;
    let status = status
        .map_err(|error| BoundedError::Failed(format!("{label} could not be reaped: {error}")))?;
    Ok((status, stdout?, stderr?))
}

/// Runs `command` while delivering NUL-delimited stdout records to `record`.
///
/// The record cap deliberately reuses the caller's collected-output cap: a
/// malformed individual record remains bounded, while a legitimate index with
/// many records is not rejected for its aggregate size.
pub(crate) fn run_bounded_records(
    command: Command,
    label: &str,
    timeout: Duration,
    max_record: usize,
    cancel: &dyn CancellationCheck,
    record: &mut (dyn FnMut(&[u8]) -> Result<(), String> + Send),
) -> Result<BoundedOutput, BoundedError> {
    let (status, (), stderr) = run_bounded_with_stdout(
        command,
        label,
        timeout,
        DRAIN_GRACE,
        cancel,
        move |stdout| read_nul_records(stdout, max_record, record),
    )?;
    Ok(BoundedOutput {
        status,
        stdout: Vec::new(),
        stderr,
    })
}

/// How long readers may keep draining after the command ended. Only bytes the
/// command already wrote (at most one pipe buffer) are pending by then; a
/// detached descendant that keeps writing past this bound fails a read. That
/// fails the command when it is stdout; stderr ends there and says so (see
/// [`capture::with_cut_note`]).
const DRAIN_GRACE: Duration = Duration::from_secs(1);

#[cfg(all(test, unix))]
#[expect(
    clippy::expect_used,
    reason = "test module: clippy's allow-expect-in-tests does not see `cfg(all(test, unix))`"
)]
mod tests;

/// A detached descendant that keeps the output pipes open must not hold the
/// call (the fixture is described in `process_fixture`).
#[cfg(test)]
mod held_pipe_tests;

#[cfg(test)]
mod record_tests;
