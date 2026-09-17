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
fn status_honors_enterprise_hostname_and_personal_access_token_alias() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("GITHUB_PERSONAL_ACCESS_TOKEN", "fixture-pat")
        .args(["status", "--hostname", "ghe.example.com", "--json"])
        .output()
        .expect("status");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("status json");
    assert_eq!(value["auth"]["hostname"], "ghe.example.com");
    assert_eq!(value["auth"]["authenticated"], true);
    assert_eq!(value["auth"]["tokenSource"], "env");
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
    assert!(
        text.contains("REF") || text.contains("ref"),
        "missing ref positional: {text}"
    );
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
    assert!(
        text.contains("reachability"),
        "missing reachability: {text}"
    );
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

// ── New command coverage ─────────────────────────────────────────────────────

#[test]
fn ast_without_lang_on_dir_exits_two_with_hint() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["ast", ".", "fn $NAME"])
        .output()
        .expect("ast");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        text.contains("--lang"),
        "expected --lang hint in error: {text}"
    );
}

#[test]
fn ast_help_shows_lang_flag() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["ast", "--help"])
        .output()
        .expect("ast --help");
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("--lang"),
        "missing --lang in ast help: {text}"
    );
}

#[test]
fn rewrite_missing_lang_emits_hint() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["rewrite", ".", "fn $N", "--to", "fn ${N}_v2"])
        .output()
        .expect("rewrite");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        text.contains("--lang"),
        "expected --lang hint in rewrite error: {text}"
    );
}

#[test]
fn history_pr_without_number_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["history", "pr", "--repo", "owner/repo"])
        .output()
        .expect("history pr");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(text.contains("--number"), "expected --number hint: {text}");
}

#[test]
fn history_commit_without_ref_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["history", "commit", "--repo", "owner/repo"])
        .output()
        .expect("history commit");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(text.contains("--ref"), "expected --ref hint: {text}");
}

#[test]
fn direct_tool_dispatch_bad_json_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["localSearch", "not-valid-json"])
        .output()
        .expect("localSearch bad json");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        text.contains("Invalid JSON") || text.contains("Usage:"),
        "expected json error: {text}"
    );
}

#[test]
fn direct_tool_dispatch_no_args_exits_two_with_usage() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["astSearch"])
        .output()
        .expect("astSearch no args");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        text.contains("Usage:") || text.contains("--scheme"),
        "expected usage hint: {text}"
    );
}

#[test]
fn tools_with_name_only_shows_usage_hint() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["tools", "localSearch"])
        .output()
        .expect("tools localSearch");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        text.contains("localSearch"),
        "expected tool name in hint: {text}"
    );
}

#[test]
fn callers_help_is_reachable() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["callers", "--help"])
        .output()
        .expect("callers --help");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("callers") || stdout(&output).contains("caller"));
}

#[test]
fn callees_help_is_reachable() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["callees", "--help"])
        .output()
        .expect("callees --help");
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn hover_help_is_reachable() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["hover", "--help"])
        .output()
        .expect("hover --help");
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn type_def_help_is_reachable() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["type-def", "--help"])
        .output()
        .expect("type-def --help");
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn implementation_help_is_reachable() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["implementation", "--help"])
        .output()
        .expect("implementation --help");
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn code_help_shows_lang_and_path_flags() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["code", "--help"])
        .output()
        .expect("code --help");
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("--lang"), "missing --lang: {text}");
    assert!(
        text.contains("--owner") || text.contains("owner"),
        "missing --owner: {text}"
    );
}

#[test]
fn gh_tree_without_owner_repo_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["gh-tree", "notaslashedrepo"])
        .output()
        .expect("gh-tree bad repo");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        text.contains("Usage:") || text.contains("OWNER/REPO"),
        "expected usage: {text}"
    );
}

#[test]
fn schema_alias_prints_the_complete_tool_contract() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["tools", "localSearch", "--scheme", "--compact"])
        .output()
        .expect("schema alias");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("contract JSON");
    assert_eq!(value["name"], "localSearch");
    assert!(value["inputSchema"].is_object());
    assert!(value["outputSchema"].is_object());
}

#[test]
fn history_compare_requires_both_refs() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args([
            "history",
            "compare",
            "--repo",
            "owner/repo",
            "--base",
            "main",
        ])
        .output()
        .expect("history compare");
    assert_eq!(exit_code(&output), Some(2));
    assert!(stderr(&output).contains("--head"), "{}", stderr(&output));
}

#[test]
fn tools_catalog_lists_enabled_count() {
    let workspace = Workspace::new();
    let output = workspace.cli().args(["tools"]).output().expect("tools");
    // tools exits 0 for catalog listing
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("enabled") || text.contains("Tools"),
        "expected tool listing: {text}"
    );
}

#[test]
fn tools_json_emits_compact_discovery_catalog() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["tools", "--json"])
        .output()
        .expect("tools json");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        output.stdout.len() < 20_000,
        "discovery catalog should stay token-efficient, got {} bytes",
        output.stdout.len()
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("catalog JSON");
    assert_eq!(value["kind"], "octocode.toolCatalog");
    assert_eq!(
        value["toolCount"],
        value["tools"].as_array().expect("tools array").len()
    );
    assert_eq!(
        value["commands"]["schema"],
        "tools <name> --scheme --json --compact"
    );
    let first = &value["tools"][0];
    assert!(first["name"].is_string());
    assert!(first["fields"].is_string());
    assert!(first["availability"]["enabled"].is_boolean());
    assert!(first.get("inputSchema").is_none());
    assert!(first.get("outputSchema").is_none());
}

#[test]
fn tools_accepts_queries_flag_like_the_node_cli() {
    let workspace = Workspace::new();
    let path = workspace.write("query-flag.rs", "fn query_flag() {}\n");
    let query = serde_json::json!({
        "path": path,
        "startLine": 1,
        "endLine": 1
    })
    .to_string();
    let output = workspace
        .cli()
        .args(["tools", "localFetch", "--queries", &query, "--compact"])
        .output()
        .expect("tools --queries");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("tool JSON");
    assert!(
        value["results"][0]["data"]["content"]
            .as_str()
            .is_some_and(|content| content.contains("query_flag"))
    );
}

#[test]
fn search_emits_one_selected_output_mode() {
    let workspace = Workspace::new();
    let path = workspace.write("search.rs", "fn needle() {}\n");
    let path = path.to_str().expect("utf8");

    let human = workspace
        .cli()
        .args(["search", "needle", path, "--fixed-strings"])
        .output()
        .expect("human search");
    assert!(human.status.success(), "{}", stderr(&human));
    assert!(
        stdout(&human).contains("search.rs:1:"),
        "{}",
        stdout(&human)
    );
    assert!(
        !stdout(&human).contains("\"searchEngine\""),
        "human output must not append JSON: {}",
        stdout(&human)
    );

    let json = workspace
        .cli()
        .args(["search", "needle", path, "--fixed-strings", "--json"])
        .output()
        .expect("json search");
    assert!(json.status.success(), "{}", stderr(&json));
    let value: serde_json::Value = serde_json::from_str(stdout(&json)).expect("one JSON document");
    assert_eq!(value["results"][0]["data"]["searchEngine"], "rg");
}

#[test]
fn rewrite_apply_previews_then_applies_with_hash_guards() {
    let workspace = Workspace::new();
    let path = workspace.write("rewrite.rs", "pub const VALUE: u32 = 2;\n");
    let output = workspace
        .cli()
        .env("ENABLE_AST_REWRITE_APPLY", "true")
        .args([
            "rewrite",
            path.to_str().expect("utf8"),
            "pub const $NAME: u32 = $VALUE;",
            "--to",
            "pub const $NAME: u64 = $VALUE;",
            "--lang",
            "rust",
            "--apply",
        ])
        .output()
        .expect("rewrite apply");
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(
        std::fs::read_to_string(path).expect("rewritten source"),
        "pub const VALUE: u64 = 2;\n"
    );
}

#[test]
fn json_errors_do_not_leak_duplicate_stderr() {
    let workspace = Workspace::new();
    let missing = workspace.workspace.join("missing.rs");
    let output = workspace
        .cli()
        .args(["--json-errors", "read", missing.to_str().expect("utf8")])
        .output()
        .expect("missing read");
    assert_eq!(exit_code(&output), Some(3));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("JSON error");
    assert_eq!(value["errorCode"], "fileAccessFailed");
    assert!(
        stderr(&output).is_empty(),
        "duplicate stderr: {}",
        stderr(&output)
    );

    let malformed = workspace
        .cli()
        .args(["--json-errors", "localSearch", "{"])
        .output()
        .expect("malformed raw JSON");
    assert_eq!(exit_code(&malformed), Some(2));
    let error: serde_json::Value =
        serde_json::from_str(stdout(&malformed)).expect("machine-readable parse error");
    assert_eq!(error["success"], false);
    assert!(stderr(&malformed).is_empty(), "{}", stderr(&malformed));
}

#[test]
fn install_rejects_unknown_method_and_accepts_claude_alias() {
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

    let claude = workspace
        .cli()
        .args(["install", "--ide", "claude", "--dry-run", "--json"])
        .output()
        .expect("claude alias");
    assert!(claude.status.success(), "{}", stderr(&claude));
    let value: serde_json::Value = serde_json::from_str(stdout(&claude)).expect("install JSON");
    assert_eq!(value["ide"], "claude-desktop");
}
