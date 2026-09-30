// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

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
fn help_lists_only_the_minimal_command_surface() {
    let workspace = Workspace::new();
    let output = workspace.cli().arg("--help").output().expect("help");
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.contains("Usage: octocode"), "{text}");
    assert!(text.contains("Native Octocode research tools"), "{text}");
    // Every tool is a first-class command under its canonical name.
    for tool in [
        "localSearch",
        "localFetch",
        "astSearch",
        "astRewrite",
        "lspSearch",
        "ghSearchRepo",
        "ghSearchCode",
        "ghStructure",
        "ghGetFileContent",
        "ghSearchHistory",
        "ghGetHistoryItem",
        "ghCloneRepo",
        "artifactSearch",
        "clasify",
        "scheme",
        "config",
        "auth",
        "skill",
        "install",
    ] {
        assert!(
            text.contains(&format!("\n  {tool}")),
            "missing {tool}: {text}"
        );
    }
    // Retired wrappers and hidden maintenance commands stay out of the surface.
    for removed in [
        "\n  search",
        "\n  read",
        "\n  fetch",
        "\n  tools",
        "\n  files",
        "\n  tree",
        "\n  symbols",
        "\n  def",
        "\n  refs",
        "\n  history",
        "\n  package",
        "\n  clone",
        "\n  repos",
        "\n  code",
        "\n  context",
        "\n  status",
        "\n  login",
        "\n  logout",
        "\n  cache",
        "\n  lsp-server",
        "\n  jev",
    ] {
        assert!(!text.contains(removed), "alias leaked into help: {removed}");
    }
}

#[test]
fn clasify_missing_key_is_actionable() {
    let workspace = Workspace::new();
    let query = serde_json::json!({
        "id":"decision",
        "goal": "test", "reasoning":"Choose the next inspection.",
        "resources":[{"id":"observed","context":{"value":{"fact":"present"}}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Is it relevant?"}]
    });
    let output = workspace
        .cli()
        .args(["clasify", &query.to_string()])
        .output()
        .expect("missing-key execution");
    assert_eq!(exit_code(&output), Some(5));
    let output = stdout(&output);
    assert!(output.contains("missingConfiguration"), "{output}");
    assert!(output.contains("OCTOCODE_CLASSIFICATION_API"), "{output}");
    assert!(
        output.contains("https://docs.typesafe.ai/introduction"),
        "{output}"
    );
}

#[test]
fn tool_output_is_compact_by_default_and_pretty_on_request() {
    let workspace = Workspace::new();
    let query = serde_json::json!({
        "id":"decision",
        "goal": "test", "reasoning":"Exercise output formatting.",
        "resources":[{"id":"observed","context":{"value":{"fact":"present"}}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Is it relevant?"}]
    })
    .to_string();
    let compact = workspace.cli().args(["clasify", &query]).output().unwrap();
    assert_eq!(
        stdout(&compact).trim_end().lines().count(),
        1,
        "{}",
        stdout(&compact)
    );
    let pretty = workspace
        .cli()
        .args(["clasify", &query, "--pretty"])
        .output()
        .unwrap();
    assert!(stdout(&pretty).lines().count() > 1, "{}", stdout(&pretty));
    let retired = workspace
        .cli()
        .args(["clasify", &query, "--compact"])
        .output()
        .unwrap();
    assert_eq!(
        retired.status.code(),
        Some(2),
        "tool commands reject --compact"
    );
}

#[test]
fn blank_classification_key_disables_clasify_despite_home_and_vendor_keys() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.home.join(".env"),
        "OCTOCODE_CLASSIFICATION_API=from-home-env\n",
    )
    .unwrap();
    let query = serde_json::json!({
        "id":"decision",
        "goal": "test", "reasoning":"Exercise the opt-out.",
        "resources":[{"id":"observed","context":{"value":{"fact":"present"}}}],
        "questions":[{"id":"relevant","type":"noul","instructions":"Is it relevant?"}]
    })
    .to_string();
    let output = workspace
        .cli()
        .env("OCTOCODE_CLASSIFICATION_API", "")
        .env("OCTOCODE_JEV_KEY", "vendor-key")
        .args(["clasify", &query])
        .output()
        .unwrap();
    assert_eq!(exit_code(&output), Some(5), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("missingConfiguration"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn tool_help_uses_canonical_core_short_descriptions() {
    let workspace = Workspace::new();
    let contract = octocode_native::contracts::parsed_contract().expect("embedded contract");
    let root = workspace.cli().arg("--help").output().expect("root help");
    assert!(root.status.success());
    let root_text = stdout(&root)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for tool in contract["tools"].as_array().expect("contract tools") {
        let name = tool["name"].as_str().expect("tool name");
        let description = tool["shortDescription"]
            .as_str()
            .expect("core short description");
        assert!(!description.is_empty());
        assert!(
            root_text.contains(description),
            "missing canonical {name} description in root help"
        );
        for flag in ["-h", "--help"] {
            let output = workspace
                .cli()
                .args([name, flag])
                .output()
                .expect("tool help");
            assert!(output.status.success(), "{name}: {}", stderr(&output));
            let text = stdout(&output)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(text.contains(description), "{name} {flag}: {text}");
        }
    }
}

#[test]
fn removed_alias_commands_are_rejected() {
    let workspace = Workspace::new();
    for alias in [
        "search",
        "read",
        "fetch",
        "tools",
        "files",
        "tree",
        "symbols",
        "ast",
        "graph",
        "rewrite",
        "def",
        "refs",
        "hover",
        "callers",
        "callees",
        "type-def",
        "implementation",
        "supertypes",
        "subtypes",
        "diagnostics",
        "repos",
        "code",
        "gh-tree",
        "clone",
        "package",
        "history",
        "context",
        "status",
        "login",
        "logout",
    ] {
        let output = workspace.cli().arg(alias).output().expect("alias");
        assert_eq!(exit_code(&output), Some(2), "{alias} must be rejected");
    }
}

#[test]
fn hidden_maintenance_commands_still_work() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["cache", "status"])
        .output()
        .expect("cache status");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("cache home:"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn auth_login_and_skill_fail_closed() {
    let workspace = Workspace::new();
    let login = workspace
        .cli()
        .args(["auth", "login"])
        .output()
        .expect("auth login");
    assert_eq!(login.status.code(), Some(1));
    // Native subcommands (list/install/remove/check/info) succeed without the
    // Node CLI; only non-native subcommands delegate and must fail closed
    // when no `octocode` npm launcher is on PATH.
    let native = workspace
        .cli()
        .args(["skill", "list"])
        .output()
        .expect("skill list");
    assert!(native.status.success(), "{}", stderr(&native));
    let skill = workspace
        .cli()
        .args(["skill", "run", "demo"])
        .output()
        .expect("skill");
    assert_eq!(skill.status.code(), Some(1));
    let text = format!("{}{}", stdout(&skill), stderr(&skill));
    assert!(
        text.contains("npx -y octocode skill") || text.contains("octocode skill run"),
        "{text}"
    );
}

#[test]
fn auth_status_honors_personal_access_token_alias() {
    let workspace = Workspace::new();
    for argv in [vec!["auth", "--json"], vec!["auth", "status", "--json"]] {
        // Unreachable API: the token cannot be verified, so it stays
        // authenticated but says so.
        let output = workspace
            .cli()
            .env("GITHUB_PERSONAL_ACCESS_TOKEN", "fixture-pat")
            .env("GITHUB_API_URL", "http://127.0.0.1:1/api/v3")
            .args(&argv)
            .output()
            .expect("auth status");
        assert!(output.status.success(), "{}", stderr(&output));
        let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("auth json");
        assert_eq!(value["authenticated"], true, "{argv:?}");
        assert_eq!(value["verification"], "unverified", "{argv:?}");
        assert_eq!(value["tokenSource"], "env", "{argv:?}");
    }
}

/// A whitespace-only env token must NOT be reported as an `env` credential:
/// the request path resolves tokens through `resolve_env_token`, which trims
/// and drops empty values, and `auth status` now shares that same selection
/// (`config.token`). Before the flows were unified the diagnostic re-read the
/// raw var and reported `env` for a token the request path would never send.
#[test]
fn auth_status_ignores_whitespace_only_env_token() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("GH_TOKEN", "   ")
        .args(["auth", "status", "--json"])
        .output()
        .expect("auth status");
    assert!(output.status.success() || output.status.code() == Some(1));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("auth json");
    assert_ne!(
        value["tokenSource"], "env",
        "whitespace-only token must not resolve as an env credential: {value}"
    );
}

#[test]
fn config_shows_files_and_keys_but_never_values() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["config", "--json"])
        .output()
        .expect("config json");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("config JSON");
    assert!(value["configFile"]["path"].is_string());
    assert!(value["configFile"]["exists"].is_boolean());
    assert!(value["envFiles"]["global"].is_string());
    assert!(value["envFiles"]["project"].is_string());
    assert!(value["envKeys"].is_array());

    let human = workspace.cli().arg("config").output().expect("config");
    assert!(human.status.success(), "{}", stderr(&human));
    let text = stdout(&human);
    assert!(text.contains("config file:"), "{text}");
    assert!(text.contains("env files:"), "{text}");
    assert!(
        text.contains("values are never printed"),
        "missing no-values note: {text}"
    );
}

#[test]
fn config_check_reports_set_state_without_the_value() {
    let workspace = Workspace::new();
    let set = workspace
        .cli()
        .env("GITHUB_TOKEN", "fixture-secret")
        .args(["config", "--check", "GITHUB_TOKEN"])
        .output()
        .expect("config check");
    assert!(set.status.success(), "{}", stderr(&set));
    assert!(
        stdout(&set).contains("GITHUB_TOKEN: set"),
        "{}",
        stdout(&set)
    );
    assert!(
        !stdout(&set).contains("fixture-secret"),
        "value leaked: {}",
        stdout(&set)
    );
    let unset = workspace
        .cli()
        .args(["config", "--check", "OCTOCODE_NOT_A_KEY"])
        .output()
        .expect("config check unset");
    assert_eq!(exit_code(&unset), Some(1));
    assert!(stdout(&unset).contains("unset"), "{}", stdout(&unset));
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
fn skill_lifecycle_runs_natively_without_the_node_cli() {
    // list/install/remove/check/info work with the npm CLI absent. The
    // workspace PATH has no `octocode`, and the delegation guard is armed so
    // any accidental delegation fails loudly.
    let workspace = Workspace::new();
    let source = workspace.home.join("src").join("demo-skill");
    std::fs::create_dir_all(source.join("references")).expect("skill dirs");
    std::fs::write(
        source.join("SKILL.md"),
        "---\nname: demo-skill\ndescription: \"Native demo\"\n---\n# Demo\n",
    )
    .expect("skill md");
    std::fs::write(source.join("references").join("g.md"), "guide\n").expect("skill ref");

    let install = workspace
        .cli()
        .env("OCTOCODE_SKILL_DELEGATED", "1")
        .args([
            "skill",
            "install",
            "--add",
            source.to_str().expect("utf8 path"),
            "--platform",
            "claude",
            "--json",
        ])
        .output()
        .expect("skill install");
    assert!(install.status.success(), "{}", stderr(&install));
    let installed: serde_json::Value =
        serde_json::from_str(stdout(&install)).expect("install json");
    assert_eq!(installed["ok"], true, "{installed}");
    assert_eq!(installed["skills"][0]["canonicalStatus"], "installed");
    assert_eq!(
        installed["skills"][0]["destinations"][0]["status"],
        "linked"
    );

    let list = workspace
        .cli()
        .env("OCTOCODE_SKILL_DELEGATED", "1")
        .args(["skill", "list", "--json"])
        .output()
        .expect("skill list");
    assert!(list.status.success(), "{}", stderr(&list));
    let listed: serde_json::Value = serde_json::from_str(stdout(&list)).expect("list json");
    assert_eq!(listed["count"], 1, "{listed}");
    assert_eq!(listed["skills"][0]["name"], "demo-skill");

    let check = workspace
        .cli()
        .env("OCTOCODE_SKILL_DELEGATED", "1")
        .args(["skill", "check", "--json"])
        .output()
        .expect("skill check");
    assert!(check.status.success(), "{}", stderr(&check));
    let checked: serde_json::Value = serde_json::from_str(stdout(&check)).expect("check json");
    assert_eq!(checked["ok"], true, "{checked}");
    assert_eq!(checked["skills"][0]["status"], "ok");

    let info = workspace
        .cli()
        .env("OCTOCODE_SKILL_DELEGATED", "1")
        .args(["skill", "info", "demo-skill"])
        .output()
        .expect("skill info");
    assert!(info.status.success(), "{}", stderr(&info));
    assert!(stdout(&info).contains("Native demo"), "{}", stdout(&info));

    let remove = workspace
        .cli()
        .env("OCTOCODE_SKILL_DELEGATED", "1")
        .args(["skill", "remove", "demo-skill", "--purge", "--json"])
        .output()
        .expect("skill remove");
    assert!(remove.status.success(), "{}", stderr(&remove));
    let removed: serde_json::Value = serde_json::from_str(stdout(&remove)).expect("remove json");
    assert_eq!(removed["ok"], true, "{removed}");
    assert!(
        !workspace.home.join("skills").join("demo-skill").exists(),
        "canonical copy must be purged"
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
        .args(["skill", "run", "demo", "--json"])
        .output()
        .expect("skill spawn");
    assert!(output.status.success(), "{}", stderr(&output));
    let recorded = std::fs::read_to_string(&log).expect("argv log");
    assert!(recorded.contains("skill"), "{recorded}");
    assert!(recorded.contains("run"), "{recorded}");
    assert!(recorded.contains("--json"), "{recorded}");
}

#[test]
fn skill_delegation_guard_breaks_native_recursion() {
    // When the `octocode` on PATH is the native binary itself (not the npm
    // launcher), the delegation guard must fail fast instead of respawning.
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("OCTOCODE_SKILL_DELEGATED", "1")
        .args(["skill", "run", "demo"])
        .output()
        .expect("skill spawn");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("native binary, not the npm CLI"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn tool_rejects_non_canonical_fields() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["astSearch", r#"{"operation":"syntax","path":"."}"#])
        .output()
        .expect("astSearch");
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
fn localfetch_pages_expose_a_rerunnable_continuation() {
    let workspace = Workspace::new();
    let content: String = (1..24).map(|n| format!("line {n}: research\n")).collect();
    let path = workspace.write("source.txt", &content);
    let query = serde_json::json!({
        "path": path,
        "chunkType": "lines",
        "chunkSize": 3,
        "goal": "test", "reasoning": "Verify paginated native reads."
    })
    .to_string();
    let first = workspace
        .cli()
        .args(["localFetch", &query])
        .output()
        .expect("first read");
    assert_eq!(first.status.code(), Some(6), "{}", stderr(&first));
    let value: serde_json::Value = serde_json::from_str(stdout(&first)).expect("page JSON");
    let first_content = value["results"][0]["data"]["content"]
        .as_str()
        .expect("first page content")
        .to_owned();
    assert!(content.starts_with(&first_content), "first page prefix");
    // Cursor tokens are per-process, so the response advertises the prefilled
    // continuation query as a directly re-runnable call.
    let call = &value["results"][0]["data"]["next"]["continue"];
    assert_eq!(call["tool"], "localFetch", "{value}");
    let continuation = serde_json::to_string(&call["query"]).expect("continuation query");
    let second = workspace
        .cli()
        .args(["localFetch", &continuation])
        .output()
        .expect("continuation read");
    let value: serde_json::Value = serde_json::from_str(stdout(&second)).expect("page two JSON");
    let second_content = value["results"][0]["data"]["content"]
        .as_str()
        .expect("second page content");
    let joined = format!("{first_content}{second_content}");
    assert!(content.starts_with(&joined), "joined prefix");
    assert!(joined.len() > first_content.len(), "second page advanced");
}

#[test]
fn tool_without_query_exits_two_with_scheme_hint() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["astSearch"])
        .output()
        .expect("astSearch no args");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        text.contains("Usage:") && text.contains("scheme astSearch"),
        "expected usage + scheme hint: {text}"
    );
}

#[test]
fn tool_with_bad_json_exits_two() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["localSearch", "not-valid-json"])
        .output()
        .expect("localSearch bad json");
    assert_eq!(exit_code(&output), Some(2));
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(text.contains("Invalid JSON"), "expected json error: {text}");
}

#[test]
fn tool_reads_query_from_input_file() {
    let workspace = Workspace::new();
    let source = workspace.write("input-source.rs", "fn from_file() {}\n");
    let query = serde_json::json!({
        "path": source,
        "goal": "test", "reasoning": "Verify --input file queries."
    })
    .to_string();
    let query_file = workspace.write("query.json", &query);
    let output = workspace
        .cli()
        .args(["localFetch", "--input", query_file.to_str().expect("utf8")])
        .output()
        .expect("localFetch --input");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("tool JSON");
    assert!(
        value["results"][0]["data"]["content"]
            .as_str()
            .is_some_and(|content| content.contains("from_file"))
    );
}

#[test]
fn scheme_lists_the_compact_discovery_catalog() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["scheme", "--compact"])
        .output()
        .expect("scheme");
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
    assert_eq!(value["commands"]["schema"], "scheme <name>");
    assert_eq!(value["commands"]["run"], "<name> '<json>'");
    assert!(
        value.get("instructions").is_none(),
        "instructions are core-delivered by the JS layers, not the binary: {value}"
    );
    assert!(
        value["fingerprint"]
            .as_str()
            .is_some_and(|fingerprint| fingerprint.len() == 64),
        "machine catalog must carry the enforcement fingerprint: {value}"
    );
    let first = &value["tools"][0];
    assert!(first["name"].is_string());
    let contract = octocode_native::contracts::parsed_contract().expect("embedded contract");
    let expected_short = contract["tools"]
        .as_array()
        .expect("contract tools")
        .iter()
        .find(|tool| tool["name"] == first["name"])
        .and_then(|tool| tool["shortDescription"].as_str())
        .expect("core shortDescription");
    assert_eq!(first["description"], expected_short);
    assert!(
        first["description"]
            .as_str()
            .is_some_and(|text| text.len() <= 96)
    );
    assert!(first["fields"].is_string());
    // Union tools list every mode instead of an empty field list.
    for tool in value["tools"].as_array().expect("tools array") {
        assert_ne!(tool["fields"], "[]", "{}", tool["name"]);
    }
    let fields_of = |name: &str| {
        value["tools"]
            .as_array()
            .and_then(|tools| tools.iter().find(|tool| tool["name"] == name))
            .and_then(|tool| tool["fields"].as_str())
            .unwrap_or_default()
            .to_owned()
    };
    assert!(fields_of("ghStructure").contains("owner*"));
    assert!(fields_of("ghSearchCode").contains("owner*"));
    assert!(fields_of("astSearch").contains("operation=match(rule)"));
    assert!(fields_of("astTopology").starts_with("analysis=deadCode["));
    assert!(first["availability"]["enabled"].is_boolean());
    let clone_tool = value["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .find(|tool| tool["name"] == "ghCloneRepo")
        .expect("clone tool");
    assert_eq!(clone_tool["availability"]["enabled"], true);
    assert!(first.get("inputSchema").is_none());
    assert!(first.get("outputSchema").is_none());
}

#[test]
fn scheme_prints_the_public_tool_contract_without_output_schema() {
    let workspace = Workspace::new();
    let help = workspace
        .cli()
        .args(["scheme", "--help"])
        .output()
        .expect("scheme help");
    assert!(help.status.success(), "{}", stderr(&help));
    let help = stdout(&help);
    assert!(help.contains("public tool contract"), "{help}");
    assert!(!help.contains("full contract"), "{help}");

    let output = workspace
        .cli()
        .args(["scheme", "localSearch", "--compact"])
        .output()
        .expect("scheme localSearch");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("contract JSON");
    assert_eq!(value["name"], "localSearch");
    assert_eq!(
        value["shortDescription"],
        "Find literal or regex matches in local files."
    );
    // instructions live at catalog level (scheme with no args), not per-tool.
    assert!(value["instructions"].is_null());
    assert!(value["inputSchema"].is_object());
    assert!(!value.to_string().contains("\"outputSchema\""));
    // The per-tool view echoes the concrete run command for the inspected tool.
    assert_eq!(value["run"], "octocode localSearch '<json>'");

    let full = workspace
        .cli()
        .args(["scheme", "localSearch", "--view", "full", "--compact"])
        .output()
        .expect("scheme localSearch full");
    assert!(full.status.success(), "{}", stderr(&full));
    let full: serde_json::Value = serde_json::from_str(stdout(&full)).expect("full contract JSON");
    assert!(full["inputSchema"].is_object());
    assert!(!full.to_string().contains("\"outputSchema\""));
}

#[test]
fn scheme_unknown_tool_exits_two_and_lists_known_names() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["scheme", "notATool"])
        .output()
        .expect("scheme unknown");
    assert_eq!(exit_code(&output), Some(2));
    let text = stderr(&output);
    assert!(text.contains("Unknown tool: notATool"), "{text}");
    assert!(text.contains("localSearch"), "known names missing: {text}");
}

#[test]
fn scheme_query_view_selects_a_single_union_branch() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args([
            "scheme",
            "ghSearchHistory",
            "--view",
            "query",
            "--select",
            "operation=commit",
            "--compact",
        ])
        .output()
        .expect("scheme select");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("schema JSON");
    assert_eq!(value["name"], "ghSearchHistory");
    // instructions live at catalog level (scheme with no args), not per-tool.
    assert!(value["instructions"].is_null());
    assert_eq!(
        value["querySchema"]["oneOf"].as_array().map(Vec::len),
        Some(1)
    );
    assert!(!value.to_string().contains("\"outputSchema\""));
}

#[test]
fn tool_accepts_bulk_queries() {
    let workspace = Workspace::new();
    let first = workspace.write("query-one.rs", "fn query_one() {}\n");
    let second = workspace.write("query-two.rs", "fn query_two() {}\n");
    let query = serde_json::json!([
        {
            "path": first,
            "startLine": 1,
            "endLine": 1,
            "goal": "test", "reasoning": "Verify the first native bulk query."
        },
        {
            "path": second,
            "startLine": 1,
            "endLine": 1,
            "goal": "test", "reasoning": "Verify the second native bulk query."
        }
    ])
    .to_string();
    let output = workspace
        .cli()
        .args(["localFetch", &query])
        .output()
        .expect("bulk queries");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("tool JSON");
    let rows = value["results"].as_array().expect("bulk rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["index"], 0);
    assert_eq!(rows[1]["index"], 1);
    assert!(
        rows[0]["data"]["content"]
            .as_str()
            .is_some_and(|content| content.contains("query_one"))
    );
    assert!(
        rows[1]["data"]["content"]
            .as_str()
            .is_some_and(|content| content.contains("query_two"))
    );
}

#[test]
fn localsearch_emits_structured_results() {
    let workspace = Workspace::new();
    let path = workspace.write("search.rs", "fn needle() {}\n");
    let query = serde_json::json!({
        "searchText": "needle",
        "path": path,
        "resultView": "matchOnly",
        "goal": "test", "reasoning": "Verify structured lexical output."
    })
    .to_string();
    let output = workspace
        .cli()
        .args(["localSearch", &query])
        .output()
        .expect("localSearch");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value =
        serde_json::from_str(stdout(&output)).expect("one JSON document");
    let data = &value["results"][0]["data"];
    // A complete single page carries no stats: the listed rows are the counts.
    assert!(data.get("stats").is_none(), "{data}");
    assert_eq!(data["files"][0]["matches"][0]["value"], "needle", "{data}");
}

#[test]
fn astrewrite_previews_then_applies_with_hash_guards() {
    let workspace = Workspace::new();
    let path = workspace.write("rewrite.rs", "pub const VALUE: u32 = 2;\n");
    let query = serde_json::json!({
        "path": path,
        "langType": "rust",
        "ruleKind": "pattern",
        "pattern": "pub const $NAME: u32 = $VALUE;",
        "rewrite": "pub const $NAME: u64 = $VALUE;",
        "goal": "test", "reasoning": "Verify guarded native rewrite application."
    });
    let preview = workspace
        .cli()
        .env("OCTOCODE_BETA", "true")
        .args(["astRewrite", &query.to_string()])
        .output()
        .expect("astRewrite preview");
    assert!(
        preview.status.success(),
        "{}{}",
        stdout(&preview),
        stderr(&preview)
    );
    let value: serde_json::Value = serde_json::from_str(stdout(&preview)).expect("preview JSON");
    let data = &value["results"][0]["data"];
    assert_eq!(data["mode"], "preview", "{data}");
    // Apply replays the preview's next.apply verbatim: it binds the snapshot
    // and the expected before-hashes.
    let apply = data["next"]["apply"]["query"].clone();
    assert_eq!(apply["apply"], true, "{data}");
    assert!(apply["snapshot"].is_string(), "{data}");
    assert!(apply["expectedHashes"].is_object(), "{data}");
    let query = apply;
    let output = workspace
        .cli()
        .env("OCTOCODE_BETA", "true")
        .args(["astRewrite", &query.to_string()])
        .output()
        .expect("astRewrite apply");
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
    let query = serde_json::json!({
        "path": missing,
        "goal": "test", "reasoning": "Verify native read errors."
    })
    .to_string();
    let output = workspace
        .cli()
        .args(["--json-errors", "localFetch", &query])
        .output()
        .expect("missing read");
    assert_eq!(exit_code(&output), Some(3));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("JSON error");
    assert_eq!(value["results"][0]["data"]["errorCode"], "fileAccessFailed");
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
    assert_eq!(error["kind"], "octocode.toolError");
    assert_eq!(error["tool"], "localSearch");
    assert!(stderr(&malformed).is_empty(), "{}", stderr(&malformed));

    let unknown = workspace
        .cli()
        .args(["--json-errors", "notACommand"])
        .output()
        .expect("unknown subcommand");
    assert_eq!(exit_code(&unknown), Some(2));
    let error: serde_json::Value =
        serde_json::from_str(stdout(&unknown)).expect("machine-readable clap error");
    assert_eq!(error["kind"], "octocode.toolError");
    assert!(
        error["error"]
            .as_str()
            .is_some_and(|message| message.contains("notACommand")),
        "{error}"
    );
    assert!(stderr(&unknown).is_empty(), "{}", stderr(&unknown));
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

#[tokio::test]
async fn github_authentication_failure_uses_exit_four_and_actionable_hint() {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "message": "Bad credentials"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let mut command = workspace.cli();
    command
        .env("GITHUB_API_URL", format!("{}/api/v3", server.uri()))
        .env("OCTOCODE_TOKEN", "invalid-fixture-token")
        .args([
            "ghGetFileContent",
            r#"{"owner":"fixture","repo":"fixture","path":"README","forceRefresh":true,"goal":"Read the fixture README.","reasoning":"Exercise the authentication failure path."}"#,
        ]);
    let output = tokio::task::spawn_blocking(move || command.output().expect("tool output"))
        .await
        .expect("tool worker");
    assert_eq!(output.status.code(), Some(4), "{}", stdout(&output));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("tool JSON");
    assert_eq!(result["results"][0]["data"]["errorCode"], "authentication");
    assert!(stdout(&output).contains("octocode auth login"));
    assert!(stdout(&output).contains("invalid env token"));
    assert!(!stdout(&output).contains("invalid-fixture-token"));
}
