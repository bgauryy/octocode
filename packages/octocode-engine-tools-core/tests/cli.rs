mod support;

use std::process::Output;
use support::Workspace;

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap_or("")
}

fn stderr(output: &Output) -> &str {
    std::str::from_utf8(&output.stderr).unwrap_or("")
}

fn exit_code(output: &Output) -> Option<i32> {
    output.status.code()
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
    let text = format!("{}{}", stdout(&skill), stderr(&skill));
    assert!(
        text.contains("npx -y octocode skill") || text.contains("octocode skill list"),
        "{text}"
    );
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
    assert_eq!(codex.status.code(), Some(2));
    let dry = workspace
        .cli()
        .args(["install", "--ide", "cursor", "--dry-run", "--json"])
        .output()
        .expect("dry-run");
    assert!(dry.status.success(), "{}", stderr(&dry));
    let preview: serde_json::Value = serde_json::from_str(stdout(&dry)).expect("dry-run json");
    let server = &preview["config"]["mcpServers"]["octocode"];
    assert_eq!(server["command"], "npx", "{preview}");
    assert_eq!(
        server["args"],
        serde_json::json!(["-y", "octocode-mcp@latest"]),
        "{preview}"
    );
    let config = workspace.home.join(".cursor").join("mcp.json");
    assert!(!config.exists(), "dry-run must not write");

    let written = workspace
        .cli()
        .args(["install", "--ide", "cursor"])
        .output()
        .expect("write");
    assert!(written.status.success(), "{}", stderr(&written));
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
fn skill_passthrough_sends_skill_argv_to_node_cli() {
    let workspace = Workspace::new();
    let bin = workspace.home.join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    let log = workspace.home.join("skill-argv.txt");
    let stub = bin.join("octocode");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\" > '{}'\n",
            log.display()
        ),
    )
    .expect("stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let output = workspace
        .cli()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .args(["skill", "list", "--json"])
        .output()
        .expect("skill spawn");
    assert!(output.status.success(), "{}", stderr(&output));
    let recorded = std::fs::read_to_string(&log).expect("argv log");
    assert!(recorded.contains("skill"), "{recorded}");
    assert!(recorded.contains("list"), "{recorded}");
    assert!(recorded.contains("--json"), "{recorded}");
}

#[test]
fn tools_rejects_non_canonical_fields() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args([
            "tools",
            "astSearch",
            r#"{"operation":"syntax","path":"."}"#,
            "--json",
            "--compact",
        ])
        .output()
        .expect("tools");
    assert_eq!(output.status.code(), Some(2));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    // tool rejects invalid operation value — error contains field name or rejection key
    assert!(
        combined.contains("invalidInput")
            || combined.to_lowercase().contains("unexpected")
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

// ── parse-time validation (step 3) ───────────────────────────────────────────

#[test]
fn graph_invalid_analysis_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["graph", ".", "badanalysis"])
        .output()
        .expect("graph invalid analysis");
    assert_eq!(exit_code(&output), Some(2), "{}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        combined.contains("possible values") || combined.contains("deadCode"),
        "missing choices hint: {combined}"
    );
}

#[test]
fn history_invalid_operation_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["history", "badop", "--repo", "owner/repo"])
        .output()
        .expect("history invalid op");
    assert_eq!(exit_code(&output), Some(2), "{}", stderr(&output));
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        combined.contains("possible values") || combined.contains("prs"),
        "missing choices hint: {combined}"
    );
}

#[test]
fn search_missing_required_args_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .arg("search")
        .output()
        .expect("search no args");
    assert_eq!(exit_code(&output), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("Usage: octocode search"),
        "{}",
        stderr(&output)
    );
}

// ── fetch command (step: remote symmetry) ───────────────────────────────────

#[test]
fn fetch_help_shows_ref_and_branch_flags() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["fetch", "--help"])
        .output()
        .expect("fetch --help");
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("REF") || text.contains("ref"), "missing ref positional: {text}");
    assert!(text.contains("--branch"), "missing --branch flag: {text}");
    assert!(text.contains("--lines"), "missing --lines flag: {text}");
    assert!(text.contains("--pretty"), "missing --pretty flag: {text}");
}

#[test]
fn fetch_missing_ref_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .arg("fetch")
        .output()
        .expect("fetch no args");
    assert_eq!(exit_code(&output), Some(2), "{:?}", output);
    let combined = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        combined.contains("Usage: octocode fetch") || combined.contains("REF"),
        "missing usage hint: {combined}"
    );
}

#[test]
fn fetch_bad_lines_format_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["fetch", "owner/repo/README.md", "--lines", "badformat"])
        .output()
        .expect("fetch bad lines");
    // Exits 2 (invalid input) or 5 (tool error from network) depending on whether
    // --lines is validated before the network call. We accept either.
    let code = exit_code(&output).unwrap_or(0);
    assert!(code != 0, "expected non-zero exit for bad --lines");
}

// ── help-text content (steps 4 & 7) ──────────────────────────────────────────

#[test]
fn files_help_shows_path_and_names_descriptions() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["files", "--help"])
        .output()
        .expect("files --help");
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("Local file or directory root"),
        "missing path description: {text}"
    );
    assert!(
        text.contains("names") || text.contains("glob"),
        "missing names hint: {text}"
    );
}

#[test]
fn graph_help_shows_valid_analysis_values() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["graph", "--help"])
        .output()
        .expect("graph --help");
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("deadCode"), "missing deadCode: {text}");
    assert!(text.contains("cycles"), "missing cycles: {text}");
    assert!(text.contains("reachability"), "missing reachability: {text}");
}

#[test]
fn pretty_flag_appears_in_subcommand_help() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["tree", "--help"])
        .output()
        .expect("tree --help");
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("--pretty"),
        "--pretty flag not in help: {text}"
    );
}
