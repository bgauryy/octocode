use super::*;

#[test]
fn checks_inspect_existing_entries_without_creating_or_repairing_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let check = super::super::InstallArgs {
        check: true,
        json: true,
        ..default_args()
    };
    for (ide, file) in [
        ("cursor", "c.json"),
        ("zed", "z.json"),
        ("opencode", "o.json"),
        ("codex", "c.toml"),
    ] {
        let path = dir
            .path()
            .canonicalize()
            .expect("canonical tempdir")
            .join(file);
        let inspect =
            |args: &super::super::InstallArgs| super::super::install(&home(spec(ide), &path), args);
        assert_eq!(inspect(&check).expect("missing"), 1, "{ide}");
        assert!(!path.exists());
        assert_eq!(inspect(&default_args()).expect("install"), 0);
        let before = std::fs::read(&path).expect("before");
        assert_eq!(inspect(&check).expect("valid"), 0);
        assert_eq!(std::fs::read(&path).expect("after"), before);
    }
    let keys = spec("cursor").server_keys();
    assert!(!super::super::valid_server(
        spec("cursor"),
        &json!({"mcpServers":{"octocode":{"command":"npx","args":["other-package"]}}}),
        &keys
    ));
    assert!(!super::super::valid_server(
        spec("vscode-cline"),
        &json!({"mcpServers":{"octocode":{"command":"npx","args":["octocode-mcp@latest"],"disabled":true}}}),
        &keys
    ));
}

/// `--remove` deletes only the entry install wrote, keeps the rest of
/// the file and a backup, and is a no-op when nothing is installed.
#[test]
fn remove_deletes_the_installed_entry_and_refuses_a_custom_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir
        .path()
        .canonicalize()
        .expect("canonical tempdir")
        .join("mcp.json");
    let target = home(spec("cursor"), &path);
    let remove = super::super::InstallArgs {
        remove: true,
        ..default_args()
    };
    assert_eq!(super::super::install(&target, &remove).expect("nothing"), 0);
    assert!(!path.exists(), "a no-op removal writes nothing");

    std::fs::write(
            &path,
            r#"{"mcpServers":{"other":{"command":"x"},"octocode":{"command":"node","args":["dev.js"]}}}"#,
        )
        .expect("fixture");
    assert_eq!(super::super::install(&target, &remove).expect("custom"), 1);
    assert!(
        std::fs::read_to_string(&path)
            .expect("read")
            .contains("dev.js")
    );

    std::fs::remove_file(&path).expect("reset");
    std::fs::write(&path, r#"{"mcpServers":{"other":{"command":"x"}}}"#).expect("fixture");
    assert_eq!(
        super::super::install(&target, &default_args()).expect("install"),
        0
    );
    let dry = super::super::InstallArgs {
        dry_run: true,
        ..remove
    };
    let installed = std::fs::read_to_string(&path).expect("installed");
    assert_eq!(super::super::install(&target, &dry).expect("dry"), 0);
    assert_eq!(std::fs::read_to_string(&path).expect("read"), installed);
    let remove = super::super::InstallArgs {
        remove: true,
        ..default_args()
    };
    assert_eq!(super::super::install(&target, &remove).expect("remove"), 0);
    let root: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    assert_eq!(root, json!({"mcpServers":{"other":{"command":"x"}}}));
    assert!(
        path.with_file_name(".mcp.json.bak").exists(),
        "removal keeps a backup"
    );
}

/// The local scope writes Claude Code's per-project section of its user
/// config, beside other projects' entries.
#[test]
fn local_scope_writes_this_directory_project_entry() {
    let home_dir = tempfile::tempdir().expect("home");
    let cwd = tempfile::tempdir().expect("cwd");
    let path = home_dir.path().join(".claude.json");
    std::fs::write(&path, r#"{"projects":{"/elsewhere":{"mcpServers":{}}}}"#).expect("fixture");
    let target = Target {
        client: spec("claude-code"),
        scope: "local".into(),
        path: path.clone(),
        keys: vec![
            "projects".into(),
            cwd.path().to_string_lossy().into_owned(),
            "mcpServers".into(),
            "octocode".into(),
        ],
    };
    assert_eq!(
        super::super::install(&target, &default_args()).expect("install"),
        0
    );
    let root: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    let project = &root["projects"][cwd.path().to_string_lossy().as_ref()];
    assert_eq!(project["mcpServers"]["octocode"]["command"], "npx");
    assert!(root["projects"]["/elsewhere"].is_object());
    assert!(super::super::valid_server(
        spec("claude-code"),
        &root,
        &target.keys
    ));
    assert!(!super::super::valid_server(
        spec("claude-code"),
        &root,
        &spec("claude-code").server_keys()
    ));
}

#[test]
fn rollback_dry_run_preserves_destination_and_real_rollback_restores() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir
        .path()
        .canonicalize()
        .expect("canonical tempdir")
        .join("config.json");
    let backup = dir
        .path()
        .canonicalize()
        .expect("canonical tempdir")
        .join("config.json.bak");
    std::fs::write(&path, "current").expect("current");
    std::fs::write(&backup, "previous").expect("backup");
    let mut args = super::super::InstallArgs {
        rollback: Some(backup.to_string_lossy().into_owned()),
        dry_run: true,
        json: true,
        ..default_args()
    };
    assert_eq!(super::super::run(args), 0);
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "current");
    args = super::super::InstallArgs {
        rollback: Some(backup.to_string_lossy().into_owned()),
        ..default_args()
    };
    assert_eq!(super::super::run(args), 0);
    assert_eq!(std::fs::read_to_string(&path).expect("read"), "previous");
}

#[test]
fn randomized_private_backup_restores_the_original_filename() {
    let path = std::path::Path::new("/safe/.config.json-0123456789abcdef0123456789abcdef.bak");
    assert_eq!(
        super::super::rollback_destination(path),
        std::path::Path::new("/safe/config.json")
    );
    let path = std::path::Path::new("/safe/..claude.json-0123456789abcdef0123456789abcdef.bak");
    assert_eq!(
        super::super::rollback_destination(path),
        std::path::Path::new("/safe/.claude.json")
    );
    for (rolling, original) in [
        ("/safe/.mcp.json.bak", "/safe/mcp.json"),
        ("/safe/..claude.json.bak", "/safe/.claude.json"),
    ] {
        let rolling = std::path::Path::new(rolling);
        assert_eq!(
            super::super::rollback_destination(rolling),
            std::path::Path::new(original)
        );
        assert_eq!(
            octocode_native::config::backup_path(&super::super::rollback_destination(rolling)),
            rolling
        );
    }
}
