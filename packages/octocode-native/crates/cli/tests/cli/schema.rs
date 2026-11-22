use super::*;

#[test]
fn schema_catalog_lists_enabled_tools_with_discoverable_fields() {
    let workspace = Workspace::new();
    let output = workspace
        .cli()
        .env("OCTOCODE_STORAGE_MODE", "persistent")
        .args(["schema", "--view", "full"])
        .output()
        .expect("catalog");
    assert!(output.status.success(), "{}", stderr(&output));
    let catalog: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    let tools = catalog["tools"].as_array().expect("tools");
    assert_eq!(catalog["toolCount"], tools.len());
    for tool in tools {
        assert!(
            tool["fields"]
                .as_str()
                .is_some_and(|fields| !fields.is_empty() && fields != "[]"),
            "{tool}"
        );
        let name = tool["name"].as_str().expect("name");
        let output = workspace
            .cli()
            .env("OCTOCODE_STORAGE_MODE", "persistent")
            .args(["schema", name])
            .output()
            .expect("tool schema");
        assert!(output.status.success(), "{name}: {}", stdout(&output));
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
        assert_eq!(value["availability"]["enabled"], true, "{value}");
        assert!(value["querySchema"].is_object(), "{value}");
    }
}

/// The catalog, its instructions and the read tools' fields follow the key:
/// the same gate MCP serves.
#[test]
fn schema_lists_clasify_only_with_a_key() {
    let workspace = Workspace::new();
    let run = |key: Option<&str>, args: &[&str]| {
        let mut command = workspace.cli();
        if let Some(key) = key {
            command.env("OCTOCODE_CLASSIFICATION_API", key);
        }
        let output = command.args(args).output().expect("schema");
        assert!(output.status.success(), "{args:?}: {}", stdout(&output));
        serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("JSON")
    };
    let presentation = octocode_runtime_base::contracts::cli_presentation().expect("presentation");
    let scout = presentation["instructions"]["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .find(|block| block["tool"] == "clasify")
        .and_then(|block| block["text"].as_str())
        .expect("clasify block");
    let contract_tools = octocode_runtime_base::id::ToolId::ALL.len();
    for (key, listed) in [
        (None, false),
        (Some("  "), false),
        (Some("fixture-key"), true),
    ] {
        let compact = run(key, &["schema"]);
        let full = run(key, &["schema", "--view", "full"]);
        for catalog in [&compact, &full] {
            let tools = catalog["tools"].as_array().expect("tools");
            let names: Vec<_> = tools
                .iter()
                .filter_map(|tool| tool["name"].as_str())
                .collect();
            assert_eq!(names.contains(&"clasify"), listed, "{key:?}: {names:?}");
            assert_eq!(catalog["toolCount"], tools.len(), "{key:?}");
            // MCP exposure settings do not remove CLI commands.
            assert_eq!(
                tools.len(),
                contract_tools - usize::from(!listed),
                "{key:?}: {names:?}"
            );
        }
        let text = full["instructions"].as_str().expect("instructions");
        assert_eq!(text.contains(scout), listed, "{key:?}");
        assert_eq!(text.contains("clasify"), listed, "{key:?}: {text}");
        // No search or read takes an inline filter, with a key or without.
        let local = run(key, &["schema", "localSearch"]);
        assert!(
            local["querySchema"]["properties"]
                .get("classification")
                .is_none(),
            "{key:?}"
        );
    }
}

/// The guide the CLI catalog points to, whole, as the skill's own file.
#[test]
fn schema_flow_prints_the_tool_flow_guide() {
    let workspace = Workspace::new();
    let flow = workspace
        .cli()
        .args(["schema", "--view", "flow"])
        .output()
        .expect("flow");
    assert!(flow.status.success(), "{}", stderr(&flow));
    let skill = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../skills/octocode-research/references/tool-flow.md");
    assert_eq!(
        stdout(&flow),
        std::fs::read_to_string(skill).expect("skill file")
    );
    let output = workspace
        .cli()
        .args(["schema", "--view", "full"])
        .output()
        .expect("catalog");
    let catalog: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    let text = catalog["instructions"].as_str().expect("instructions");
    assert!(!text.contains("octocode://"), "{text}");
    let bad = workspace
        .cli()
        .args(["schema", "localSearch", "--view", "flow"])
        .output()
        .expect("tool flow");
    assert_eq!(exit_code(&bad), Some(2), "{}", stdout(&bad));
}

#[test]
fn schema_views_and_help_examples_are_usable_without_node() {
    let workspace = Workspace::new();
    workspace.write("src/example.ts", "export function registerHandler() {}\n");
    let query = |args: &[&str]| {
        let output = workspace.cli().args(args).output().expect("schema");
        assert!(output.status.success(), "{args:?}: {}", stdout(&output));
        serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("JSON")
    };
    assert_eq!(
        query(&["schema", "localSearch"]),
        query(&["schema", "localSearch", "--view", "query"])
    );
    let variants = query(&["schema", "astSearch", "--view", "variants"]);
    assert!(
        variants["variants"]
            .as_array()
            .is_some_and(|items| items.len() > 1)
    );
    let selected = query(&["schema", "astSearch", "--select", "operation=symbols"]);
    assert!(
        selected["querySchema"]["properties"]["symbolFilter"].is_object(),
        "{selected}"
    );
    // A named tool's view stays readable: it says what would enable it.
    let clasify = query(&["schema", "clasify"]);
    assert_eq!(clasify["availability"]["enabled"], false, "{clasify}");
    assert_eq!(
        clasify["availability"]["envVar"], "OCTOCODE_CLASSIFICATION_API",
        "{clasify}"
    );
    for args in [
        vec!["schema", "unknownTool"],
        vec!["schema", "localSearch", "--view", "bad"],
        vec!["schema", "astSearch", "--select", "operation=unknown"],
    ] {
        let output = workspace
            .cli()
            .args(&args)
            .output()
            .expect("invalid discovery");
        assert_eq!(exit_code(&output), Some(2), "{args:?}");
    }
    let help = workspace
        .cli()
        .args(["localSearch", "--help"])
        .output()
        .expect("help");
    assert!(help.status.success());
    let example = stdout(&help)
        .split("\n  octocode localSearch '")
        .nth(1)
        .and_then(|example| example.split_once('\''))
        .map(|(input, _)| input)
        .expect("JSON example");
    let input: serde_json::Value = serde_json::from_str(example).expect("example JSON");
    assert!(input["queries"].is_array());
    let output = workspace
        .cli()
        .args(["localSearch", example, "--json"])
        .output()
        .expect("example execution");
    assert!(output.status.success(), "{}", stdout(&output));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("result");
    assert!(
        result["results"][0]["data"]["matchCount"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "{result}"
    );
}

/// `full` and `flow` are catalog views: a tool asked for either is refused
/// with the call that reads it, not just the views a tool has.
#[test]
fn a_catalog_view_asked_of_a_tool_is_refused_with_the_catalog_call() {
    let workspace = Workspace::new();
    for view in ["full", "flow"] {
        let output = workspace
            .cli()
            .args(["schema", "ghSearchCode", "--view", view])
            .output()
            .expect("schema");
        assert_eq!(exit_code(&output), Some(2), "{view}");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("query (default), variants or output")
                && text.contains("octocode schema --view full|flow")
                && text.contains("without a tool"),
            "{view}: {text}"
        );
    }
}
