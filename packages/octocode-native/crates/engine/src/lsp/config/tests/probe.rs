use super::*;
use crate::lsp::config::probe::{ProbeError, probe_command};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Run `command args..` with null stdio; `Ok` when it exited successfully.
/// See [`probe_command`]. Blocking: call from a blocking thread.
fn probe_command_succeeds(command: &str, args: &[&str]) -> Result<(), ProbeError> {
    let mut probe = std::process::Command::new(command);
    probe.args(args);
    probe_command(&mut probe, false).map(drop)
}

/// `probe` run by a caller that stops (its deadline passes) `after` from now.
fn caller_stopping_after<T>(after: std::time::Duration, probe: impl FnOnce() -> T) -> T {
    let stop = Arc::new(AtomicBool::new(false));
    let raise = Arc::clone(&stop);
    std::thread::spawn(move || {
        std::thread::sleep(after);
        raise.store(true, Ordering::Relaxed);
    });
    crate::deep_stack::while_stoppable(&stop, probe)
}

#[cfg(unix)]
#[test]
fn command_probe_ends_when_its_caller_stops_and_kills_a_hung_command() {
    // System binaries, not freshly written scripts. The property is "returns
    // long before the 30 s sleep ends", so scheduling delay cannot fail it.
    let started = std::time::Instant::now();
    let hung = caller_stopping_after(std::time::Duration::from_millis(200), || {
        probe_command_succeeds("/bin/sleep", &["30"])
    });
    assert!(matches!(hung, Err(ProbeError::Stopped)), "{hung:?}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "a hung probe must be killed when its caller stops"
    );
    assert!(probe_command_succeeds("/bin/sh", &["-c", "exit 0"]).is_ok());
    let failed = probe_command_succeeds("/bin/sh", &["-c", "exit 3"]);
    assert!(matches!(failed, Err(ProbeError::Exited(_))), "{failed:?}");
}

/// A program that cannot start is a typed spawn error, never "it answered no".
#[cfg(unix)]
#[test]
fn a_probe_that_cannot_start_says_so() {
    let missing = probe_command_succeeds("/nonexistent/octocode-probe-missing", &[]);
    assert!(
        matches!(&missing, Err(ProbeError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound),
        "{missing:?}"
    );
}

/// B13: under parallel load a valid `rustup which` took longer than the probe's
/// old fixed 3 s and was killed, read as "no component". A probe has no clock of
/// its own: one slower than any such bound still answers.
#[cfg(unix)]
#[test]
fn a_slow_probe_still_answers() {
    let slow = probe_command_succeeds("/bin/sh", &["-c", "sleep 4; exit 0"]);
    assert!(slow.is_ok(), "{slow:?}");
}

#[cfg(unix)]
#[test]
fn command_probe_timeout_kills_the_whole_process_group() {
    // A rustup-style proxy forks the real binary; killing only the
    // leader would leave that child running after the probe returns.
    let root = TmpDir::new();
    std::fs::create_dir_all(&root).expect("root");
    let pid_file = root.join("child.pid");
    let script = format!(
        "/bin/sleep 30 & echo $! > '{}'; wait",
        pid_file.to_string_lossy()
    );
    let stopped = caller_stopping_after(std::time::Duration::from_millis(1_500), || {
        probe_command_succeeds("/bin/sh", &["-c", &script])
    });
    assert!(matches!(stopped, Err(ProbeError::Stopped)), "{stopped:?}");
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .expect("the probe recorded its child pid")
        .trim()
        .parse()
        .expect("numeric pid");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    // SAFETY: signal 0 only checks whether the pid still exists.
    while unsafe { libc::kill(pid, 0) } == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the probe's grandchild outlived the timeout"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// A probe child that calls `setsid` leaves the process group; the
/// timeout's descendant sweep must still kill it.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn command_probe_timeout_kills_a_setsid_descendant() {
    let has_python = std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success());
    if !has_python {
        return;
    }
    let root = TmpDir::new();
    std::fs::create_dir_all(&root).expect("root");
    let pid_file = root.join("escaped.pid");
    let pid_arg = pid_file.to_string_lossy().into_owned();
    let script = "import os,sys,time\nif os.fork()==0:\n    os.setsid()\n    open(sys.argv[1],'w').write(str(os.getpid()))\n    time.sleep(60)\nelse:\n    time.sleep(60)\n";
    let stopped = caller_stopping_after(std::time::Duration::from_millis(1_500), || {
        probe_command_succeeds("python3", &["-c", script, &pid_arg])
    });
    assert!(matches!(stopped, Err(ProbeError::Stopped)), "{stopped:?}");
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .expect("the escaped child recorded its pid")
        .trim()
        .parse()
        .expect("numeric pid");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    // SAFETY: signal 0 only checks whether the pid still exists.
    while unsafe { libc::kill(pid, 0) } == 0 {
        if std::time::Instant::now() >= deadline {
            // SAFETY: clean up the leaked process before failing.
            unsafe { libc::kill(pid, libc::SIGKILL) };
            panic!("the setsid child outlived the probe timeout");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
