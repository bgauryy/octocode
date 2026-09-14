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
fn login_and_skill_fail_closed() {
    let workspace = Workspace::new();
    let login = workspace.cli().arg("login").output().expect("login");
    assert_eq!(login.status.code(), Some(1));
    let skill = workspace
        .cli()
        .args(["skill", "list"])
        .output()
        .expect("skill");
    assert_eq!(skill.status.code(), Some(1));
    let unknown = workspace
        .cli()
        .args(["skill", "unknown"])
        .output()
        .expect("unknown skill");
    assert_eq!(unknown.status.code(), Some(2));
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
