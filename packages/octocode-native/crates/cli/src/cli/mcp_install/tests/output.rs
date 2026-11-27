use super::super::{InstallArgs, Scope, client_status, install, list_clients, run};
use super::*;
use octocode_test_support::json_fixture;

/// `path` inside a fresh canonical directory, kept alive by the guard.
fn config_in(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().canonicalize().expect("canonical").join(name);
    (dir, path)
}

fn args(json: bool) -> InstallArgs {
    InstallArgs {
        json,
        ..default_args()
    }
}

/// Every install outcome has an exit and leaves the file as it says, in
/// text as in JSON: a write, an identical reinstall, a differing entry
/// that needs --force, and a dry run that writes nothing.
#[test]
fn install_outcomes_exit_the_same_in_text_and_json() {
    for json in [false, true] {
        let (_dir, path) = config_in("mcp.json");
        let target = home(spec("cursor"), &path);
        let dry = InstallArgs {
            dry_run: true,
            ..args(json)
        };
        assert_eq!(install(&target, &dry).expect("dry run"), 0);
        assert!(!path.exists(), "a dry run writes nothing");
        assert_eq!(install(&target, &args(json)).expect("install"), 0);
        let written = std::fs::read_to_string(&path).expect("written");
        assert_eq!(install(&target, &args(json)).expect("same"), 0);
        assert_eq!(install(&target, &dry).expect("dry run of same"), 0);
        std::fs::write(&path, written.replace("npx", "node")).expect("edit");
        assert_eq!(
            install(&target, &args(json)).expect("different"),
            1,
            "an edited entry needs --force"
        );
        assert_eq!(install(&target, &dry).expect("dry run of different"), 0);
        let force = InstallArgs {
            force: true,
            ..args(json)
        };
        assert_eq!(install(&target, &force).expect("force"), 0);
        assert_eq!(std::fs::read_to_string(&path).expect("restored"), written);
    }
}

/// `--check` passes only on an entry install wrote; `--remove` refuses a
/// custom entry without --force, previews with --dry-run and otherwise
/// deletes, in either output.
#[test]
fn check_and_remove_exit_by_what_the_entry_is() {
    for json in [false, true] {
        let (_dir, path) = config_in("mcp.json");
        let target = home(spec("cursor"), &path);
        let check = InstallArgs {
            check: true,
            ..args(json)
        };
        let remove = InstallArgs {
            remove: true,
            ..args(json)
        };
        assert_eq!(install(&target, &check).expect("absent"), 1);
        assert_eq!(install(&target, &remove).expect("nothing to remove"), 0);
        assert_eq!(install(&target, &args(json)).expect("install"), 0);
        assert_eq!(install(&target, &check).expect("valid"), 0);
        let written = std::fs::read_to_string(&path).expect("written");
        let dry = InstallArgs {
            dry_run: true,
            ..remove
        };
        assert_eq!(install(&target, &dry).expect("dry removal"), 0);
        assert_eq!(std::fs::read_to_string(&path).expect("kept"), written);
        assert_eq!(
            install(
                &target,
                &InstallArgs {
                    remove: true,
                    ..args(json)
                }
            )
            .expect("removal"),
            0
        );
        assert_eq!(install(&target, &check).expect("removed"), 1);

        std::fs::write(
            &path,
            r#"{"mcpServers":{"octocode":{"command":"node","args":["server.js"]}}}"#,
        )
        .expect("custom entry");
        assert_eq!(install(&target, &check).expect("custom"), 1);
        assert_eq!(
            install(
                &target,
                &InstallArgs {
                    remove: true,
                    ..args(json)
                }
            )
            .expect("custom removal"),
            1,
            "an entry install did not write needs --force"
        );
        assert!(
            std::fs::read_to_string(&path)
                .expect("kept")
                .contains("server.js")
        );
        assert_eq!(
            install(
                &target,
                &InstallArgs {
                    remove: true,
                    force: true,
                    ..args(json)
                }
            )
            .expect("forced removal"),
            0
        );
        assert!(
            !std::fs::read_to_string(&path)
                .expect("rewritten")
                .contains("server.js")
        );
    }
}

/// An opencode entry counts as installed only as a local server, and an
/// entry without a launch command never does.
#[test]
fn an_entry_without_its_launch_shape_is_not_valid() {
    let opencode = spec("opencode");
    let keys = opencode.server_keys();
    let (_dir, path) = config_in("opencode.json");
    assert_eq!(
        install(&home(opencode, &path), &args(true)).expect("install"),
        0
    );
    let mut root: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    assert!(super::super::valid_server(opencode, &root, &keys));
    let entry = keys
        .iter()
        .try_fold(&mut root, |value, key| value.get_mut(key))
        .expect("entry");
    entry["type"] = json!("remote");
    assert!(!super::super::valid_server(opencode, &root, &keys));
    let cursor = spec("cursor");
    assert!(!super::super::valid_server(
        cursor,
        &json_fixture(
            r#"{
                "mcpServers": {"octocode": {"args": ["octocode-mcp@latest"]}}
            }"#
        ),
        &cursor.server_keys()
    ));
}

/// `--list` names each client's state at a scope: a client without that
/// scope is unsupported, a missing file absent, an unreadable one invalid,
/// an entry install wrote installed and any other `octocode` entry custom.
#[test]
fn list_reports_each_clients_entry_state_at_the_scope() {
    let dir = tempfile::tempdir().expect("cwd");
    let cwd = dir.path().canonicalize().expect("canonical");
    let state = |id: &str| {
        let client = mcp_clients::client(id).expect("client");
        client_status(client, Scope::Workspace, &cwd).status
    };
    assert_eq!(state("cursor"), "absent");
    assert_eq!(state("claude-desktop"), "unsupported");

    std::fs::create_dir_all(cwd.join(".cursor")).expect("cursor dir");
    std::fs::write(cwd.join(".cursor/mcp.json"), "{not json").expect("broken");
    assert_eq!(state("cursor"), "invalid");
    let workspace = Target {
        client: spec("cursor"),
        scope: "workspace".into(),
        path: cwd.join(".cursor/mcp.json"),
        keys: spec("cursor").server_keys(),
    };
    std::fs::remove_file(&workspace.path).expect("remove broken");
    assert_eq!(install(&workspace, &args(true)).expect("install"), 0);
    assert_eq!(state("cursor"), "installed");
    std::fs::write(
        &workspace.path,
        r#"{"mcpServers":{"octocode":{"command":"node","args":["server.js"]}}}"#,
    )
    .expect("custom");
    assert_eq!(state("cursor"), "custom");

    let statuses = mcp_clients::CLIENTS
        .iter()
        .map(|client| client_status(client, Scope::Workspace, &cwd))
        .collect::<Vec<_>>();
    assert!(statuses.iter().any(|row| row.status == "unsupported"));
    for json in [false, true] {
        assert_eq!(list_clients(Scope::Workspace, &statuses, json), 0);
    }
}

/// `--rollback` takes only an existing `.bak` file, previews in text, and
/// reports a restore it cannot make.
#[test]
fn rollback_refuses_what_is_not_a_backup_and_restores_one() {
    let (_dir, path) = config_in("mcp.json");
    let rollback = |backup: &std::path::Path, dry_run: bool, json: bool| {
        run(InstallArgs {
            rollback: Some(backup.to_string_lossy().into_owned()),
            dry_run,
            json,
            ..default_args()
        })
    };
    let backup = path.with_file_name("mcp.json.bak");
    assert_eq!(rollback(&backup, false, false), 2, "a missing backup");
    let other = path.with_file_name("mcp.json.old");
    std::fs::write(&other, "{}").expect("not a backup");
    assert_eq!(rollback(&other, false, false), 2, "not a .bak file");

    std::fs::write(&path, "current").expect("current");
    std::fs::write(&backup, "previous").expect("backup");
    assert_eq!(rollback(&backup, true, false), 0);
    assert_eq!(std::fs::read_to_string(&path).expect("kept"), "current");
    assert_eq!(rollback(&backup, false, true), 0);
    assert_eq!(
        std::fs::read_to_string(&path).expect("restored"),
        "previous"
    );

    let unreadable = path.with_file_name("dir.json.bak");
    std::fs::create_dir(&unreadable).expect("a directory named like a backup");
    assert_eq!(rollback(&unreadable, false, false), 5);
}

/// A request run cannot act on exits 2 before any file is touched: no
/// client, an unknown client, or a scope the client does not have.
#[test]
fn run_refuses_requests_it_cannot_act_on() {
    for json in [false, true] {
        let request = |ide: Option<&str>, scope: Scope| {
            run(InstallArgs {
                ide: ide.map(str::to_owned),
                scope,
                json,
                ..InstallArgs::default()
            })
        };
        // Text without a client on a terminal opens the picker; JSON never does.
        if json {
            assert_eq!(request(None, Scope::Home), 2);
        }
        assert_eq!(request(Some(""), Scope::Home), 2);
        assert_eq!(request(Some("no-such-client"), Scope::Home), 2);
        assert_eq!(request(Some("cursor"), Scope::Local), 2);
    }
}
