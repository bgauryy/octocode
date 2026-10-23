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
        json!({"deferred": [], "selection": false}),
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
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert_eq!(tool["annotations"]["readOnlyHint"], true, "{tool}");
    }
}

#[test]
fn instructions_name_only_available_tools_and_omit_cli_sections() {
    let catalog = catalog(
        &["localSearch"],
        json!({"deferred": [], "selection": false}),
        json!([]),
    );
    let text = build(presentation(), &catalog)
        .expect("listing")
        .instructions;
    assert!(text.contains("<local>"), "{text}");
    assert!(!text.contains("<github>"), "{text}");
    assert!(!text.contains("<cli>"), "{text}");
}

#[test]
fn lsp_search_names_the_resolved_language_servers() {
    let catalog = catalog(
        &["lspSearch"],
        json!({"deferred": [], "selection": false}),
        json!(["rust", "ts/js"]),
    );
    let listing = build(presentation(), &catalog).expect("listing");
    let description = listing.tools[0]["description"].as_str().expect("text");
    assert!(
        description.contains("Servers: rust, ts/js."),
        "{description}"
    );
}

#[test]
fn selection_lists_the_context_selection_variant() {
    let plain = catalog(
        &["localSearch"],
        json!({"deferred": [], "selection": false}),
        json!([]),
    );
    let ranked = catalog(
        &["localSearch"],
        json!({"deferred": [], "selection": true}),
        json!([]),
    );
    let plain = build(presentation(), &plain).expect("plain").tools;
    let ranked = build(presentation(), &ranked).expect("ranked").tools;
    assert_ne!(plain[0]["inputSchema"], ranked[0]["inputSchema"]);
}

#[test]
fn deferred_tools_leave_the_list_and_run_through_the_dispatcher() {
    let catalog = catalog(
        &["localSearch", "localFetch"],
        json!({"deferred": ["localFetch"], "selection": false}),
        json!([]),
    );
    let tools = build(presentation(), &catalog).expect("listing").tools;
    assert_eq!(names(&tools), ["localSearch", "run"]);
    let run = &tools[1];
    assert_eq!(
        run["inputSchema"]["properties"]["tool"]["enum"],
        json!(["localFetch"])
    );
    let description = run["description"].as_str().expect("text");
    assert!(description.contains("\nlocalFetch: "), "{description}");
    assert!(!description.contains("localSearch:"), "{description}");
}

#[test]
fn clasify_lists_the_variant_for_the_exposed_source_readers() {
    let both = catalog(
        &["clasify", "localFetch", "ghGetFileContent"],
        json!({"deferred": [], "selection": false}),
        json!([]),
    );
    let none = catalog(
        &["clasify"],
        json!({"deferred": [], "selection": false}),
        json!([]),
    );
    let both = build(presentation(), &both).expect("both").tools;
    let none = build(presentation(), &none).expect("none").tools;
    assert_ne!(both[0]["inputSchema"], none[0]["inputSchema"]);
}

#[test]
fn a_catalog_without_available_tools_is_an_error() {
    let catalog = catalog(&[], json!({"deferred": [], "selection": false}), json!([]));
    assert!(build(presentation(), &catalog).is_err());
}

#[test]
fn a_catalog_tool_without_a_presentation_is_an_error() {
    let catalog = json!({
        "tools": [{"name": "unknownTool", "available": true}],
        "presentation": {"deferred": [], "selection": false},
        "lspServers": [],
    });
    let Err(message) = build(presentation(), &catalog) else {
        panic!("an unknown tool has no contract");
    };
    assert!(message.contains("unknownTool"), "{message}");
}
