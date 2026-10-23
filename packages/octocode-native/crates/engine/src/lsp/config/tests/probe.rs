use super::*;

#[cfg(unix)]
#[test]
fn command_probe_is_bounded_and_kills_a_hung_command() {
    // System binaries, not freshly written scripts: exec'ing a file this
    // process just wrote races parallel tests' forks (ETXTBSY). Bounds are
    // generous so a loaded machine cannot turn scheduling delay into a
    // failure: the property is "returns long before the 30 s sleep ends".
    let started = std::time::Instant::now();
    assert!(!crate::lsp::config::probe::probe_command_succeeds(
        "/bin/sleep",
        &["30"],
        std::time::Duration::from_millis(200),
    ));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "a hung probe must be killed at its deadline"
    );
    assert!(crate::lsp::config::probe::probe_command_succeeds(
        "/bin/sh",
        &["-c", "exit 0"],
        std::time::Duration::from_secs(60),
    ));
    assert!(!crate::lsp::config::probe::probe_command_succeeds(
        "/bin/sh",
        &["-c", "exit 3"],
        std::time::Duration::from_secs(60),
    ));
    assert!(!crate::lsp::config::probe::probe_command_succeeds(
        "/nonexistent/octocode-probe-missing",
        &[],
        std::time::Duration::from_secs(1),
    ));
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
    assert!(!crate::lsp::config::probe::probe_command_succeeds(
        "/bin/sh",
        &["-c", &script],
        std::time::Duration::from_millis(1_500),
    ));
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
    assert!(!crate::lsp::config::probe::probe_command_succeeds(
        "python3",
        &["-c", script, &pid_arg],
        std::time::Duration::from_millis(1_500),
    ));
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
