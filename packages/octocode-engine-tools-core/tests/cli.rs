mod support;

use std::process::Output;
use support::Workspace;

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap_or("")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap_or("")
}

#[test]
fn help_uses_the_octocode_command_name() {
    let workspace = Workspace::new();
    let output = workspace.cli().arg("--help").output().expect("help");
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("Usage: octocode"), "{text}");
    assert!(text.contains("Native Octocode research tools"), "{text}");
}

#[test]
fn clone_without_owner_repo_prints_usage() {
    let workspace = Workspace::new();
    let output = workspace.cli().arg("clone").output().expect("clone");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("Usage: octocode clone"),
        "{:?}",
        output
    );
}

#[test]
fn login_fails_closed() {
    let workspace = Workspace::new();
    let login = workspace
        .cli()
        .env("GITHUB_API_URL", "http://127.0.0.1:1/api/v3")
        .args(["login", "--no-open"])
        .output()
        .expect("login");
    assert_eq!(login.status.code(), Some(1));
}

#[test]
fn skill_without_octocode_prints_node_command_and_exits_1() {
    let workspace = Workspace::new();
    let empty_path = workspace.home.join("empty-path");
    std::fs::create_dir_all(&empty_path).expect("empty PATH dir");
    let output = workspace
        .cli()
        .env("PATH", &empty_path)
        .args(["skill", "install", "--all", "--platform", "cursor"])
        .output()
        .expect("skill");
    assert_eq!(output.status.code(), Some(1));
    let err = stderr(&output);
    assert_eq!(
        err,
        "octo skill requires the Node CLI (`octocode`) on PATH.\n\
         Install: npm i -g octocode\n\
         Then:    octocode skill install --all --platform cursor\n\
         Or:      npx -y octocode skill install --all --platform cursor\n"
    );
    assert!(
        !workspace.home.join("skills").exists(),
        "native must not create $OCTOCODE_HOME/skills"
    );
}

#[test]
fn skill_does_not_reenter_native_octocode_on_path() {
    let workspace = Workspace::new();
    let native = std::path::PathBuf::from(env!("CARGO_BIN_EXE_octocode"));
    let native_dir = native.parent().expect("native dir");
    let output = workspace
        .cli()
        .env("PATH", native_dir)
        .args(["skill", "list"])
        .output()
        .expect("skill");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        stderr(&output).starts_with("octo skill requires the Node CLI (`octocode`) on PATH."),
        "{}",
        stderr(&output)
    );
    assert!(
        !workspace.home.join("skills").exists(),
        "native must not create $OCTOCODE_HOME/skills"
    );
}

#[cfg(unix)]
#[test]
fn skill_spawns_octocode_with_skill_and_user_args() {
    let workspace = Workspace::new();
    let bin_dir = workspace.home.join("bin");
    std::fs::create_dir_all(&bin_dir).expect("bin dir");
    let argv_file = workspace.home.join("octocode-argv.txt");
    write_unix_script(
        &bin_dir.join("octocode"),
        &format!(
            r#"printf '%s\n' "$@" > "{}"
if [ "$1" = skill ] && [ "$2" = help ]; then
  echo "usage: octocode skill list|install|remove|check|info|help"
fi
exit 0
"#,
            argv_file.display()
        ),
    );
    write_unix_script(
        &bin_dir.join("octo"),
        "echo spawned octo instead of octocode >&2\nexit 99\n",
    );

    let install = workspace
        .cli()
        .env("PATH", &bin_dir)
        .args([
            "skill",
            "install",
            "--all",
            "--platform",
            "cursor",
            "--global",
        ])
        .output()
        .expect("skill install");
    assert_eq!(install.status.code(), Some(0), "{}", stderr(&install));
    assert_eq!(
        std::fs::read_to_string(&argv_file).expect("argv"),
        "skill\ninstall\n--all\n--platform\ncursor\n--global\n"
    );
    assert!(
        !workspace.home.join("skills").exists(),
        "native must not create $OCTOCODE_HOME/skills"
    );

    let help = workspace
        .cli()
        .env("PATH", &bin_dir)
        .args(["skill", "help"])
        .output()
        .expect("skill help");
    assert_eq!(help.status.code(), Some(0), "{}", stderr(&help));
    assert_eq!(
        std::fs::read_to_string(&argv_file).expect("argv"),
        "skill\nhelp\n"
    );
    assert!(
        stdout(&help).contains("usage: octocode skill list|install|remove|check|info|help"),
        "{}",
        stdout(&help)
    );
}

#[cfg(unix)]
#[test]
fn skill_skips_native_and_spawns_later_node_octocode() {
    let workspace = Workspace::new();
    let native = std::path::PathBuf::from(env!("CARGO_BIN_EXE_octocode"));
    let native_dir = native.parent().expect("native dir");
    let node_dir = workspace.home.join("node-bin");
    std::fs::create_dir_all(&node_dir).expect("node dir");
    let argv_file = workspace.home.join("octocode-argv.txt");
    write_unix_script(
        &node_dir.join("octocode"),
        &format!(
            r#"printf '%s\n' "$@" > "{}"
exit 0
"#,
            argv_file.display()
        ),
    );
    let path = std::env::join_paths([native_dir.as_os_str(), node_dir.as_os_str()]).expect("PATH");
    let output = workspace
        .cli()
        .env("PATH", &path)
        .args(["skill", "install", "--all"])
        .output()
        .expect("skill");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        std::fs::read_to_string(&argv_file).expect("argv"),
        "skill\ninstall\n--all\n"
    );
}

#[cfg(unix)]
fn write_unix_script(path: &std::path::Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).expect("script");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .expect("script metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod");
}

#[test]
fn tools_rejects_non_canonical_fields() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args([
            "tools",
            "astSearch",
            "--queries",
            r#"{"operation":"syntax","path":"."}"#,
            "--json",
            "--compact",
        ])
        .output()
        .expect("tools");
    assert_eq!(output.status.code(), Some(2));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        combined.contains("invalidInput")
            || combined.contains("Unexpected")
            || combined.contains("operation"),
        "{combined}"
    );
}

#[test]
fn read_pages_and_next_reconstructs_source() {
    let workspace = Workspace::new();
    let content: String = (1..24).map(|n| format!("line {n}: research\n")).collect();
    let path = workspace.write("source.txt", &content);
    let first = workspace
        .cli()
        .args([
            "read",
            path.to_str().expect("utf8"),
            "--chunk",
            "lines",
            "--limit",
            "3",
        ])
        .output()
        .expect("first read");
    assert_eq!(first.status.code(), Some(6), "{}", stderr(&first));
    let token = stderr(&first)
        .split("Continue: octocode next ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .expect("continuation token");
    let mut joined = stdout(&first).to_owned();
    let second = workspace
        .cli()
        .args(["next", token])
        .output()
        .expect("next");
    joined.push_str(stdout(&second));
    assert!(content.starts_with(&joined), "joined prefix");

    let drained = workspace
        .cli()
        .args([
            "read",
            path.to_str().expect("utf8"),
            "--chunk",
            "lines",
            "--limit",
            "3",
            "--all",
        ])
        .output()
        .expect("drain");
    assert!(drained.status.success(), "{}", stderr(&drained));
    assert_eq!(stdout(&drained), content);
}

fn install_server(home: &std::path::Path, ide: &str) -> serde_json::Value {
    let path = match ide {
        "cursor" => home.join(".cursor").join("mcp.json"),
        other => panic!("test helper covers cursor, got {other}"),
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{}", path.display()));
    assert!(
        !text.contains("\"command\": \"octo\"") && !text.contains("\"command\":\"octo\""),
        "written JSON must not use command octo: {text}"
    );
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("installed JSON");
    let server = parsed["mcpServers"]["octocode"].clone();
    let args = server["args"].as_array().expect("args");
    assert_eq!(server["command"], "npx");
    assert_eq!(server["type"], "stdio");
    assert_eq!(
        server["args"],
        serde_json::json!(["-y", "octocode-mcp@latest"])
    );
    assert!(
        args.iter()
            .all(|arg| arg.as_str() != Some("mcp") && arg.as_str() != Some("octo")),
        "written args must not contain octo or mcp: {args:?}"
    );
    server
}

#[test]
fn install_without_ide_is_usage() {
    let workspace = Workspace::new();
    let output = workspace.cli().arg("install").output().expect("install");
    assert_eq!(output.status.code(), Some(2));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        combined.contains("Missing required option: --ide") || combined.contains("--ide"),
        "{combined}"
    );
}

#[test]
fn install_unknown_and_toml_ides_are_usage() {
    let workspace = Workspace::new();
    let unknown = workspace
        .cli()
        .args(["install", "--ide", "notepad"])
        .output()
        .expect("unknown");
    assert_eq!(unknown.status.code(), Some(2));
    let codex = workspace
        .cli()
        .args(["install", "--ide", "codex"])
        .output()
        .expect("codex");
    assert_eq!(codex.status.code(), Some(2));
    assert!(
        stderr(&codex).contains("TOML/YAML") || stdout(&codex).contains("TOML/YAML"),
        "{}",
        stderr(&codex)
    );
}

#[test]
fn install_writes_npx_stdio_server_and_rejects_octo_mcp() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["install", "--ide", "cursor", "--json"])
        .output()
        .expect("install");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let server = install_server(&workspace.home, "cursor");
    assert_eq!(server["command"], "npx");
    assert_eq!(server["args"][0], "-y");
}

#[test]
fn install_already_installed_without_force_exits_1() {
    let workspace = Workspace::new();
    let first = workspace
        .cli()
        .args(["install", "--ide", "cursor", "--json"])
        .output()
        .expect("first");
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    let second = workspace
        .cli()
        .args(["install", "--ide", "cursor", "--json"])
        .output()
        .expect("second");
    assert_eq!(second.status.code(), Some(1), "{}", stderr(&second));
    let payload: serde_json::Value =
        serde_json::from_str(stdout(&second)).expect("alreadyInstalled json");
    assert_eq!(payload["alreadyInstalled"], true);
    assert_eq!(payload["success"], false);
}

#[test]
fn install_dry_run_does_not_write() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["install", "--ide", "cursor", "--dry-run", "--json"])
        .output()
        .expect("dry-run");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(!workspace.home.join(".cursor").join("mcp.json").exists());
}

#[test]
fn install_without_home_does_not_write_cwd() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .args(["install", "--ide", "cursor", "--json"])
        .output()
        .expect("install without HOME");
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let payload: serde_json::Value =
        serde_json::from_str(stdout(&output)).expect("missing home json");
    assert_eq!(payload["success"], false);
    assert!(
        payload["error"]
            .as_str()
            .unwrap_or_default()
            .contains("refusing to write IDE config into the current directory"),
        "{payload}"
    );
    assert!(
        !workspace
            .workspace
            .join(".cursor")
            .join("mcp.json")
            .exists()
    );
    let listed = workspace
        .cli()
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .args(["install", "--list", "--json"])
        .output()
        .expect("list without HOME");
    assert_eq!(listed.status.code(), Some(0), "{}", stderr(&listed));
}

#[test]
fn help_lists_install_not_mcp_subcommand() {
    let workspace = Workspace::new();
    let output = workspace.cli().arg("--help").output().expect("help");
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("install"), "{text}");
    assert!(
        !text.contains("  mcp ") && !text.to_lowercase().contains("\n  mcp\n"),
        "native Command must not grow an mcp subcommand: {text}"
    );
}
