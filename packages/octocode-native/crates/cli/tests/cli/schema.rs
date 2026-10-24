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
    assert_eq!(
        query(&["schema", "clasify"])["availability"]["enabled"],
        false
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
