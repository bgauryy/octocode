//! A bounded helper subprocess: no stdin, capped stdout, a wall-clock limit,
//! cooperative cancellation, and (on Unix) its own process group so a
//! timeout or cancel also kills any grandchildren it started.
use super::cancel::CancellationCheck;
use std::{
    io::Read,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(crate) struct BoundedOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    /// The first [`STDERR_BYTES`] of stderr, for an error message.
    pub stderr: Vec<u8>,
}

const STDERR_BYTES: usize = 64 * 1024;
const POLL: Duration = Duration::from_millis(20);

/// Runs `command` to completion. `label` names it in errors.
pub(crate) fn run_bounded(
    mut command: Command,
    label: &str,
    timeout: Duration,
    max_stdout: usize,
    cancel: &dyn CancellationCheck,
) -> Result<BoundedOutput, String> {
    cancel.check()?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot run {label}: {error}"))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_reader = thread::spawn(move || read_capped(stdout, max_stdout));
    let stderr_reader = thread::spawn(move || read_capped(stderr, STDERR_BYTES));
    let started = Instant::now();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(error) => break Err(error.to_string()),
            Ok(None) => {}
        }
        if let Err(reason) = cancel.check() {
            break Err(reason);
        }
        if started.elapsed() >= timeout {
            break Err(format!(
                "{label} timed out after {}s",
                timeout.as_secs_f64()
            ));
        }
        thread::sleep(POLL);
    };
    let status = match outcome {
        Ok(status) => {
            // The leader exited; a grandchild holding a pipe open must not
            // keep the readers (and this call) waiting.
            kill_group(&mut child);
            status
        }
        Err(message) => {
            kill_group(&mut child);
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(message);
        }
    };
    let (stdout, exceeded) = stdout_reader
        .join()
        .map_err(|_| format!("{label} output reader failed"))??;
    let (stderr, _) = stderr_reader
        .join()
        .map_err(|_| format!("{label} error reader failed"))??;
    if exceeded {
        return Err(format!(
            "{label} exceeded the {} MiB output limit",
            max_stdout / (1024 * 1024)
        ));
    }
    Ok(BoundedOutput {
        status,
        stdout,
        stderr,
    })
}

/// Reads to EOF, keeping at most `cap` bytes; `true` when more arrived.
fn read_capped(source: Option<impl Read>, cap: usize) -> Result<(Vec<u8>, bool), String> {
    let Some(mut source) = source else {
        return Ok((Vec::new(), false));
    };
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    let mut exceeded = false;
    loop {
        let read = source.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok((bytes, exceeded));
        }
        let room = cap.saturating_sub(bytes.len());
        bytes.extend_from_slice(&chunk[..read.min(room)]);
        exceeded |= read > room;
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn kill_group(child: &mut Child) {
    if let Ok(pid) = i32::try_from(child.id()) {
        // SAFETY: the child leads its own process group (configure_process_group);
        // a negative pid signals that group only.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill_group(child: &mut Child) {
    let _ = child.kill();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::tools::cancel::NeverCancel;

    struct Cancelled;
    impl CancellationCheck for Cancelled {
        fn check(&self) -> Result<(), String> {
            Err("cancelled".into())
        }
    }

    fn sh(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn timeout_kills_the_whole_process_group() {
        let dir = tempfile::tempdir().expect("dir");
        let pidfile = dir.path().join("grandchild.pid");
        let script = format!("sleep 30 & echo $! > '{}'; wait", pidfile.display());
        let started = Instant::now();
        let error = run_bounded(
            sh(&script),
            "sh",
            Duration::from_millis(300),
            1024,
            &NeverCancel,
        )
        .err()
        .expect("timeout");
        assert!(error.contains("timed out"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
        let pid: u32 = std::fs::read_to_string(&pidfile)
            .expect("pid")
            .trim()
            .parse()
            .expect("pid number");
        thread::sleep(Duration::from_millis(100));
        assert!(
            !crate::process_status::is_alive(pid),
            "grandchild {pid} survived"
        );
    }

    #[test]
    fn output_over_the_cap_is_an_error_not_a_silent_cut() {
        let error = run_bounded(
            sh("head -c 4096 /dev/zero"),
            "sh",
            Duration::from_secs(5),
            1024,
            &NeverCancel,
        )
        .err()
        .expect("over cap");
        assert!(error.contains("output limit"), "{error}");
        let ok = run_bounded(
            sh("printf hi"),
            "sh",
            Duration::from_secs(5),
            1024,
            &NeverCancel,
        )
        .expect("runs");
        assert_eq!(ok.stdout, b"hi");
        assert!(ok.status.success());
    }

    #[test]
    fn cancellation_stops_before_spawn() {
        let error = run_bounded(
            sh("sleep 30"),
            "sh",
            Duration::from_secs(30),
            1024,
            &Cancelled,
        )
        .err()
        .expect("cancelled");
        assert_eq!(error, "cancelled");
    }
}
