use super::*;

#[test]
fn global_config_roundtrip_preserves_other_lines_and_hides_values() {
    let workspace = Workspace::new();
    let path = workspace.home.join(".env");
    std::fs::write(
        &path,
        "# keep me\r\nOTHER=leave-alone\r\nexport TEST_SETTING=old\r\nTEST_SETTING=duplicate\r\n",
    )
    .unwrap();
    let secret = " space 'quoted' # = value ";
    let add = workspace
        .cli()
        .args(["config", "set", "TEST_SETTING", secret, "--json"])
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
    assert_eq!(text.matches("TEST_SETTING=").count(), 1);
    assert_eq!(
        octocode_native::config::parse_env(Some(&text))["TEST_SETTING"],
        secret
    );
    let get = workspace
        .cli()
        .args(["config", "get", "TEST_SETTING"])
        .output()
        .unwrap();
    assert!(get.status.success());
    assert_eq!(String::from_utf8_lossy(&get.stdout), format!("{secret}\n"));
    let get_json = workspace
        .cli()
        .args(["config", "get", "TEST_SETTING", "--json"])
        .output()
        .unwrap();
    let got = serde_json::from_slice::<serde_json::Value>(&get_json.stdout).unwrap();
    assert_eq!(got["value"], secret);
    assert_eq!(got["source"], "home");
    assert_eq!(got["path"], path.to_string_lossy().as_ref());
    let home = workspace.cli().args(["config", "home"]).output().unwrap();
    assert!(home.status.success());
    assert_eq!(
        String::from_utf8_lossy(&home.stdout).trim_end(),
        workspace.home.to_string_lossy()
    );
    let check = workspace
        .cli()
        .args(["config", "check", "TEST_SETTING", "--json"])
        .output()
        .unwrap();
    assert!(check.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&check.stdout).unwrap()["set"],
        true
    );
    let show = workspace.cli().args(["config", "--json"]).output().unwrap();
    assert!(show.status.success());
    let shown = serde_json::from_slice::<serde_json::Value>(&show.stdout).unwrap();
    assert_eq!(
        shown["envFiles"]["global"]["path"],
        path.to_string_lossy().as_ref()
    );
    assert_eq!(shown["envFiles"]["global"]["exists"], true);
    assert!(!String::from_utf8_lossy(&show.stdout).contains(secret));
    for expected in [true, false] {
        let remove = workspace
            .cli()
            .args(["config", "unset", "TEST_SETTING", "--json"])
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
        .args(["config", "check", "TEST_SETTING", "--json"])
        .output()
        .unwrap();
    assert_eq!(unset.status.code(), Some(1));
    let missing = workspace
        .cli()
        .args(["config", "get", "TEST_SETTING"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(missing.stdout.is_empty());
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
        vec!["config", "set", "BAD-KEY", "value"],
        vec!["config", "set", "GOOD", "bad\nINJECTED=true"],
        vec!["config", "set", "PATH", "/tmp"],
        vec!["config", "set", "GOOD"],
        vec!["config", "set", "GOOD", "v", "--stdin"],
        vec!["config", "set", "GOOD", "v", "extra"],
        vec!["config", "unset", "KEEP", "extra"],
    ] {
        let output = workspace.cli().args(args).arg("--json").output().unwrap();
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
        .args(["config", "set", "TEST_KEY", "--stdin", "--json"])
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
        .args(["config", "set", "TEST_KEY", "value"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "KEEP=original\n");
    std::fs::remove_file(&path).unwrap();
    // A refused edit need not leave the lock behind; hold it as a concurrent editor would.
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(workspace.home.join(".env.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    let output = workspace
        .cli()
        .args(["config", "set", "TEST_KEY", "value"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(!path.exists());
}

/// `config get` masks secrets and says where the value lives, so a person or
/// agent can read it there; other values print in clear.
#[test]
fn config_get_masks_secrets_and_names_their_source() {
    let workspace = Workspace::new();
    let secret = "ghp_synthetic000000000000000000000000000";
    let home_env = workspace.home.join(".env");
    std::fs::write(
        &home_env,
        format!("GH_TOKEN={secret}\nPLAIN_SETTING=visible\n"),
    )
    .unwrap();
    let project_env = workspace.workspace.join(".octocode/.env");
    std::fs::create_dir_all(project_env.parent().unwrap()).unwrap();
    std::fs::write(&project_env, "SERVICE_API_KEY=workspace-secret-value\n").unwrap();
    std::fs::write(
        workspace.home.join(".octocoderc"),
        r#"{"classification":{"api":"file-secret-value"}}"#,
    )
    .unwrap();
    let get = |key: &str, json: bool, env: Option<(&str, &str)>| {
        let mut command = workspace.cli();
        command.args(["config", "get", key]);
        if json {
            command.arg("--json");
        }
        if let Some((name, value)) = env {
            command.env(name, value);
        }
        command.output().unwrap()
    };
    let as_json = |output: &std::process::Output| {
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };

    let token = get("GH_TOKEN", true, None);
    assert!(token.status.success());
    let token = as_json(&token);
    assert_eq!(token["value"], "[REDACTED-GH_TOKEN]");
    assert_eq!(token["source"], "home");
    assert_eq!(token["path"], home_env.to_string_lossy().as_ref());
    assert_eq!(token["set"], true);

    let text = get("GH_TOKEN", false, None);
    assert!(text.status.success());
    let line = String::from_utf8_lossy(&text.stdout);
    assert!(!line.contains(secret), "{line}");
    assert_eq!(
        line,
        format!("[REDACTED-GH_TOKEN] (home: {})\n", home_env.display())
    );

    let workspace_key = as_json(&get("SERVICE_API_KEY", true, None));
    assert_eq!(workspace_key["value"], "[REDACTED-SERVICE_API_KEY]");
    assert_eq!(workspace_key["source"], "workspace");
    let canonical_project_env = std::fs::canonicalize(&project_env).unwrap();
    assert_eq!(
        std::fs::canonicalize(workspace_key["path"].as_str().unwrap()).unwrap(),
        canonical_project_env
    );

    let file_key = as_json(&get("OCTOCODE_CLASSIFICATION_API", true, None));
    assert_eq!(file_key["value"], "[REDACTED-OCTOCODE_CLASSIFICATION_API]");
    assert_eq!(file_key["source"], "home");
    assert_eq!(
        file_key["path"],
        workspace
            .home
            .join(".octocoderc")
            .to_string_lossy()
            .as_ref()
    );

    // A setting path reads the variable that sets it.
    let by_setting = get("classification.api", true, None);
    assert!(by_setting.status.success());
    let by_setting = as_json(&by_setting);
    assert_eq!(by_setting["key"], "OCTOCODE_CLASSIFICATION_API");
    assert_eq!(by_setting["value"], file_key["value"]);
    assert_eq!(by_setting["path"], file_key["path"]);

    let process = as_json(&get(
        "GITHUB_TOKEN",
        true,
        Some(("GITHUB_TOKEN", "process-secret-value")),
    ));
    assert_eq!(process["value"], "[REDACTED-GITHUB_TOKEN]");
    assert_eq!(process["source"], "env");
    assert!(
        process.get("path").is_none_or(serde_json::Value::is_null),
        "{process}"
    );

    let plain = get("PLAIN_SETTING", true, None);
    let plain = as_json(&plain);
    assert_eq!(plain["value"], "visible");
    assert_eq!(plain["source"], "home");
    assert_eq!(
        String::from_utf8_lossy(&get("PLAIN_SETTING", false, None).stdout),
        "visible\n"
    );

    for output in [get("GH_TOKEN", true, None), get("GH_TOKEN", false, None)] {
        assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
        assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
    }
}
