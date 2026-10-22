//! `schema` and the input section of `<tool> --help`, served from the
//! embedded CLI presentation (`octocode-config` emits it from the same core
//! Zod contract MCP uses). Only runtime availability is joined here: the
//! catalog lists enabled tools, and per-tool views carry `availability` and
//! `run`.
use serde_json::{Value, json};

/// A usage error (exit 2) or an execution error (exit 5).
#[derive(Debug)]
pub(super) struct SchemaError {
    pub message: String,
    pub exit: u8,
}

fn usage(message: impl Into<String>) -> SchemaError {
    SchemaError {
        message: message.into(),
        exit: 2,
    }
}

fn presentation() -> Result<&'static Value, SchemaError> {
    octocode_native::contracts::cli_presentation().map_err(|error| SchemaError {
        message: format!("Embedded CLI presentation is invalid: {error}"),
        exit: 5,
    })
}

fn tools(presentation: &Value) -> impl Iterator<Item = &Value> {
    presentation["tools"].as_array().into_iter().flatten()
}

fn machine_entry<'a>(machine: &'a Value, name: &str) -> Option<&'a Value> {
    machine["tools"]
        .as_array()?
        .iter()
        .find(|tool| tool["name"] == name)
}

fn enabled(machine: &Value, name: &str) -> bool {
    machine_entry(machine, name).is_some_and(|tool| tool["availability"]["enabled"] == true)
}

/// A clasify example's source names a reader this instance runs (a URL
/// capture reads back through localFetch).
fn example_uses_available_tools(example: &Value, allowed: &[&str]) -> bool {
    if let Some(source) = example.get("source") {
        let reader = source["tool"].as_str().unwrap_or("localFetch");
        if !allowed.contains(&reader) {
            return false;
        }
    }
    example["queries"].as_array().is_none_or(|queries| {
        queries
            .iter()
            .all(|query| example_uses_available_tools(query, allowed))
    })
}

fn remove_unavailable_examples(view: &mut Value, allowed: &[&str]) {
    match view {
        Value::Object(object) => {
            if object
                .get("example")
                .is_some_and(|example| !example_uses_available_tools(example, allowed))
            {
                object.remove("example");
            }
            if let Some(examples) = object.get_mut("examples").and_then(Value::as_array_mut) {
                examples.retain(|example| example_uses_available_tools(example, allowed));
            }
            for child in object.values_mut() {
                remove_unavailable_examples(child, allowed);
            }
        }
        Value::Array(values) => {
            for child in values {
                remove_unavailable_examples(child, allowed);
            }
        }
        _ => {}
    }
}

/// Select a core-generated presentation using the runtime's resolved switch.
pub(super) fn configured_tool<'a>(tool: &'a Value, machine: &Value) -> &'a Value {
    if machine["presentation"]["selection"] == true {
        tool.get("selection").unwrap_or(tool)
    } else {
        tool
    }
}

/// Enabled tools and discovery commands; the full view adds instructions,
/// the grammar inventory, setup routes and field summaries.
pub(super) fn catalog(machine: &Value, view: Option<&str>) -> Result<Value, SchemaError> {
    if view.is_some_and(|view| view != "full") {
        return Err(usage("Without a tool, only --view full is supported."));
    }
    let presentation = presentation()?;
    let instruction_view = match (
        machine["presentation"]["selection"] == true,
        enabled(machine, "clasify"),
    ) {
        (false, false) => "ordinary",
        (true, false) => "selection",
        (false, true) => "clasify",
        (true, true) => "both",
    };
    let listed: Vec<Value> = tools(presentation)
        .filter_map(|tool| {
            let name = tool["name"].as_str()?;
            let entry = machine_entry(machine, name).filter(|_| enabled(machine, name))?;
            Some(json!({
                "name": name,
                "description": tool["shortDescription"],
                "fields": entry.get("fields").cloned().unwrap_or_else(|| json!("[]")),
            }))
        })
        .collect();
    let mut value = json!({
        "kind": "octocode.toolCatalog",
        "version": 1,
        "toolCount": listed.len(),
        "commands": {
            "run": "octocode <tool> '<json>' (or --input FILE|-)",
            "query": "octocode schema <tool> --view query [--select FIELD=VALUE]",
            "variants": "octocode schema <tool> --view variants",
            "full": "octocode schema <tool> --view full",
            // First-run setup an agent cannot discover from the tool list.
            "setup": {
                "auth": "octocode auth status (login: octocode auth login)",
                "mcp": "octocode install --ide <id> (ids: octocode install --list)",
                "skills": "octocode skill install --all --platform <claude|codex|cursor|...> --global",
                "graph": "octocode graph --help",
            },
        },
        "instructions": presentation["instructions"][instruction_view],
    });
    if let Some(grammars) = grammar_instruction(&machine["grammarCapabilities"]) {
        value["grammars"] = Value::String(grammars);
    }
    value["tools"] = Value::Array(listed);
    if view.is_none() {
        if let Some(tools) = value["tools"].as_array_mut() {
            for tool in tools {
                if let Some(row) = tool.as_object_mut() {
                    row.remove("fields");
                }
            }
        }
        if let Some(catalog) = value.as_object_mut() {
            catalog.remove("grammars");
            catalog.remove("instructions");
        }
        value["commands"] = json!({
            "run": "octocode <tool> '<json>' (or --input FILE|-)",
            "query": "octocode schema <tool> [--select operation=VALUE]",
            "full": "octocode schema --view full"
        });
    }
    Ok(value)
}

/// `octocode schema <tool> [--view] [--select]`.
pub(super) fn tool_view(
    machine: &Value,
    name: &str,
    view: Option<&str>,
    select: Option<&str>,
) -> Result<Value, SchemaError> {
    let presentation = presentation()?;
    let Some(tool) = tools(presentation).find(|tool| tool["name"] == name) else {
        let known = tools(presentation)
            .filter_map(|tool| tool["name"].as_str())
            .filter(|tool| enabled(machine, tool))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(usage(format!("Unknown tool: {name}. Known tools: {known}")));
    };
    let configured = configured_tool(tool, machine);
    let view = view.unwrap_or("query");
    let mut value = match select {
        None => configured["views"][view].clone(),
        Some(_) if view != "query" => {
            return Err(usage("--select requires the query view and a tool name"));
        }
        Some(selection) => selected(tool, selection)?.clone(),
    };
    if name == "clasify" && select.is_none() {
        // Core renders clasify once per exposed set of source readers.
        let available = presentation["clasifySourceTools"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|tool| enabled(machine, tool))
            .collect::<Vec<_>>();
        value = configured["sourceViews"][available.join(",")]["views"][view].clone();
        remove_unavailable_examples(&mut value, &available);
    }
    value["availability"] = machine_entry(machine, name)
        .map(|entry| entry["availability"].clone())
        .unwrap_or_else(|| json!({ "enabled": false }));
    value["run"] = Value::String(format!("octocode {name} '<json>'"));
    Ok(value)
}

/// The `--select operation=X` view: that operation's row. A JSON-quoted
/// value selects the same row as the bare one.
fn selected<'a>(tool: &'a Value, selection: &str) -> Result<&'a Value, SchemaError> {
    let (field, raw) = selection.split_once('=').unwrap_or(("", ""));
    if field.trim() != "operation" || raw.trim().is_empty() {
        return Err(usage(
            "--select expects operation=VALUE, e.g. operation=commit",
        ));
    }
    let operation = serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_owned());
    tool["selections"]
        .get(format!("operation={operation}"))
        .ok_or_else(|| {
            let known = tool["variantNames"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            usage(if known.is_empty() {
                "This tool has no operations to select.".to_owned()
            } else {
                format!("Unknown operation: {operation}. Operations: {known}")
            })
        })
}

/// The input section printed after clap's help for `<tool> --help`.
pub(super) fn tool_help(name: &str) -> Option<&'static str> {
    let presentation = octocode_native::contracts::cli_presentation().ok()?;
    tools(presentation)
        .find(|tool| tool["name"] == name)?
        .get("help")?
        .as_str()
}

/// The runtime grammar inventory line, stating the common feature set once.
fn grammar_instruction(capabilities: &Value) -> Option<String> {
    let structural: Vec<&Value> = capabilities
        .as_array()?
        .iter()
        .filter(|capability| {
            capability["structuralSearch"] == true
                && capability["extensions"]
                    .as_array()
                    .is_some_and(|extensions| !extensions.is_empty())
        })
        .collect();
    if structural.is_empty() {
        return None;
    }
    let features_of = |capability: &Value| {
        let mut features = vec!["structural"];
        if capability["signatureOutline"] == true {
            features.push("outline");
        }
        if capability["graphFacts"] == true {
            features.push("graph");
        }
        features.join("+")
    };
    // Most common feature set; the first one seen wins a tie.
    let mut counts: Vec<(String, usize)> = Vec::new();
    for capability in &structural {
        let features = features_of(capability);
        match counts.iter_mut().find(|(seen, _)| *seen == features) {
            Some((_, count)) => *count += 1,
            None => counts.push((features, 1)),
        }
    }
    let mut common = &counts[0];
    for entry in &counts {
        if entry.1 > common.1 {
            common = entry;
        }
    }
    let common = common.0.clone();
    let strings = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let inventory = structural
        .iter()
        .map(|capability| {
            let mut ids: Vec<String> = capability["languageId"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
                .into_iter()
                .collect();
            ids.extend(strings(&capability["selectorAliases"]));
            let ids = ids.join(",");
            let extensions = strings(&capability["extensions"])
                .iter()
                .map(|extension| format!(".{extension}"))
                .collect::<Vec<_>>()
                .join(" ");
            let features = features_of(capability);
            let tag = if features == common {
                String::new()
            } else {
                format!("; {features}")
            };
            let ids = if ids.is_empty() {
                String::new()
            } else {
                format!("{ids}: ")
            };
            format!(
                "{} ({ids}{extensions}{tag})",
                capability["language"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "Runtime grammar inventory ({common} unless noted): {inventory}. Parser availability does not imply an installed language server or complete cross-file linking."
    ))
}

#[cfg(test)]
mod tests {
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
        let bare =
            tool_view(&m, "astSearch", Some("query"), Some("operation=symbols")).expect("bare");
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
}
