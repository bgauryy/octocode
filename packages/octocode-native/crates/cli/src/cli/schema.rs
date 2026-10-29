//! `schema` and the input section of `<tool> --help`, served from the
//! embedded CLI presentation (`octocode-config` emits it from the same core
//! Zod contract MCP uses). Only runtime availability is joined here: the
//! catalog lists enabled tools, and per-tool views carry `availability` and
//! `run`.
mod grammar;
use grammar::grammar_instruction;
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

/// The CLI's own notes, after the shared instructions in the full catalog.
const CLI_INSTRUCTIONS: &str = "<cli>CLI only: ghCloneRepo. Graph tools also use MCP when OCTOCODE_BETA=true. Fields: schema <name> --view query. A next.* page still exits 0.</cli>";

/// Core's one instruction string (embedded through config), served by MCP
/// `initialize` and, with the CLI's own notes, by the full catalog.
pub(super) fn instructions(presentation: &Value) -> &str {
    presentation["instructions"].as_str().unwrap_or_default()
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
    let listed: Vec<Value> = tools(presentation)
        .filter_map(|tool| {
            let name = tool["name"].as_str()?;
            let entry = machine_entry(machine, name).filter(|_| enabled(machine, name))?;
            Some(json!({
                "name": name,
                "title": tool["title"],
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
            // First-run setup an agent cannot discover from the tool list.
            "setup": {
                "auth": "octocode auth status (login: octocode auth login)",
                "mcp": "octocode install --ide <id> (ids: octocode install --list)",
                "skills": "octocode skill install --all --platform <claude|codex|cursor|...> --global",
                "graph": "octocode graph --help",
            },
        },
        "instructions": format!("{}\n{CLI_INSTRUCTIONS}", instructions(presentation)),
    });
    if let Some(grammars) = grammar_instruction(&machine["grammarCapabilities"]) {
        value["grammars"] = Value::String(grammars);
    }
    if view.is_some() {
        value["grammarCapabilities"] = machine["grammarCapabilities"].clone();
        value["sourceDecoding"] = machine["sourceDecoding"].clone();
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
    if configured["views"].get(view).is_none() {
        return Err(usage("A tool's views: query (default) or variants."));
    }
    let mut value = match select {
        None => configured["views"][view].clone(),
        Some(_) if view != "query" => {
            return Err(usage("--select requires the query view and a tool name"));
        }
        Some(selection) => selected(tool, selection)?.clone(),
    };
    if name == "lspSearch"
        && let (Some(description), Some(servers)) = (
            value["description"].as_str(),
            machine["lspServers"].as_array(),
        )
    {
        let servers: Vec<&str> = servers.iter().filter_map(Value::as_str).collect();
        value["description"] = Value::String(with_lsp_servers(description, &servers));
    }
    value["availability"] = machine_entry(machine, name)
        .map(|entry| entry["availability"].clone())
        .unwrap_or_else(|| json!({ "enabled": false }));
    value["run"] = Value::String(format!("octocode {name} '<json>'"));
    Ok(value)
}

/// The lspSearch description with the language servers this runtime
/// resolves, as the last sentence of its `<when>` text:
/// `Servers found here: rust, ts/js.` (what this host has, not what Octocode supports).
pub(super) fn with_lsp_servers(description: &str, servers: &[&str]) -> String {
    let note = if servers.is_empty() {
        "No language server found.".to_owned()
    } else {
        format!("Servers found here: {}.", servers.join(", "))
    };
    description.replacen("</when>", &format!(" {note}</when>"), 1)
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

#[cfg(test)]
mod tests;
