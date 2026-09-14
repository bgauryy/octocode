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
