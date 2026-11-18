#[test]
fn cargo_metadata_runs_the_configured_cargo_only() {
    let root = tempfile::tempdir().expect("fixture");
    std::fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
    )
    .expect("manifest");
    // The configured path is used as-is: no ambient `CARGO` fallback.
    let missing = root.path().join("no-such-cargo");
    let (_, failures) = super::load_cargo_crates(
        root.path(),
        &[root.path().join("Cargo.toml")].into(),
        missing.to_str(),
        &octocode_runtime_base::execution::cancel::NeverCancel,
    );
    assert_eq!(failures.len(), 1, "a missing configured cargo cannot run");
}

/// A cancelled build stops `cargo metadata` as cancelled, and its process
/// group dies with it. The test's limit is a minute: the first exec of a
/// new script can wait seconds on a busy macOS host (executable scans run
/// one at a time), so only the cancellation may end this run.
#[cfg(unix)]
#[test]
fn cancellation_stops_cargo_metadata_and_its_children() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;
    /// Cancels once the fake cargo has started its child.
    struct CancelOnceStarted(std::path::PathBuf);
    impl octocode_runtime_base::execution::cancel::CancellationCheck for CancelOnceStarted {
        fn check(&self) -> Result<(), String> {
            let started = std::fs::read_to_string(&self.0).is_ok_and(|pid| pid.ends_with('\n'));
            if started {
                Err("cancelled".into())
            } else {
                Ok(())
            }
        }
    }
    let root = tempfile::tempdir().expect("fixture");
    let pidfile = root.path().join("child.pid");
    let cargo = root.path().join("fake-cargo");
    std::fs::write(
        &cargo,
        format!(
            "#!/bin/sh\nsleep 30 & echo $! > '{}'\nwait\n",
            pidfile.display()
        ),
    )
    .expect("fake cargo");
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let cancel = CancelOnceStarted(pidfile.clone());
    let result = super::metadata(
        &root.path().join("Cargo.toml"),
        cargo.to_str(),
        &cancel,
        Duration::from_secs(60),
    );
    assert_eq!(result.err().as_deref(), Some("cancelled"));
    let pid: u32 = std::fs::read_to_string(&pidfile)
        .expect("pid")
        .trim()
        .parse()
        .expect("pid number");
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !octocode_runtime_base::process_status::is_alive(pid),
        "child {pid} survived"
    );
}
