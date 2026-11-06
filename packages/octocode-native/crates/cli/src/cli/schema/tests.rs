use super::{TOOL_FLOW_COMMAND, catalog, grammar_instruction, tool_view};
use serde_json::{Value, json};

/// A machine catalog with every presentation tool enabled except `off`.
fn machine(off: &[&str]) -> Value {
    let presentation = octocode_native::contracts::cli_presentation().expect("presentation");
    let tools: Vec<Value> = presentation["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| {
                let name = tool["name"].as_str().expect("name");
                json!({ "name": name, "fields": "[]", "availability": { "enabled": !off.contains(&name) } })
            })
            .collect();
    json!({ "fingerprint": "f", "grammarCapabilities": [], "tools": tools })
}

fn message(result: Result<Value, super::SchemaError>) -> String {
    result.expect_err("an error").message
}

#[test]
fn every_catalog_view_names_one_query_command() {
    let machine = machine(&[]);
    let compact = catalog(&machine, None).expect("catalog");
    let full = catalog(&machine, Some("full")).expect("catalog");
    assert_eq!(compact["commands"]["query"], full["commands"]["query"]);
}

#[test]
fn catalog_lists_only_enabled_tools() {
    let value = catalog(&machine(&["clasify"]), None).expect("catalog");
    let names: Vec<&str> = value["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"localSearch"));
    assert!(!names.contains(&"clasify"));
    assert_eq!(value["toolCount"], names.len());
    assert!(
        value.get("grammars").is_none(),
        "no structural grammars, no line"
    );
}

#[test]
fn full_catalog_serves_the_core_instructions_then_the_cli_notes() {
    let presentation = octocode_native::contracts::cli_presentation().expect("presentation");
    let shared = presentation["instructions"].as_str().expect("instructions");
    let block = |name: &str| {
        super::tools(presentation)
            .find(|tool| tool["name"] == name)
            .and_then(|tool| tool["instructions"].as_str())
            .expect("a block")
    };
    let (scout, cves) = (block("clasify"), block("securityCheck"));
    for (off, expected) in [
        (vec![], format!("{shared}{scout}{cves}")),
        (vec!["ghSearchHistory"], format!("{shared}{scout}{cves}")),
        (vec!["securityCheck"], format!("{shared}{scout}")),
        (
            vec!["clasify", "ghSearchHistory"],
            format!("{shared}{cves}"),
        ),
        (vec!["clasify", "securityCheck"], shared.to_owned()),
    ] {
        let value = catalog(&machine(&off), Some("full")).expect("catalog");
        assert_eq!(
            value["instructions"].as_str().expect("instructions"),
            format!(
                "{}\n<cli>Listed tools are available; execution checks credentials and prerequisites. MCP exposure: graph tools need OCTOCODE_BETA, clone needs OCTOCODE_ENABLE_CLONE. Fields: schema <name> --view query. A next.* page still exits 0.</cli>",
                expected
            ),
            "{off:?}"
        );
    }
    // Without the tool, the CLI's own notes do not name it either.
    let value = catalog(&machine(&["clasify", "securityCheck"]), Some("full")).expect("catalog");
    let text = value["instructions"].as_str().expect("instructions");
    for word in ["clasify", "classification", "securityCheck", "CVE"] {
        assert!(!text.contains(word), "{word}: {text}");
    }
}

/// A CLI agent cannot read an MCP resource, so its catalog names none, and it
/// still lists the command that prints the guide.
#[test]
fn the_cli_catalog_names_its_flow_command_and_no_resource() {
    let value = catalog(&machine(&[]), Some("full")).expect("catalog");
    let text = value["instructions"].as_str().expect("instructions");
    assert!(!text.contains("octocode://"), "{text}");
    let listed = catalog(&machine(&[]), None).expect("catalog");
    assert_eq!(listed["commands"]["flow"], TOOL_FLOW_COMMAND, "{listed}");
}

#[test]
fn selection_schema_follows_the_resolved_presentation_switch() {
    let mut machine = machine(&[]);
    for enabled in [false, true] {
        machine["presentation"] = json!({ "selection": enabled });
        for name in ["localSearch", "localFetch"] {
            let row = tool_view(&machine, name, Some("query"), None).expect("schema");
            let properties = &row["querySchema"]["properties"];
            assert_eq!(properties.get("classification").is_some(), enabled);
            assert!(
                !row["querySchema"]["required"]
                    .as_array()
                    .expect("required")
                    .contains(&json!("classification"))
            );
            // Private plan state is never a public row field.
            assert!(properties.get("selection").is_none(), "{row}");
            assert!(properties.get("cursor").is_none(), "{row}");
        }
    }
    let full = tool_view(&machine, "localSearch", Some("full"), None);
    assert!(message(full).starts_with("A tool's views"));
}

#[test]
fn select_reads_one_operation_row() {
    let m = machine(&[]);
    assert!(
        message(tool_view(&m, "lspSearch", Some("query"), Some("x")))
            .starts_with("--select expects operation=VALUE")
    );
    assert!(
        message(tool_view(
            &m,
            "lspSearch",
            Some("query"),
            Some("operation=nope")
        ))
        .starts_with("Unknown operation: nope. Operations: ")
    );
    assert!(tool_view(&m, "lspSearch", Some("full"), Some("operation=callers")).is_err());
    // A JSON-quoted string selects the same row as the bare value.
    let bare = tool_view(&m, "astSearch", Some("query"), Some("operation=symbols")).expect("bare");
    let quoted = tool_view(
        &m,
        "astSearch",
        Some("query"),
        Some("operation=\"symbols\""),
    )
    .expect("quoted");
    assert_eq!(bare, quoted);
    assert!(
        bare["querySchema"]["properties"]["symbolFilter"].is_object(),
        "{bare}"
    );
}
#[test]
fn grammar_line_states_the_common_feature_set_once() {
    let capabilities = json!([
        { "language": "TypeScript", "languageId": "typescript", "selectorAliases": [], "extensions": ["ts"], "structuralSearch": true, "syntaxTree": true, "signatureOutline": true, "facts": {"declarations": true}, "graphFacts": true },
        { "language": "Go", "languageId": "go", "selectorAliases": [], "extensions": ["go"], "structuralSearch": true, "syntaxTree": true, "signatureOutline": true, "facts": {"declarations": true}, "graphFacts": true },
        { "language": "Asm", "languageId": "asm", "selectorAliases": ["s"], "extensions": ["asm", "s"], "structuralSearch": true, "signatureOutline": false, "graphFacts": false },
        { "language": "Plain", "languageId": "plain", "selectorAliases": [], "extensions": [], "structuralSearch": true },
        { "language": "Declarations", "languageId": "decl", "extensions": ["decl"], "facts": {"declarations": true}, "graphFacts": true },
        { "language": "Unavailable", "languageId": "none", "extensions": ["none"], "structuralSearch": false, "syntaxTree": false, "signatureOutline": false, "graphFacts": false },
    ]);
    assert_eq!(
        grammar_instruction(&capabilities).expect("line"),
        "Runtime grammar inventory (structural+syntaxTree+outline+declarations+graph unless noted): TypeScript (typescript: .ts); Go (go: .go); Asm (asm,s: .asm .s; structural); Plain (plain; structural); Declarations (decl: .decl; declarations+graph). Parser availability does not imply an installed language server or complete cross-file linking."
    );
    assert!(grammar_instruction(&json!([])).is_none());
}

#[test]
fn lsp_search_names_its_servers_at_the_end_of_when() {
    assert_eq!(
        super::with_lsp_servers(
            "<when>Resolve symbols.</when><workflow>Read hits.</workflow>",
            &["rust", "ts/js"]
        ),
        "<when>Resolve symbols. Servers found here: rust, ts/js.</when><workflow>Read hits.</workflow>"
    );
    assert_eq!(
        super::with_lsp_servers("<when>Resolve symbols.</when><workflow>x</workflow>", &[]),
        "<when>Resolve symbols. No language server found.</when><workflow>x</workflow>"
    );
}

/// `schema <tool>` states the input contract once: the query fields, never
/// a second copy of them inside the call envelope (`--help` documents the
/// envelope, `{"queries":[…]}` or `{"cursor":"…"}`).
#[test]
fn a_tool_schema_states_its_query_fields_once() {
    let machine = machine(&[]);
    let presentation = octocode_native::contracts::cli_presentation().expect("presentation");
    for tool in presentation["tools"].as_array().expect("tools") {
        let name = tool["name"].as_str().expect("name");
        let value = tool_view(&machine, name, None, None).expect("tool schema");
        assert!(value["querySchema"].is_object(), "{name}");
        assert!(value.get("inputSchema").is_none(), "{name}: {value}");
    }
}

/// Each request the schema views cannot answer is a usage error (exit 2)
/// that names what is accepted, never an empty view.
#[test]
fn views_a_request_cannot_have_are_usage_errors() {
    let m = machine(&["clasify"]);
    for result in [
        catalog(&m, Some("variants")),
        tool_view(&m, "noSuchTool", None, None),
        tool_view(&m, "localSearch", Some("noSuchView"), None),
        tool_view(&m, "localSearch", Some("variants"), Some("operation=x")),
        tool_view(&m, "lspSearch", Some("output"), Some("operation=x")),
    ] {
        assert_eq!(result.expect_err("refused").exit, 2);
    }
    let unknown = message(tool_view(&m, "noSuchTool", None, None));
    assert!(unknown.contains("localSearch"), "{unknown}");
    assert!(
        !unknown.contains("clasify"),
        "a disabled tool is not offered"
    );
}

/// The output view is the meaning of each output field and coverage reason,
/// the same one the MCP resource serves.
#[test]
fn the_output_view_is_the_tools_output_reference() {
    let m = machine(&[]);
    let view = tool_view(&m, "localSearch", Some("output"), None).expect("output view");
    assert_eq!(view["name"], "localSearch");
    assert!(!view["fields"].is_null());
    assert_eq!(super::output_of("localSearch"), Some(view));
    assert_eq!(super::output_of("noSuchTool"), None);
}

/// Without a tool entry in the runtime's catalog a tool is reported disabled.
#[test]
fn a_tool_the_runtime_does_not_list_is_disabled() {
    let m = json!({ "fingerprint": "f", "grammarCapabilities": [], "tools": [] });
    let view = tool_view(&m, "localSearch", Some("variants"), None).expect("variants");
    assert_eq!(view["availability"]["enabled"], false);
}
