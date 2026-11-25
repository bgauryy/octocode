mod output;

use super::{TOOL_FLOW_COMMAND, catalog, grammar_instruction, tool_view, unknown_tool};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

/// A machine catalog with every presentation tool enabled except `off`.
fn machine(off: &[&str]) -> Value {
    let presentation = octocode_runtime_base::contracts::cli_presentation().expect("presentation");
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
fn full_catalog_exposes_each_tools_batch_limit() {
    use octocode_runtime_base::{contracts::batch_limit, id::ToolId};
    let full = catalog(&machine(&[]), Some("full")).expect("catalog");
    for entry in super::tools(&full) {
        let id = ToolId::from_name(entry["name"].as_str().expect("name")).expect("tool");
        assert_eq!(entry["maxQueries"], json!(batch_limit(id)), "{id:?}");
    }
    assert_eq!(
        full["executable"],
        json!(std::env::current_exe().expect("running CLI"))
    );
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
    let presentation = octocode_runtime_base::contracts::cli_presentation().expect("presentation");
    let all = super::tools(presentation)
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    // Core's composition for the tools this machine enables: a disabled tool's
    // routes and block never show (byte equality with core is the runtime's
    // `compose_instructions` test).
    for off in [
        vec![],
        vec!["ghSearchHistory"],
        vec!["securityCheck"],
        vec!["clasify", "ghSearchHistory"],
        vec!["clasify", "securityCheck"],
    ] {
        let expected =
            octocode_runtime_base::contracts::compose_instructions(presentation, |name| {
                all.contains(&name) && !off.contains(&name)
            });
        let value = catalog(&machine(&off), Some("full")).expect("catalog");
        assert_eq!(
            value["instructions"].as_str().expect("instructions"),
            format!("{expected}\n{}", super::cli_notes(presentation)),
            "{off:?}"
        );
    }
    // The CLI's notes are core's text, carried by the embedded presentation.
    assert!(
        super::cli_notes(presentation).starts_with("<cli>")
            && super::cli_notes(presentation).ends_with("</cli>"),
        "{}",
        super::cli_notes(presentation)
    );
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

/// A second pair selects the row the presentation narrows to it; where the
/// presentation has none, a pair a form of the operation admits selects the
/// operation's row, and any other pair is a usage error that names the fix.
#[test]
fn select_takes_a_second_pair() {
    let m = machine(&[]);
    let view = |selection| tool_view(&m, "astSearch", Some("query"), Some(selection));
    let operation = view("operation=match").expect("operation");
    let presentation = octocode_runtime_base::contracts::cli_presentation().expect("presentation");
    let narrowed = presentation["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "astSearch")
        .expect("astSearch")["selections"]
        .get("operation=match,view=content");
    let pair = view("operation=match,view=content").expect("a second pair");
    match narrowed {
        Some(narrowed) => {
            assert_eq!(pair["querySchema"], narrowed["querySchema"]);
            assert!(
                pair["querySchema"].to_string().len() <= operation["querySchema"].to_string().len(),
                "a narrowed row is no larger than its operation's"
            );
        }
        None => assert_eq!(pair, operation),
    }
    assert_eq!(
        view("operation=\"match\", view=\"content\"").expect("quoted"),
        pair
    );
    assert_eq!(
        message(view("operation=match,view=nope")).split(':').next(),
        Some("operation=match takes view")
    );
    assert_eq!(
        message(view("operation=match,nope=1")),
        "operation=match has no field nope."
    );
    for malformed in [
        "operation=match,",
        "operation=match,view",
        "view=content",
        "operation=match,view=content,limit=1",
    ] {
        assert!(
            message(view(malformed)).starts_with("--select expects operation=VALUE[,FIELD=VALUE]"),
            "{malformed}"
        );
    }
    // A field every form takes freely narrows nothing and selects the row.
    assert_eq!(
        view("operation=match,limit=5").expect("an open field"),
        operation
    );
}

/// The tool `name` of the embedded presentation.
fn presented(name: &str) -> &'static Value {
    octocode_runtime_base::contracts::cli_presentation().expect("presentation")["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == name)
        .unwrap_or_else(|| panic!("{name} is presented"))
}

/// A second pair the contract narrows serves that narrowed row: the pair's
/// field takes only the selected value, and the row is not the operation's.
#[test]
fn a_second_pair_serves_the_narrowed_row_of_the_contract() {
    let m = machine(&[]);
    for (name, operation, field, value) in [
        ("astSearch", "match", "view", "files"),
        ("lspSearch", "references", "view", "files"),
        ("artifactSearch", "resolve", "ecosystem", "pypi"),
    ] {
        let key = format!("operation={operation},{field}={value}");
        let narrowed = presented(name)["selections"]
            .get(&key)
            .unwrap_or_else(|| panic!("{name} publishes {key}"));
        let view = |selection: &str| {
            tool_view(&m, name, Some("query"), Some(selection))
                .unwrap_or_else(|error| panic!("{name} {selection}: {}", error.message))
        };
        let row = view(&key);
        assert_eq!(row["querySchema"], narrowed["querySchema"], "{name} {key}");
        assert_eq!(row["description"], narrowed["description"], "{name} {key}");
        assert_eq!(
            row["querySchema"]["properties"][field]["enum"],
            json!([value]),
            "{name} {key}"
        );
        let whole = view(&format!("operation={operation}"));
        assert_ne!(row["querySchema"], whole["querySchema"], "{name} {key}");
        assert_eq!(row["run"], whole["run"], "{name} {key}");
        assert_eq!(row["availability"], whole["availability"], "{name} {key}");
    }
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
    let capabilities = json_fixture(
        r#"[
            {
                "language": "TypeScript",
                "languageId": "typescript",
                "selectorAliases": [],
                "extensions": ["ts"],
                "structuralSearch": true,
                "syntaxTree": true,
                "signatureOutline": true,
                "facts": {"declarations": true},
                "graphFacts": true
            },
            {
                "language": "Go",
                "languageId": "go",
                "selectorAliases": [],
                "extensions": ["go"],
                "structuralSearch": true,
                "syntaxTree": true,
                "signatureOutline": true,
                "facts": {"declarations": true},
                "graphFacts": true
            },
            {
                "language": "Asm",
                "languageId": "asm",
                "selectorAliases": ["s"],
                "extensions": ["asm", "s"],
                "structuralSearch": true,
                "signatureOutline": false,
                "graphFacts": false
            },
            {
                "language": "Plain",
                "languageId": "plain",
                "selectorAliases": [],
                "extensions": [],
                "structuralSearch": true
            },
            {
                "language": "Declarations",
                "languageId": "decl",
                "extensions": ["decl"],
                "facts": {"declarations": true},
                "graphFacts": true
            },
            {
                "language": "Unavailable",
                "languageId": "none",
                "extensions": ["none"],
                "structuralSearch": false,
                "syntaxTree": false,
                "signatureOutline": false,
                "graphFacts": false
            }
        ]"#,
    );
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
    let presentation = octocode_runtime_base::contracts::cli_presentation().expect("presentation");
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

/// A tool this package does not serve but another does (graphBuild is the
/// graph package's) is refused with the package that serves it and the command
/// that launches it; a name no package serves gets no such pointer.
#[test]
fn a_tool_another_package_serves_names_that_package_and_its_launch_command() {
    use crate::cli::tool_package::ToolPackage;
    let m = machine(&[]);
    for tool in octocode_runtime_base::id::ToolId::ALL {
        let name = tool.as_str();
        let owner = ToolPackage::of(tool);
        let other = if owner == ToolPackage::Graph {
            ToolPackage::Research
        } else {
            ToolPackage::Graph
        };
        let note = other.elsewhere_note(name);
        let note = note.unwrap_or_else(|| panic!("{name} is served by {}", owner.cli()));
        assert!(note.contains(owner.cli()), "{note}");
        assert!(note.contains(owner.launch()), "{note}");
        assert!(note.contains(name), "{note}");
        assert!(owner.elsewhere_note(name).is_none(), "{name}");
    }
    assert!(ToolPackage::Research.elsewhere_note("noSuchTool").is_none());
    let unknown = unknown_tool(&m, ToolPackage::Research, "graphBuild").message;
    assert!(unknown.contains("Unknown tool: graphBuild"), "{unknown}");
    assert!(
        unknown.contains(ToolPackage::Graph.launch()),
        "the refusal gives the launch command: {unknown}"
    );
}

/// Without a tool entry in the runtime's catalog a tool is reported disabled.
#[test]
fn a_tool_the_runtime_does_not_list_is_disabled() {
    let m = json_fixture(r#"{"fingerprint": "f", "grammarCapabilities": [], "tools": []}"#);
    let view = tool_view(&m, "localSearch", Some("variants"), None).expect("variants");
    assert_eq!(view["availability"]["enabled"], false);
}

/// The CLI states the instructions once, in the full catalog: no catalog row
/// and no per-tool view (`schema <tool>`, `--view query`, `--view variants`)
/// repeats one of their statements.
#[test]
fn cli_views_state_the_instructions_once() {
    use crate::cli::test_support::{instruction_sentences, strings_of};
    let machine = machine(&[]);
    let full = catalog(&machine, Some("full")).expect("catalog");
    let instructions = full["instructions"].as_str().expect("instructions");
    let sentences = instruction_sentences(instructions);
    assert!(sentences.len() > 20, "{sentences:?}");
    let mut rest = full.clone();
    rest.as_object_mut()
        .expect("catalog")
        .remove("instructions");
    let mut exposed = vec![("catalog".to_owned(), strings_of(&rest))];
    for tool in full["tools"].as_array().expect("tools") {
        let name = tool["name"].as_str().expect("name");
        for view in [None, Some("query"), Some("variants")] {
            let value = tool_view(&machine, name, view, None).expect("tool view");
            exposed.push((format!("{name} {view:?}"), strings_of(&value)));
        }
    }
    for sentence in &sentences {
        assert_eq!(
            instructions.matches(sentence).count(),
            1,
            "stated twice: {sentence}"
        );
        for (place, text) in &exposed {
            assert!(!text.contains(sentence), "{place} repeats: {sentence}");
        }
    }
}

/// Memory storage refuses materialize, so the CLI view of `ghStructure`
/// names it nowhere; persistent storage keeps it.
#[test]
fn memory_storage_strips_materialize_from_the_ghstructure_view() {
    let mut m = machine(&[]);
    for view in ["query", "variants"] {
        let text = tool_view(&m, "ghStructure", Some(view), None)
            .unwrap()
            .to_string();
        assert!(text.contains("materialize"), "{view}");
    }
    m["persistentStorage"] = json!(false);
    for view in ["query", "variants"] {
        let text = tool_view(&m, "ghStructure", Some(view), None)
            .unwrap()
            .to_string();
        assert!(!text.contains("materialize"), "{view}: {text}");
    }
    let other = tool_view(&m, "ghGetFileContent", Some("query"), None).unwrap();
    assert!(other.to_string().len() > 100);
}
