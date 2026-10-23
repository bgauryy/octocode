use super::{catalog, grammar_instruction, remove_unavailable_examples, tool_view};
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
fn instructions_name_only_the_tools_this_instance_runs() {
    let all = catalog(&machine(&[]), Some("full")).expect("catalog");
    let all = all["instructions"].as_str().expect("instructions");
    assert!(all.contains("<local>") && all.contains("<cli>CLI only: ghCloneRepo."));
    let presentation = octocode_native::contracts::cli_presentation().expect("presentation");
    let others = presentation["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .filter(|name| *name != "ghSearchHistory")
        .collect::<Vec<_>>();
    let narrowed = catalog(&machine(&others), Some("full")).expect("catalog");
    let narrowed = narrowed["instructions"].as_str().expect("instructions");
    assert!(narrowed.contains("<github>Discover: ghSearchHistory/history.</github>"));
    for absent in [
        "<local>",
        "<packages>",
        "<read>",
        "ghCloneRepo",
        "<clasify>",
    ] {
        assert!(!narrowed.contains(absent), "{absent} in {narrowed}");
    }
    assert!(
        narrowed.ends_with(
            "<cli>Fields: schema <name> --view query. A next.* page still exits 0.</cli>"
        )
    );
}

#[test]
fn classification_instructions_follow_available_routes() {
    for selection in [false, true] {
        for clasify in [false, true] {
            let mut machine = machine(if clasify { &[] } else { &["clasify"] });
            machine["presentation"] = json!({ "selection": selection });
            let value = catalog(&machine, Some("full")).expect("catalog");
            let instructions = value["instructions"].as_str().expect("instructions");
            assert_eq!(instructions.contains("<clasify>"), selection || clasify);
            assert_eq!(instructions.contains("clasifyContext"), selection);
            assert_eq!(instructions.contains("clasify scouts unread"), clasify);
        }
    }
}

#[test]
fn selection_schema_follows_the_resolved_presentation_switch() {
    let mut machine = machine(&[]);
    for enabled in [false, true] {
        machine["presentation"] = json!({ "selection": enabled });
        for name in ["localSearch", "localFetch"] {
            for view in ["query", "full"] {
                let row = tool_view(&machine, name, Some(view), None).expect("schema");
                assert_eq!(
                    row["querySchema"]["properties"]
                        .get("clasifyContext")
                        .is_some(),
                    enabled
                );
                assert!(
                    !row["querySchema"]["required"]
                        .as_array()
                        .expect("required")
                        .contains(&json!("clasifyContext"))
                );
                if view == "query" {
                    assert_eq!(
                        row["continuationFields"].get("selection").is_some(),
                        enabled
                    );
                } else {
                    assert_eq!(
                        row["querySchema"]["properties"].get("selection").is_some(),
                        enabled
                    );
                }
            }
        }
    }
}

#[test]
fn clasify_sources_follow_enabled_readers() {
    for (off, readers) in [
        (vec![], vec!["localFetch", "ghGetFileContent"]),
        (vec!["ghGetFileContent"], vec!["localFetch"]),
        (vec!["localFetch", "ghGetFileContent"], vec![]),
    ] {
        let row = tool_view(&machine(&off), "clasify", Some("query"), None).expect("schema");
        let source = row["querySchema"]["properties"].get("source");
        assert_eq!(source.is_some(), !readers.is_empty(), "{off:?}");
        let text = source.map(Value::to_string).unwrap_or_default();
        for reader in ["localFetch", "ghGetFileContent"] {
            assert_eq!(
                text.contains(reader),
                readers.contains(&reader),
                "{off:?} {reader}"
            );
        }
    }
}

#[test]
fn clasify_variant_examples_never_suggest_disabled_readers() {
    let mut view = json!({
        "variants": [
            { "name": "source", "example": { "source": { "tool": "localFetch" } } },
            { "name": "url", "example": { "source": { "url": "https://a.test" } } },
            { "name": "state", "example": { "state": "known" } }
        ]
    });
    remove_unavailable_examples(&mut view, &["ghGetFileContent"]);
    assert!(view["variants"][0].get("example").is_none());
    assert!(view["variants"][1].get("example").is_none());
    assert_eq!(view["variants"][2]["example"]["state"], "known");
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
    assert_eq!(
        message(tool_view(
            &m,
            "lspSearch",
            Some("full"),
            Some("operation=callers")
        )),
        "--select requires the query view and a tool name"
    );
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
        bare["querySchema"]["properties"]["symbolName"].is_object(),
        "{bare}"
    );
}
#[test]
fn grammar_line_states_the_common_feature_set_once() {
    let capabilities = json!([
        { "language": "TypeScript", "languageId": "typescript", "selectorAliases": [], "extensions": ["ts"], "structuralSearch": true, "signatureOutline": true, "graphFacts": true },
        { "language": "Go", "languageId": "go", "selectorAliases": [], "extensions": ["go"], "structuralSearch": true, "signatureOutline": true, "graphFacts": true },
        { "language": "Asm", "languageId": "asm", "selectorAliases": ["s"], "extensions": ["asm", "s"], "structuralSearch": true, "signatureOutline": false, "graphFacts": false },
        { "language": "Plain", "languageId": "plain", "selectorAliases": [], "extensions": [], "structuralSearch": true },
    ]);
    assert_eq!(
        grammar_instruction(&capabilities).expect("line"),
        "Runtime grammar inventory (structural+outline+graph unless noted): TypeScript (typescript: .ts); Go (go: .go); Asm (asm,s: .asm .s; structural). Parser availability does not imply an installed language server or complete cross-file linking."
    );
    assert!(grammar_instruction(&json!([])).is_none());
}

#[test]
fn lsp_search_names_its_servers_before_the_notes() {
    assert_eq!(
        super::with_lsp_servers(
            "Resolve symbols. If empty: retry. Next: x.",
            &["rust", "ts/js"]
        ),
        "Resolve symbols. Servers: rust, ts/js. If empty: retry. Next: x."
    );
    assert_eq!(
        super::with_lsp_servers("Resolve symbols.", &[]),
        "Resolve symbols. No language server found."
    );
}
