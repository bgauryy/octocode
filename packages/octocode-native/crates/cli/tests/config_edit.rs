#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod support;
use support::Workspace;

#[test]
fn global_config_roundtrip_preserves_other_lines_and_hides_values() {
    let workspace = Workspace::new();
    let path = workspace.home.join(".env");
    std::fs::write(
        &path,
        "# keep me\r\nOTHER=leave-alone\r\nexport TEST_KEY=old\r\nTEST_KEY=duplicate\r\n",
    )
    .unwrap();
    let secret = " space 'quoted' # = value ";
    let add = workspace
        .cli()
        .args(["config", "--add", "TEST_KEY", secret, "--json"])
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    assert_eq!(result["changed"], true);
    assert!(!String::from_utf8_lossy(&add.stdout).contains(secret));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("# keep me\r\nOTHER=leave-alone\r\n"));
    assert_eq!(text.matches("TEST_KEY=").count(), 1);
    assert_eq!(
        octocode_native::config::parse_env(Some(&text))["TEST_KEY"],
        secret
    );
    let check = workspace
        .cli()
        .args(["config", "--check", "TEST_KEY", "--json"])
        .output()
        .unwrap();
    assert!(check.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&check.stdout).unwrap()["set"],
        true
    );
    let show = workspace
        .cli()
        .args(["showConfig", "--json"])
        .output()
        .unwrap();
    assert!(show.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&show.stdout).unwrap()["path"],
        path.to_string_lossy().as_ref()
    );
    for expected in [true, false] {
        let remove = workspace
            .cli()
            .args(["config", "--remove", "TEST_KEY", "--json"])
            .output()
            .unwrap();
        assert!(remove.status.success());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&remove.stdout).unwrap()["changed"],
            expected
        );
    }
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "# keep me\r\nOTHER=leave-alone\r\n"
    );
    let unset = workspace
        .cli()
        .args(["config", "--check", "TEST_KEY", "--json"])
        .output()
        .unwrap();
    assert_eq!(unset.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&unset.stdout).unwrap()["set"],
        false
    );
}

#[test]
fn config_rejects_invalid_edits_without_changing_the_file() {
    let workspace = Workspace::new();
    let path = workspace.home.join(".env");
    std::fs::write(&path, "KEEP=original\n").unwrap();
    for args in [
        vec!["config", "--add", "BAD-KEY", "value"],
        vec!["config", "--add", "GOOD", "bad\nINJECTED=true"],
        vec!["config", "--add", "PATH", "/tmp"],
        vec!["config", "--add", "GOOD"],
        vec!["config", "--add", "GOOD", "v", "--remove", "KEEP"],
        vec!["config", "--remove", "KEEP", "--check", "KEEP"],
    ] {
        let output = workspace
            .cli()
            .args(args)
            .arg("--json-errors")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "KEEP=original\n");
    }
}

#[test]
fn config_stdin_creates_private_global_file() {
    use std::io::Write;
    use std::process::Stdio;
    let workspace = Workspace::new();
    let mut child = workspace
        .cli()
        .args(["config", "--add", "TEST_KEY", "--value-stdin", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"secret with spaces\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret"));
    let path = workspace.home.join(".env");
    assert_eq!(
        octocode_native::config::parse_env(Some(&std::fs::read_to_string(&path).unwrap()))["TEST_KEY"],
        "secret with spaces"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[cfg(unix)]
#[test]
fn config_refuses_symlinks_and_concurrent_edits() {
    let workspace = Workspace::new();
    let target = workspace.workspace.join("target.env");
    std::fs::write(&target, "KEEP=original\n").unwrap();
    let path = workspace.home.join(".env");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let output = workspace
        .cli()
        .args(["config", "--add", "TEST_KEY", "value"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "KEEP=original\n");
    std::fs::remove_file(&path).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(workspace.home.join(".env.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let output = workspace
        .cli()
        .args(["config", "--add", "TEST_KEY", "value"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(!path.exists());
}

#[test]
fn json_auth_failures_exit_nonzero() {
    let workspace = Workspace::new();
    for args in [
        vec![
            "auth",
            "login",
            "--hostname",
            "audit.example.invalid",
            "--json",
        ],
        vec!["auth", "login", "--force", "--json"],
    ] {
        let output = workspace
            .cli()
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["success"],
            false
        );
    }
}
