use super::build;
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

fn presentation() -> &'static Value {
    octocode_runtime_base::contracts::cli_presentation().expect("embedded presentation")
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
        json_fixture(r#"{"selection": false}"#),
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

/// The served instructions are core's composition for exactly the listed
/// tools (byte equality with core is the runtime's `compose_instructions`
/// test), with no CLI notes.
#[test]
fn instructions_are_core_s_composition_for_the_listed_tools() {
    let optional: Vec<String> = presentation()["instructions"]["blocks"]
        .as_array()
        .expect("blocks")
        .iter()
        .filter_map(|block| block["tool"].as_str().map(str::to_owned))
        .collect();
    assert!(optional.iter().any(|name| name == "clasify"));
    let plain: Vec<String> = presentation_tools()
        .into_iter()
        .filter(|name| !optional.contains(name))
        .collect();
    let mut sets: Vec<Vec<String>> = vec![
        vec!["localSearch".into()],
        plain.clone(),
        presentation_tools(),
    ];
    sets.extend(optional.iter().map(|name| {
        let mut set = plain.clone();
        set.push(name.clone());
        set
    }));
    for set in sets {
        let available: Vec<&str> = set.iter().map(String::as_str).collect();
        let catalog = catalog(
            &available,
            json_fixture(r#"{"selection": true}"#),
            json!([]),
        );
        let text = build(presentation(), &catalog)
            .expect("listing")
            .instructions;
        assert_eq!(
            text,
            octocode_runtime_base::contracts::compose_instructions(presentation(), |name| {
                available.contains(&name)
            }),
            "{available:?}"
        );
        assert!(!text.contains("<cli>"), "{text}");
        // A tool that is not listed is not named, by its block or by a route.
        for name in presentation_tools() {
            if !available.contains(&name.as_str()) && name != "clasify" {
                assert!(!text.contains(&name), "{name}: {text}");
            }
        }
    }
}

#[test]
fn lsp_search_names_the_resolved_language_servers() {
    let catalog = catalog(
        &["lspSearch"],
        json_fixture(r#"{"selection": false}"#),
        json_fixture(r#"["rust", "ts/js"]"#),
    );
    let listing = build(presentation(), &catalog).expect("listing");
    let description = listing.tools[0]["description"].as_str().expect("text");
    assert!(
        description.contains(" Servers found here: rust, ts/js.</when>"),
        "{description}"
    );
}

#[test]
fn every_enabled_tool_is_listed_with_its_exact_schema() {
    let available = presentation_tools();
    let names = available.iter().map(String::as_str).collect::<Vec<_>>();
    let catalog = catalog(&names, json_fixture(r#"{"selection": false}"#), json!([]));
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
    let catalog = catalog(&[], json_fixture(r#"{"selection": false}"#), json!([]));
    assert!(build(presentation(), &catalog).is_err());
}

#[test]
fn a_catalog_tool_without_a_presentation_is_an_error() {
    let catalog = json_fixture(
        r#"{
            "tools": [{"name": "unknownTool", "available": true}],
            "presentation": {"selection": false},
            "lspServers": []
        }"#,
    );
    let Err(message) = build(presentation(), &catalog) else {
        panic!("an unknown tool has no contract");
    };
    assert!(message.contains("unknownTool"), "{message}");
}

/// What `tools/list` sends is what core published: each entry, after the
/// rmcp `Tool` the handler serves it as, is the presentation's `mcp`
/// definition, and its `inputSchema` is byte for byte the embedded one.
#[test]
fn tools_list_serves_the_core_published_definitions_byte_for_byte() {
    let names = presentation_tools();
    let names = names.iter().map(String::as_str).collect::<Vec<_>>();
    for selection in [false, true] {
        let catalog = catalog(&names, json!({"selection": selection}), json!([]));
        let listing = build(presentation(), &catalog).expect("listing");
        for listed in listing.tools {
            let name = listed["name"].as_str().expect("name").to_owned();
            let source = presentation()["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .find(|tool| tool["name"] == name.as_str())
                .expect("presented");
            let mcp = &source
                .get("selection")
                .filter(|_| selection)
                .unwrap_or(source)["mcp"];
            let served: rmcp::model::Tool =
                serde_json::from_value(listed).expect("an rmcp tool definition");
            let wire = serde_json::to_value(&served).expect("serialized tool");
            let mut expected = mcp.clone();
            expected["name"] = json!(name);
            // The one runtime addition: the language servers this host has.
            if name == "lspSearch" {
                let text = mcp["description"].as_str().expect("description");
                expected["description"] =
                    json!(super::super::super::schema::with_lsp_servers(text, &[]));
            }
            assert_eq!(wire, expected, "{name} selection={selection}");
            assert_eq!(
                serde_json::to_string(&wire["inputSchema"]).expect("schema text"),
                serde_json::to_string(&mcp["inputSchema"]).expect("schema text"),
                "{name} selection={selection}"
            );
        }
    }
}

/// Instructions are stated once per connection: `initialize` carries them and
/// no `tools/list` entry repeats one of their statements, in either variant.
/// A host that copies server instructions into each tool's text (Codex code
/// mode does, per `ALL_TOOLS` entry) repeats them itself; this server must not.
#[test]
fn no_listed_tool_repeats_the_instructions() {
    use crate::cli::test_support::{instruction_sentences, strings_of};
    let names = presentation_tools();
    let names = names.iter().map(String::as_str).collect::<Vec<_>>();
    for selection in [false, true] {
        let catalog = catalog(&names, json!({"selection": selection}), json!([]));
        let listing = build(presentation(), &catalog).expect("listing");
        let sentences = instruction_sentences(&listing.instructions);
        assert!(sentences.len() > 20, "{sentences:?}");
        for sentence in &sentences {
            assert_eq!(
                listing.instructions.matches(sentence).count(),
                1,
                "stated twice: {sentence}"
            );
        }
        for tool in &listing.tools {
            let text = strings_of(tool);
            for sentence in &sentences {
                assert!(
                    !text.contains(sentence),
                    "{} selection={selection} repeats: {sentence}",
                    tool["name"]
                );
            }
        }
    }
}
