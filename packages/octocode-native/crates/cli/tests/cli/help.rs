use super::*;

#[test]
fn root_help_lists_all_tools_independent_of_mcp_gates_and_filters() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("OCTOCODE_ENABLE_LOCAL", "false")
        .env("OCTOCODE_BETA", "false")
        .env("OCTOCODE_ENABLE_CLONE", "false")
        .env("OCTOCODE_CLASSIFICATION_API", "")
        .env("TOOLS_TO_RUN", "localFetch")
        .env("DISABLE_TOOLS", "localFetch,clasify")
        .arg("--help")
        .output()
        .expect("help");
    assert!(output.status.success());
    let text = stdout(&output);
    let tools = octocode_native::id::ToolId::ALL;
    assert_eq!(tools.len(), 16);
    for tool in tools {
        assert!(
            text.contains(&format!("\n  {} ", tool.as_str())),
            "missing {}: {text}",
            tool.as_str()
        );
    }
    // Each tool retains its canonical JSON input and continuation help.
    let own = workspace
        .cli()
        .args(["clasify", "--help"])
        .output()
        .expect("tool help");
    assert!(own.status.success(), "{}", stderr(&own));
    // JSON only: no positional fields to point at, no trailing space.
    let own = stdout(&own);
    assert!(own.contains("{\"queries\":[…]}"), "{own}");
    assert!(own.contains("{\"cursor\":\"…\"}"), "{own}");
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
fn tool_help_uses_canonical_core_titles() {
    let workspace = Workspace::new();
    let contract = octocode_native::contracts::parsed_contract().expect("embedded contract");
    let root = all_tools_enabled(workspace.cli().arg("--help"))
        .output()
        .expect("root help");
    assert!(root.status.success());
    // The tool commands are exactly the contract tools: a tool dropped from
    // the contract must not linger as a command, and none may be missing.
    let system = [
        "schema", "config", "auth", "skill", "install", "mcp", "help",
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
        let title = tool["title"].as_str().expect("core title");
        assert!(!title.is_empty());
        assert!(
            root_text.contains(title),
            "missing canonical {name} title in root help"
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
            assert!(text.contains(title), "{name} {flag}: {text}");
            assert_eq!(
                text.contains(
                    "MCP beta (set OCTOCODE_BETA=true or local.beta:true); available in CLI."
                ),
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
