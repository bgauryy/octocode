use super::*;

#[test]
fn root_help_lists_only_available_tools() {
    let workspace = Workspace::new();
    let output = workspace.cli().arg("--help").output().expect("help");
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(
        !text.contains("clasify"),
        "clasify offered while disabled: {text}"
    );
    assert!(text.contains("\n  localSearch"), "{text}");
    // A hidden tool stays callable by name and keeps its own help.
    let own = workspace
        .cli()
        .args(["clasify", "--help"])
        .output()
        .expect("tool help");
    assert!(own.status.success(), "{}", stderr(&own));
    // JSON only: no positional fields to point at, no trailing space.
    let own = stdout(&own);
    assert!(own.contains("Usage: octocode clasify [OPTIONS]\n"), "{own}");
    assert!(!own.contains("positional fields"), "{own}");
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
        "mcp",
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
        "schema", "config", "auth", "graph", "skill", "install", "mcp", "help",
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
        // A parked (contract `disabled`) tool is no command.
        .filter(|tool| tool["disabled"] != true)
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    listed_tools.sort_unstable();
    contract_tools.sort_unstable();
    assert_eq!(listed_tools, contract_tools);
    let root_text = stdout(&root)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for tool in contract["tools"]
        .as_array()
        .expect("contract tools")
        .iter()
        .filter(|tool| tool["disabled"] != true)
    {
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
            assert_eq!(
                text.contains("Beta, disabled by default (set OCTOCODE_BETA=true"),
                tool["beta"] == true,
                "{name} {flag} names the beta gate exactly when the tool is beta: {text}"
            );
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

/// A tool the contract parks (`disabled`) is refused on the CLI even with the
/// beta gate on, and neither `--help` nor `schema` lists it. Vacuous until
/// the contract parks a tool; then it covers each one.
#[test]
fn disabled_tools_are_refused_and_absent_from_help_and_schema() {
    use octocode_native::id::ToolId;
    let workspace = Workspace::new();
    for tool in ToolId::ALL.into_iter().filter(|id| id.is_disabled()) {
        let name = tool.as_str();
        let help = workspace.cli().arg("--help").output().expect("help");
        assert!(
            !stdout(&help)
                .lines()
                .any(|line| line.trim_start().starts_with(&format!("{name} "))),
            "{name} in help: {}",
            stdout(&help)
        );
        let schema = workspace.cli().arg("schema").output().expect("schema");
        let catalog: serde_json::Value = serde_json::from_slice(&schema.stdout).expect("schema");
        assert!(
            catalog["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .all(|listed| listed["name"] != name),
            "{name} listed: {catalog}"
        );
        let call = workspace
            .cli()
            .env("OCTOCODE_BETA", "true")
            .args([name, "{}", "--json"])
            .output()
            .expect("call");
        assert_eq!(exit_code(&call), Some(5), "{}", stdout(&call));
        let error: serde_json::Value = serde_json::from_slice(&call.stdout).expect("envelope");
        assert_eq!(error["errorCode"], "toolUnavailable", "{error}");
        assert!(
            error["error"]
                .as_str()
                .is_some_and(|text| text.contains("disabled in this build")),
            "{error}"
        );
    }
}
