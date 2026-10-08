// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used)]

use crate::support;

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

/// Opens every availability gate so root help lists every contract tool.
fn all_tools_enabled(command: &mut std::process::Command) -> &mut std::process::Command {
    command
        .env("OCTOCODE_CLASSIFICATION_API", "fixture-key")
        .env("OCTOCODE_BETA", "true")
}

#[test]
fn root_help_lists_only_available_tools() {
    let workspace = Workspace::new();
    let output = workspace.cli().arg("--help").output().expect("help");
    assert!(output.status.success());
    let text = stdout(&output);
    for gated in ["clasify", "astRewrite", "astTopology"] {
        assert!(
            !text.contains(gated),
            "{gated} offered while disabled: {text}"
        );
    }
    assert!(text.contains("\n  localSearch"), "{text}");
    // A hidden tool stays callable by name and keeps its own help.
    let own = workspace
        .cli()
        .args(["clasify", "--help"])
        .output()
        .expect("tool help");
    assert!(own.status.success(), "{}", stderr(&own));
}

#[test]
fn help_lists_only_the_minimal_command_surface() {
    let workspace = Workspace::new();
    let output = all_tools_enabled(workspace.cli().arg("--help"))
        .output()
        .expect("help");
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
        "schema",
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
        "\n  scheme",
        "\n  showConfig",
        "\n  catalog",
        "\n  serve",
    ] {
        assert!(!text.contains(removed), "alias leaked into help: {removed}");
    }
}

#[test]
fn clasify_missing_key_is_actionable() {
    let workspace = Workspace::new();
    let query = serde_json::json!({"queries":[{
        "id":"decision",
        "mainGoal": "test", "reasoning":"Choose the next inspection.",
        "resources":[{"id":"observed","value":{"fact":"present"}}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Is it relevant?"}]
    }]});
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
fn clasify_with_every_resource_failed_exits_by_failure_class() {
    let workspace = Workspace::new();
    let run = |question: serde_json::Value| {
        let query = serde_json::json!({"queries":[{
            "mainGoal": "test", "reasoning":"Exercise failure exit codes.",
            "resources":[{"id":"observed","value":{"fact":"present"}}],
            "questions":[question]
        }]});
        workspace
            .cli()
            .env("OCTOCODE_CLASSIFICATION_API", "dummy")
            .env("OCTOCODE_CLASSIFICATION_API_HOST", "http://127.0.0.1:9")
            .args(["clasify", &query.to_string()])
            .output()
            .expect("clasify execution")
    };
    // A provider that cannot be reached is an execution failure, not bad input.
    let unreachable = run(serde_json::json!({"type":"yesno","ask":"Is it relevant?"}));
    assert_eq!(exit_code(&unreachable), Some(5), "{}", stdout(&unreachable));
    // locate over supplied state rejects the caller's request.
    let unsupported = run(serde_json::json!({"type":"locate","ask":"fact"}));
    assert_eq!(exit_code(&unsupported), Some(2), "{}", stdout(&unsupported));
    assert!(
        stdout(&unsupported).contains("classificationLocateUnsupported"),
        "{}",
        stdout(&unsupported)
    );
}

#[test]
fn tool_output_is_compact_json_on_a_pipe() {
    let workspace = Workspace::new();
    let query = serde_json::json!({"queries":[{
        "id":"decision",
        "mainGoal": "test", "reasoning":"Exercise output formatting.",
        "resources":[{"id":"observed","value":{"fact":"present"}}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Is it relevant?"}]
    }]})
    .to_string();
    let compact = workspace.cli().args(["clasify", &query]).output().unwrap();
    assert_eq!(
        stdout(&compact).trim_end().lines().count(),
        1,
        "{}",
        stdout(&compact)
    );
    let forced = workspace
        .cli()
        .args(["clasify", &query, "--json"])
        .output()
        .unwrap();
    assert_eq!(stdout(&forced), stdout(&compact));
    for retired in ["--compact", "--pretty"] {
        let output = workspace
            .cli()
            .args(["clasify", &query, retired])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "tool commands reject {retired}"
        );
    }
}

#[test]
fn blank_classification_key_disables_clasify_despite_home_and_vendor_keys() {
    let workspace = Workspace::new();
    std::fs::write(
        workspace.home.join(".env"),
        "OCTOCODE_CLASSIFICATION_API=from-home-env\n",
    )
    .unwrap();
    let query = serde_json::json!({"queries":[{
        "id":"decision",
        "mainGoal": "test", "reasoning":"Exercise the opt-out.",
        "resources":[{"id":"observed","value":{"fact":"present"}}],
        "questions":[{"id":"relevant","type":"yesno","ask":"Is it relevant?"}]
    }]})
    .to_string();
    let output = workspace
        .cli()
        .env("OCTOCODE_CLASSIFICATION_API", "")
        // The retired vendor alias is not a credential.
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
    let root = all_tools_enabled(workspace.cli().arg("--help"))
        .output()
        .expect("root help");
    assert!(root.status.success());
    // The tool commands are exactly the contract tools: a tool dropped from
    // the contract must not linger as a command, and none may be missing.
    let system = [
        "schema", "config", "auth", "graph", "skill", "install", "help",
    ];
    let mut listed_tools = stdout(&root)
        .split("Commands:")
        .nth(1)
        .expect("commands section")
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| !system.contains(name))
        .collect::<Vec<_>>();
    let mut contract_tools = contract["tools"]
        .as_array()
        .expect("contract tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    listed_tools.sort_unstable();
    contract_tools.sort_unstable();
    assert_eq!(listed_tools, contract_tools);
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
        "scheme",
        "showConfig",
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
    // Every skill subcommand runs in the npm launcher; with none on PATH the
    // host fails closed and names the rerun command.
    for args in [["skill", "list"], ["skill", "run"]] {
        let skill = workspace.cli().args(args).output().expect("skill");
        assert_eq!(skill.status.code(), Some(1));
        let text = format!("{}{}", stdout(&skill), stderr(&skill));
        assert!(
            text.contains(&format!("npx -y octocode {}", args.join(" "))),
            "{text}"
        );
    }
}

#[test]
fn auth_status_reports_an_unreachable_env_token_as_unverified() {
    let workspace = Workspace::new();
    // `auth` requires its subcommand: `status` is not implied.
    let bare = workspace.cli().arg("auth").output().expect("bare auth");
    assert_eq!(exit_code(&bare), Some(2), "{}", stdout(&bare));
    // Unreachable API: the token cannot be verified, so it stays
    // authenticated but says so.
    let output = workspace
        .cli()
        .env("GITHUB_TOKEN", "fixture-pat")
        .env("GITHUB_API_URL", "http://127.0.0.1:1/api/v3")
        .args(["auth", "status", "--json"])
        .output()
        .expect("auth status");
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("auth json");
    assert_eq!(value["authenticated"], true, "{value}");
    assert_eq!(value["verification"], "unverified", "{value}");
    assert_eq!(value["tokenSource"], "env", "{value}");
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
    for scope in ["global", "project"] {
        assert!(value["envFiles"][scope]["path"].is_string(), "{value}");
        assert!(value["envFiles"][scope]["exists"].is_boolean(), "{value}");
    }
    assert!(value["envKeys"].is_array());

    let human = workspace.cli().arg("config").output().expect("config");
    assert!(human.status.success(), "{}", stderr(&human));
    let text = stdout(&human);
    assert!(text.contains("\nconfig\n  global "), "{text}");
    assert!(text.contains("\n.env\n  global "), "{text}");
    assert!(
        text.contains("Values are never printed."),
        "missing no-values note: {text}"
    );
}

#[test]
fn config_check_reports_set_state_without_the_value() {
    let workspace = Workspace::new();
    let set = workspace
        .cli()
        .env("GITHUB_TOKEN", "fixture-secret")
        .args(["config", "check", "GITHUB_TOKEN"])
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
        .args(["config", "check", "OCTOCODE_NOT_A_KEY"])
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
    assert_eq!(preview["entry"]["method"], "npx", "{preview}");
    assert_eq!(preview["entry"]["customCommand"], false, "{preview}");
    assert!(
        preview.get("config").is_none(),
        "dry-run must not expose saved configuration: {preview}"
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
fn tool_rejects_non_canonical_fields() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args([
            "astSearch",
            r#"{"queries":[{"operation":"syntax","path":"."}]}"#,
        ])
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
    // Line pages carry their source numbers (`<line>\t<text>`).
    let numbered: String = (1..24)
        .map(|n| format!("{n}\tline {n}: research\n"))
        .collect();
    let query = serde_json::json!({"queries":[{
        "path": path,
        "unit": "lines",
        "length": 3,
        "mainGoal": "test", "reasoning": "Verify paginated native reads."
    }]})
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
    assert!(numbered.starts_with(&first_content), "first page prefix");
    // Cursor tokens are per-process, so the response advertises the prefilled
    // continuation query as a directly re-runnable call.
    let call = &value["results"][0]["data"]["next"]["continue"];
    assert_eq!(call["tool"], "localFetch", "{value}");
    let continuation = serde_json::to_string(&serde_json::json!({"queries":[call["query"]]}))
        .expect("continuation query");
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
    assert!(numbered.starts_with(&joined), "joined prefix");
    assert!(joined.len() > first_content.len(), "second page advanced");
}

#[test]
fn tool_without_query_exits_two_with_schema_hint() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .args(["astSearch"])
        .output()
        .expect("astSearch no args");
    assert_eq!(exit_code(&output), Some(2));
    // A pipe is JSON mode: the usage error is the envelope on stdout.
    let error: serde_json::Value = serde_json::from_str(stdout(&output)).expect("JSON error");
    assert_eq!(error["kind"], "octocode.toolError", "{error}");
    assert_eq!(error["tool"], "astSearch", "{error}");
    assert_eq!(error["errorCode"], "invalidInput", "{error}");
    let message = error["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("Missing JSON query") && message.contains("octocode schema astSearch"),
        "expected usage + schema hint: {message}"
    );
    assert!(stderr(&output).is_empty(), "{}", stderr(&output));
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
fn tool_reads_query_from_stdin() {
    use std::io::Write;
    let workspace = Workspace::new();
    let source = workspace.write("stdin-source.rs", "fn from_stdin() {}\n");
    let query = serde_json::json!({"queries":[{"path": source}]}).to_string();
    let mut child = workspace
        .cli()
        .args(["localFetch", "--input", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("localFetch --input -");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(query.as_bytes())
        .expect("write query");
    let output = child.wait_with_output().expect("output");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("from_stdin"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn tool_reads_query_from_input_file() {
    let workspace = Workspace::new();
    let source = workspace.write("input-source.rs", "fn from_file() {}\n");
    let query = serde_json::json!({"queries":[{
        "path": source,
        "mainGoal": "test", "reasoning": "Verify --input file queries."
    }]})
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
fn catalog_lists_the_machine_catalog_for_the_launcher() {
    let workspace = Workspace::new();
    let output = workspace.cli().arg("catalog").output().expect("catalog");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        output.stdout.len() < 20_000,
        "machine catalog should stay token-efficient, got {} bytes",
        output.stdout.len()
    );
    assert_eq!(stdout(&output).trim_end().lines().count(), 1);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("catalog JSON");
    // Presentation (kind, commands, descriptions, instructions) is composed
    // by the npm launcher's `schema`; the binary carries machine facts only.
    for presentation in ["kind", "commands", "output", "instructions"] {
        assert!(value.get(presentation).is_none(), "{presentation}: {value}");
    }
    assert!(
        value["fingerprint"]
            .as_str()
            .is_some_and(|fingerprint| fingerprint.len() == 64),
        "machine catalog must carry the enforcement fingerprint: {value}"
    );
    let first = &value["tools"][0];
    assert!(first["name"].is_string());
    assert!(first.get("description").is_none(), "{first}");
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
    assert!(fields_of("astTopology").starts_with("operation=deadCode["));
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

/// `schema` is served by the npm launcher; the binary owns its help and
/// names the launcher when run directly.
#[test]
fn schema_help_is_native_and_execution_names_the_launcher() {
    let workspace = Workspace::new();
    let help = workspace
        .cli()
        .args(["schema", "--help"])
        .output()
        .expect("schema help");
    assert!(help.status.success(), "{}", stderr(&help));
    let help = stdout(&help);
    assert!(
        help.contains("octocode schema <tool> --view query"),
        "{help}"
    );
    assert!(help.contains("--select"), "{help}");

    let output = workspace
        .cli()
        .args(["schema", "localSearch"])
        .output()
        .expect("schema localSearch");
    assert_eq!(exit_code(&output), Some(5));
    let error: serde_json::Value = serde_json::from_str(stdout(&output)).expect("JSON error");
    assert_eq!(error["kind"], "octocode.toolError", "{error}");
    assert!(
        error["error"]
            .as_str()
            .is_some_and(|message| message.contains("npm launcher")),
        "{error}"
    );
    // --view takes only the views the launcher serves.
    let bad_view = workspace
        .cli()
        .args(["schema", "localSearch", "--view", "bad"])
        .output()
        .expect("schema bad view");
    assert_eq!(exit_code(&bad_view), Some(2));
}

#[test]
fn tool_accepts_bulk_queries() {
    let workspace = Workspace::new();
    let first = workspace.write("query-one.rs", "fn query_one() {}\n");
    let second = workspace.write("query-two.rs", "fn query_two() {}\n");
    let query = serde_json::json!({"queries":[
        {
            "path": first,
            "ranges": ["1-1"],
            "mainGoal": "test", "reasoning": "Verify the first native bulk query."
        },
        {
            "path": second,
            "ranges": ["1-1"],
            "mainGoal": "test", "reasoning": "Verify the second native bulk query."
        }
    ]})
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

/// Host spellings of line ranges read the same lines as the canonical form.
#[test]
fn localfetch_accepts_host_line_range_spellings() {
    let workspace = Workspace::new();
    let lines: String = (1..=10).map(|n| format!("line {n}\n")).collect();
    let path = workspace.write("ranges.txt", &lines);
    for ranges in [
        serde_json::json!("2,4"),
        serde_json::json!([" 2-4"]),
        serde_json::json!(["2", "4"]),
        serde_json::json!([2, 4]),
    ] {
        let query = serde_json::json!({"queries":[{
            "path": path, "ranges": ranges,
            "mainGoal": "test", "reasoning": "Verify tolerant line ranges."
        }]})
        .to_string();
        let output = workspace
            .cli()
            .args(["localFetch", &query])
            .output()
            .expect("ranges query");
        assert!(output.status.success(), "{ranges}: {}", stderr(&output));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("tool JSON");
        let content = value["results"][0]["data"]["content"]
            .as_str()
            .unwrap_or_default();
        for line in ["line 2", "line 3", "line 4"] {
            assert!(content.contains(line), "{ranges}: {value}");
        }
        assert!(!content.contains("line 5"), "{ranges}: {value}");
    }
}

#[test]
fn localsearch_emits_structured_results() {
    let workspace = Workspace::new();
    let path = workspace.write("search.rs", "fn needle() {}\n");
    let query = serde_json::json!({"queries":[{
        "matchString": "needle",
        "path": path,
        "resultView": "matchOnly",
        "mainGoal": "test", "reasoning": "Verify structured lexical output."
    }]})
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
    let query = serde_json::json!({"queries":[{
        "path": path,
        "language": "rust",
        "pattern": "pub const $NAME: u32 = $VALUE;",
        "rewrite": "pub const $NAME: u64 = $VALUE;",
        "mainGoal": "test", "reasoning": "Verify guarded native rewrite application."
    }]});
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
    // Apply replays the preview's hints.apply verbatim: it binds the snapshot
    // and the expected before-hashes.
    let query = data["hints"]["apply"]["query"].clone();
    let apply = &query["queries"][0];
    assert_eq!(apply["apply"], true, "{data}");
    assert!(apply["snapshot"].is_string(), "{data}");
    assert!(apply["expectedHashes"].is_object(), "{data}");
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
fn json_mode_errors_go_to_stdout_without_stderr() {
    let workspace = Workspace::new();
    let missing = workspace.workspace.join("missing.rs");
    let query = serde_json::json!({"queries":[{
        "path": missing,
        "mainGoal": "test", "reasoning": "Verify native read errors."
    }]})
    .to_string();
    let output = workspace
        .cli()
        .args(["localFetch", &query])
        .output()
        .expect("missing read");
    assert_eq!(exit_code(&output), Some(3));
    let value: serde_json::Value = serde_json::from_str(stdout(&output)).expect("JSON error");
    assert_eq!(value["results"][0]["data"]["errorCode"], "pathNotFound");
    assert!(
        stderr(&output).is_empty(),
        "duplicate stderr: {}",
        stderr(&output)
    );

    let malformed = workspace
        .cli()
        .args(["localSearch", "{"])
        .output()
        .expect("malformed raw JSON");
    assert_eq!(exit_code(&malformed), Some(2));
    let error: serde_json::Value =
        serde_json::from_str(stdout(&malformed)).expect("machine-readable parse error");
    assert_eq!(error["kind"], "octocode.toolError");
    assert_eq!(error["tool"], "localSearch");
    assert_eq!(error["errorCode"], "invalidInput", "{error}");
    assert!(stderr(&malformed).is_empty(), "{}", stderr(&malformed));

    // A schema-invalid query answers with the same typed envelope MCP
    // returns as structuredContent, repair details kept.
    let typo = workspace
        .cli()
        .args([
            "localFetch",
            r#"{"queries":[{"path":"a.rs","matchstring":"x"}]}"#,
        ])
        .output()
        .expect("schema-invalid query");
    assert_eq!(exit_code(&typo), Some(2));
    let error: serde_json::Value =
        serde_json::from_str(stdout(&typo)).expect("machine-readable input error");
    assert_eq!(error["kind"], "octocode.toolError", "{error}");
    assert_eq!(error["tool"], "localFetch", "{error}");
    assert_eq!(error["errorCode"], "invalidInput", "{error}");
    assert!(
        error["details"]
            .as_array()
            .is_some_and(|details| details.iter().any(|d| d
                .as_str()
                .is_some_and(|d| d.contains("did you mean 'matchString'?")))),
        "{error}"
    );

    let unknown = workspace
        .cli()
        .args(["notACommand"])
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

    // A mistyped tool keeps clap's did-you-mean names in the envelope.
    let typo = workspace
        .cli()
        .args(["lokalSearch", "{}"])
        .output()
        .expect("typo subcommand");
    assert_eq!(exit_code(&typo), Some(2));
    let error: serde_json::Value = serde_json::from_str(stdout(&typo)).expect("JSON error");
    assert!(
        error["error"]
            .as_str()
            .is_some_and(|message| message.contains("localSearch")),
        "{error}"
    );

    // Retired global flags are argument errors, not aliases.
    for flag in ["--json-errors", "--redact-emails", "--no-color"] {
        let retired = workspace
            .cli()
            .args([flag, "localSearch", "{}"])
            .output()
            .expect("retired flag");
        assert_eq!(exit_code(&retired), Some(2), "{flag}");
    }
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
        assert_eq!(error["kind"], "octocode.toolError", "{error}");
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
        .env("GITHUB_TOKEN", "invalid-fixture-token")
        .args([
            "ghGetFileContent",
            r#"{"queries":[{"owner":"fixture","repo":"fixture","path":"README","forceRefresh":true,"mainGoal":"Read the fixture README.","reasoning":"Exercise the authentication failure path."}]}"#,
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
