use super::*;
use crate::execution::cancel::NeverCancel;
use crate::process_fixture::{RETURN_SLACK, gone_within, recorded_pid, until_set_up};

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

/// Short timeouts first; a run whose fixture did not start its
/// descendant in time is repeated with the next ([`until_set_up`]).
const SET_UP_TIMEOUTS: [Duration; 3] = [
    Duration::from_millis(300),
    Duration::from_millis(1200),
    Duration::from_millis(4800),
];

#[test]
fn timeout_kills_the_whole_process_group() {
    let dir = tempfile::tempdir().expect("dir");
    let pidfile = dir.path().join("grandchild.pid");
    let script = format!("sleep 30 & echo $! > '{}'; wait", pidfile.display());
    let (result, timeout, elapsed, pid) = until_set_up(&SET_UP_TIMEOUTS, |timeout| {
        let started = Instant::now();
        let result = run_bounded(sh(&script), "sh", timeout, 1024, &NeverCancel);
        let elapsed = started.elapsed();
        Some((result, timeout, elapsed, recorded_pid(&pidfile)?))
    });
    let error = result.err().expect("timeout");
    assert!(
        matches!(&error, BoundedError::TimedOut(message) if message.contains("timed out")),
        "{error:?}"
    );
    assert!(elapsed < timeout + RETURN_SLACK, "{elapsed:?}");
    // A killed grandchild stays visible until its new parent reaps it.
    assert!(
        gone_within(pid, Duration::from_secs(5)),
        "grandchild {pid} survived"
    );
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn timeout_kills_a_setsid_descendant() {
    let dir = tempfile::tempdir().expect("dir");
    let pidfile = dir.path().join("escaped.pid");
    let (result, pid) = until_set_up(&SET_UP_TIMEOUTS, |timeout| {
        let mut command = Command::new("python3");
        command.args(["-c", "import os,sys,time\nif os.fork()==0:\n os.setsid()\n os.close(1); os.close(2)\n open(sys.argv[1],'w').write(str(os.getpid()))\n time.sleep(60)\nelse:\n time.sleep(60)\n"]);
        command.arg(&pidfile);
        let result = run_bounded(command, "python3", timeout, 1024, &NeverCancel);
        Some((result, recorded_pid(&pidfile)?))
    });
    let gone = gone_within(pid, Duration::from_secs(5));
    if !gone {
        crate::process_fixture::kill(pid);
    }
    assert!(matches!(result, Err(BoundedError::TimedOut(_))));
    assert!(gone, "setsid descendant {pid} survived timeout");
}

/// The leader prints `boom` to stderr and `hi` to stdout, then exits with
/// `code`. A grandchild keeps stderr open until the read end closes; it
/// has left the process group before the leader goes on, so the leader's
/// end cannot take it down. Stdout is closed before it is forked, so the
/// leader is stdout's last writer.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn stderr_holder(code: i32) -> Command {
    let mut command = Command::new("python3");
    command.args([
        "-c",
        "import os,sys,time\nsys.stderr.write('boom\\n'); sys.stderr.flush()\nmiddle=os.fork()\nif middle==0:\n os.close(1)\n r,w=os.pipe()\n if os.fork()==0:\n  os.setsid(); os.close(r); os.write(w,b'x'); os.close(w)\n  try:\n   while True: os.write(2,b'.'); time.sleep(0.05)\n  except OSError: pass\n  os._exit(0)\n os.close(w); os.read(r,1)\n os._exit(0)\nos.waitpid(middle,0)\nsys.stdout.write('hi'); sys.stdout.flush()\nos._exit(int(sys.argv[1]))\n",
    ]);
    command.arg(code.to_string());
    command
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
/// The held-stderr fixture's output with a zero stderr grace, the time
/// the call took and its timeout. A run the timeout ended before the
/// leader exited ran under too much load to test anything, and is
/// repeated with a longer timeout ([`until_set_up`]).
fn run_with_held_stderr(code: i32) -> (BoundedOutput, Duration, Duration) {
    let timeouts = [5, 20].map(Duration::from_secs);
    let (result, elapsed, timeout) = until_set_up(&timeouts, |timeout| {
        let started = Instant::now();
        let result = run_bounded_draining(
            stderr_holder(code),
            "python3",
            timeout,
            1024,
            Duration::ZERO,
            &NeverCancel,
        );
        let elapsed = started.elapsed();
        (!matches!(result, Err(BoundedError::TimedOut(_)))).then_some((result, elapsed, timeout))
    });
    let output = result.expect("stdout is complete, so the command's own result stands");
    (output, elapsed, timeout)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_success_is_not_failed_by_stderr_still_open_at_the_grace() {
    let (output, elapsed, timeout) = run_with_held_stderr(0);
    assert!(
        elapsed < timeout + RETURN_SLACK,
        "returned after {elapsed:?}"
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"hi");
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_failure_quotes_the_stderr_it_has_and_says_it_was_cut() {
    let (output, ..) = run_with_held_stderr(3);
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.starts_with("boom\n"), "{stderr}");
    assert!(
        stderr
            .trim_end()
            .ends_with("[stderr cut: output kept streaming after the command ended]"),
        "{stderr}"
    );
}

#[test]
fn stderr_over_its_cap_says_it_was_truncated() {
    let output = run_bounded(
        sh("head -c 100000 /dev/zero >&2; exit 2"),
        "sh",
        Duration::from_secs(5),
        1024,
        &NeverCancel,
    )
    .expect("runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .trim_end()
            .ends_with("[stderr truncated at 65536 bytes]"),
        "{}",
        &stderr[stderr.len().saturating_sub(80)..]
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
    assert!(
        matches!(&error, BoundedError::Failed(message) if message.contains("output limit")),
        "{error:?}"
    );
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
fn nul_records_can_exceed_the_collected_output_cap_in_total() {
    let mut records = 0_usize;
    // 300,000 records, 5.1 MB in all; `yes` writes them in a fraction of
    // a second, where a shell loop took seconds and, under load, the
    // whole 15 s timeout.
    let output = run_bounded_records(
        sh("yes '120000 blob\tlink' | head -n 300000 | tr '\\n' '\\0'"),
        "sh",
        Duration::from_secs(15),
        1024,
        &NeverCancel,
        &mut |line| {
            assert!(line.starts_with(b"120000 blob"));
            records += 1;
            Ok(())
        },
    )
    .expect("streams records");
    assert_eq!(records, 300000);
    assert!(output.status.success());
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
    assert_eq!(error, BoundedError::Cancelled("cancelled".into()));
}
