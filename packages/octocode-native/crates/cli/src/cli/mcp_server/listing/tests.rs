use super::build;
use serde_json::{Value, json};

fn presentation() -> &'static Value {
    octocode_native::contracts::cli_presentation().expect("embedded presentation")
}

/// A catalog with the named tools available.
fn catalog(available: &[&str], presentation: Value, lsp_servers: Value) -> Value {
    let tools = presentation_tools()
        .into_iter()
        .map(|name| json!({"name": name, "available": available.contains(&name.as_str())}))
        .collect::<Vec<_>>();
    json!({"tools": tools, "presentation": presentation, "lspServers": lsp_servers})
}

fn presentation_tools() -> Vec<String> {
    presentation()["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect()
}

fn names(tools: &[Value]) -> Vec<&str> {
    tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect()
}

#[test]
fn lists_available_tools_in_catalog_order_with_published_inputs() {
    let catalog = catalog(
        &["localSearch", "localFetch"],
        json!({"selection": false}),
        json!([]),
    );
    let listing = build(presentation(), &catalog).expect("listing");
    let expected = presentation_tools()
        .into_iter()
        .filter(|name| ["localSearch", "localFetch"].contains(&name.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(names(&listing.tools), expected);
    for tool in &listing.tools {
        assert!(tool["title"].is_string(), "{tool}");
        assert!(tool["description"].is_string(), "{tool}");
        assert!(tool["inputSchema"].is_object(), "{tool}");
        assert_eq!(tool["annotations"]["readOnlyHint"], true, "{tool}");
    }
}

#[test]
fn instructions_are_the_one_core_string_without_cli_notes() {
    let catalog = catalog(&["localSearch"], json!({"selection": false}), json!([]));
    let text = build(presentation(), &catalog)
        .expect("listing")
        .instructions;
    assert_eq!(text, presentation()["instructions"].as_str().expect("text"));
    assert!(
        text.contains("Local:") && text.contains("GitHub:"),
        "{text}"
    );
    assert!(!text.contains("<cli>"), "{text}");
}

#[test]
fn lsp_search_names_the_resolved_language_servers() {
    let catalog = catalog(
        &["lspSearch"],
        json!({"selection": false}),
        json!(["rust", "ts/js"]),
    );
    let listing = build(presentation(), &catalog).expect("listing");
    let description = listing.tools[0]["description"].as_str().expect("text");
    assert!(
        description.contains(" Servers: rust, ts/js.</when><workflow>"),
        "{description}"
    );
}

#[test]
fn selection_lists_the_context_selection_variant() {
    let plain = catalog(&["localSearch"], json!({"selection": false}), json!([]));
    let ranked = catalog(&["localSearch"], json!({"selection": true}), json!([]));
    let plain = build(presentation(), &plain).expect("plain").tools;
    let ranked = build(presentation(), &ranked).expect("ranked").tools;
    assert_ne!(plain[0]["inputSchema"], ranked[0]["inputSchema"]);
}

#[test]
fn every_enabled_tool_is_listed_with_its_exact_schema() {
    let available = presentation_tools();
    let names = available.iter().map(String::as_str).collect::<Vec<_>>();
    let catalog = catalog(&names, json!({"selection":false}), json!([]));
    let listing = build(presentation(), &catalog).unwrap();
    assert_eq!(listing.tools.len(), available.len());
    for listed in listing.tools {
        let source = presentation()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == listed["name"])
            .unwrap();
        assert_eq!(listed["inputSchema"], source["mcp"]["inputSchema"]);
    }
}

#[test]
fn a_catalog_without_available_tools_is_an_error() {
    let catalog = catalog(&[], json!({"selection": false}), json!([]));
    assert!(build(presentation(), &catalog).is_err());
}

#[test]
fn a_catalog_tool_without_a_presentation_is_an_error() {
    let catalog = json!({
        "tools": [{"name": "unknownTool", "available": true}],
        "presentation": {"selection": false},
        "lspServers": [],
    });
    let Err(message) = build(presentation(), &catalog) else {
        panic!("an unknown tool has no contract");
    };
    assert!(message.contains("unknownTool"), "{message}");
}
