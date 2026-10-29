use super::{catalog, grammar_instruction, tool_view};
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
    for off in [vec![], vec!["clasify", "ghSearchHistory"]] {
        let value = catalog(&machine(&off), Some("full")).expect("catalog");
        let text = value["instructions"].as_str().expect("instructions");
        assert!(text.starts_with(shared), "{off:?}: {text}");
        assert!(
            text.ends_with(
                "<cli>CLI only: ghCloneRepo. Graph tools also use MCP when OCTOCODE_BETA=true. Fields: schema <name> --view query. A next.* page still exits 0.</cli>"
            ),
            "{text}"
        );
    }
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
