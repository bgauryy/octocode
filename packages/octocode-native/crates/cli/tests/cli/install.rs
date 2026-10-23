use super::*;

/// `--scope workspace` writes the project file under the working directory,
/// `--list` reports it per client, and `--remove` takes it out again.
#[test]
fn install_scopes_list_status_and_remove_round_trip() {
    let workspace = Workspace::new();
    let run = |args: &[&str]| workspace.cli().args(args).output().expect("install");
    let listed = |scope: &str| -> serde_json::Value {
        let output = run(&["install", "--list", "--scope", scope, "--json"]);
        assert!(output.status.success(), "{}", stderr(&output));
        let list: serde_json::Value = serde_json::from_str(stdout(&output)).expect("list json");
        assert_eq!(list["scope"], scope, "{list}");
        list["ides"]
            .as_array()
            .expect("ides")
            .iter()
            .find(|row| row["id"] == "cursor")
            .cloned()
            .expect("cursor row")
    };
    assert_eq!(listed("workspace")["status"], "absent");
    let written = run(&["install", "--ide", "cursor", "--scope", "workspace"]);
    assert!(written.status.success(), "{}", stderr(&written));
    let config = workspace.workspace.join(".cursor").join("mcp.json");
    assert!(config.exists(), "workspace scope writes the project file");
    assert!(!workspace.home.join(".cursor").join("mcp.json").exists());
    let row = listed("workspace");
    assert_eq!(row["status"], "installed", "{row}");
    assert_eq!(
        row["scopes"],
        serde_json::json!(["home", "workspace"]),
        "{row}"
    );
    assert_eq!(listed("home")["status"], "absent");

    let unsupported = run(&["install", "--ide", "windsurf", "--scope", "workspace"]);
    assert_eq!(unsupported.status.code(), Some(2));
    assert!(
        stderr(&unsupported).contains("supports --scope home"),
        "{}",
        stderr(&unsupported)
    );

    let removed = run(&[
        "install",
        "--ide",
        "cursor",
        "--scope",
        "workspace",
        "--remove",
    ]);
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(
        stdout(&removed).contains("Undo: octocode install --rollback "),
        "{}",
        stdout(&removed)
    );
    assert_eq!(listed("workspace")["status"], "absent");
}

#[test]
fn install_writes_npx_latest_and_never_octo_mcp() {
    let workspace = Workspace::new();
    let missing = workspace.cli().arg("install").output().expect("install");
    assert_eq!(missing.status.code(), Some(2));
    let codex = workspace
        .cli()
        .args(["install", "--ide", "codex"])
        .output()
        .expect("codex");
    // codex is a supported IDE: install should succeed
    assert!(
        codex.status.success(),
        "codex install failed: {}",
        stderr(&codex)
    );
    let dry = workspace
        .cli()
        .args(["install", "--ide", "cursor", "--dry-run", "--json"])
        .output()
        .expect("dry-run");
    assert!(dry.status.success(), "{}", stderr(&dry));
    let preview: serde_json::Value = serde_json::from_str(stdout(&dry)).expect("dry-run json");
    assert_eq!(preview["entry"]["method"], "npx", "{preview}");
    assert_eq!(preview["entry"]["customCommand"], false, "{preview}");
    assert!(
        preview.get("config").is_none(),
        "dry-run must not expose saved configuration: {preview}"
    );
    let text = workspace
        .cli()
        .args(["install", "--ide", "cursor", "--dry-run"])
        .output()
        .expect("dry-run text");
    assert!(text.status.success(), "{}", stderr(&text));
    let text = stdout(&text);
    assert!(
        text.starts_with("Dry run for cursor: nothing written.\n"),
        "{text}"
    );
    assert!(
        text.contains("\n  action   add the octocode entry\n"),
        "{text}"
    );
    assert!(text.contains("\n  command  npx "), "{text}");
    assert!(!text.contains('{'), "dry-run text is not JSON: {text}");
    let config = workspace.home.join(".cursor").join("mcp.json");
    assert!(!config.exists(), "dry-run must not write");

    let written = workspace
        .cli()
        .args(["install", "--ide", "cursor"])
        .output()
        .expect("write");
    assert!(written.status.success(), "{}", stderr(&written));
    assert!(
        stdout(&written).contains("Next: restart cursor")
            && stdout(&written).contains("octocode auth status"),
        "{}",
        stdout(&written)
    );
    let installed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config).expect("mcp.json"))
            .expect("written json");
    let server = &installed["mcpServers"]["octocode"];
    assert_eq!(server["command"], "npx");
    assert_eq!(
        server["args"],
        serde_json::json!(["-y", "octocode-mcp@latest"])
    );
    assert_ne!(server["command"], "octo");
    assert!(
        server["args"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|value| value != "mcp")
    );
}

#[test]
fn install_rejects_unknown_method_and_names_near_miss_ids() {
    let workspace = Workspace::new();
    let invalid = workspace
        .cli()
        .args([
            "install",
            "--ide",
            "cursor",
            "--method",
            "invalid",
            "--dry-run",
        ])
        .output()
        .expect("invalid method");
    assert_eq!(exit_code(&invalid), Some(2), "{}", stderr(&invalid));

    // Ids are exact: `claude` names both Claude clients instead of picking one.
    for alias in ["claude", "vscode"] {
        let near = workspace
            .cli()
            .args(["install", "--ide", alias, "--dry-run", "--json"])
            .output()
            .expect("retired alias");
        assert_eq!(exit_code(&near), Some(2), "{alias}: {}", stdout(&near));
        let error: serde_json::Value = serde_json::from_str(stdout(&near)).expect("JSON error");
        assert_bare_error(&error);
        assert!(
            error["error"]
                .as_str()
                .is_some_and(|message| message.contains("Did you mean")),
            "{error}"
        );
    }
    let near = workspace
        .cli()
        .args(["install", "--ide", "claude", "--dry-run"])
        .output()
        .expect("near miss text");
    let text = stderr(&near);
    assert!(
        text.contains("Did you mean")
            && text.contains("claude-code")
            && text.contains("claude-desktop"),
        "{text}"
    );
    let desktop = workspace
        .cli()
        .args(["install", "--ide", "claude-desktop", "--dry-run", "--json"])
        .output()
        .expect("claude-desktop");
    assert!(desktop.status.success(), "{}", stderr(&desktop));
    let value: serde_json::Value = serde_json::from_str(stdout(&desktop)).expect("install JSON");
    assert_eq!(value["ide"], "claude-desktop");
}
