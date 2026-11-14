//! `schema` and the input section of `<tool> --help`, served from the
//! embedded CLI presentation (`octocode-config` emits it from the same core
//! Zod contract MCP uses). Only runtime availability is joined here: the
//! catalog lists enabled tools, and per-tool views carry `availability` and
//! `run`.
mod grammar;
mod output;
use grammar::grammar_instruction;
pub(super) use output::output_of;
use output::output_view;
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

/// The `tools[]` entry named `name` of a presentation or a machine catalog.
pub(in crate::cli) fn tool_entry<'a>(document: &'a Value, name: &str) -> Option<&'a Value> {
    tools(document).find(|tool| tool["name"] == name)
}

fn enabled(machine: &Value, name: &str) -> bool {
    tool_entry(machine, name).is_some_and(|tool| tool["availability"]["enabled"] == true)
}

/// The one spelling of the per-tool query-schema command, in every catalog view.
const QUERY_COMMAND: &str =
    "octocode schema <tool> --view query [--select operation=VALUE[,FIELD=VALUE]]";

/// The CLI's own notes, after the shared instructions in the full catalog.
const CLI_INSTRUCTIONS: &str = "<cli>Listed tools are available; execution checks credentials and prerequisites. Fields: schema <name> --view query. A next.* page still exits 0.</cli>";

/// Core's instructions (embedded through config) composed for the tools
/// `available` says exist: the routes whose tools are all there, then each such
/// tool's block. Served by MCP `initialize` and, with the CLI's own notes, by
/// the full catalog.
pub(super) fn instructions(presentation: &Value, available: impl Fn(&str) -> bool) -> String {
    octocode_native::contracts::compose_instructions(presentation, available)
}

/// Select a core-generated presentation using the runtime's resolved switch.
pub(super) fn configured_tool<'a>(tool: &'a Value, machine: &Value) -> &'a Value {
    if machine["presentation"]["selection"] == true {
        tool.get("selection").unwrap_or(tool)
    } else {
        tool
    }
}

/// What prints the tool-flow guide: the CLI's reader of the MCP resource.
const TOOL_FLOW_COMMAND: &str = "octocode schema --view flow";

/// Enabled tools and discovery commands; the full view adds instructions,
/// the grammar inventory, setup routes and field summaries.
pub(super) fn catalog(machine: &Value, view: Option<&str>) -> Result<Value, SchemaError> {
    if view.is_some_and(|view| view != "full") {
        return Err(usage(
            "Without a tool, only --view full or --view flow is supported.",
        ));
    }
    let presentation = presentation()?;
    let listed: Vec<Value> = tools(presentation)
        .filter_map(|tool| {
            let name = tool["name"].as_str()?;
            let entry = tool_entry(machine, name).filter(|_| enabled(machine, name))?;
            Some(json!({
                "name": name,
                "title": tool["title"],
                "when": octocode_native::contracts::cli_tool_trigger(name),
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
            "query": QUERY_COMMAND,
            "variants": "octocode schema <tool> --view variants",
            // First-run setup an agent cannot discover from the tool list.
            "setup": {
                "auth": "octocode auth status (login: octocode auth login)",
                "mcp": "octocode install --ide <id> (ids: octocode install --list)",
                "skills": "octocode skill install --all --platform <claude|codex|cursor|...> --global",
                "graph": "octocode graphBuild --help, then octocode graphQuery --help",
            },
        },
        "instructions": format!(
            "{}\n{CLI_INSTRUCTIONS}",
            instructions(presentation, |name| enabled(machine, name))
        ),
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
            "query": QUERY_COMMAND,
            "full": "octocode schema --view full",
            "flow": TOOL_FLOW_COMMAND
        });
    }
    Ok(value)
}

/// What every form of tool `name` requires, for the root help's entry per
/// tool, as `;`-separated parts: the fields every form shares, then
/// `operation a|b: f` for the operations that also need `f`
/// (`owner, repo; operation pullRequest|issue: number; commit: ref`); `none`
/// when every field is optional. A form that only adds fields to another
/// form of the same operations (a `view`, `ranges`) is that form narrowed,
/// so the wider form stands for it. Read from the variants view
/// `schema <tool> --view variants` serves, so the entry and that view never
/// disagree.
pub(super) fn required_by_operation(name: &str) -> Option<Vec<String>> {
    let tool = tool_entry(presentation().ok()?, name)?;
    let strings = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    let forms: Vec<(Vec<String>, Vec<String>)> = tool["views"]["variants"]["variants"]
        .as_array()?
        .iter()
        .map(|form| (strings(&form["operations"]), strings(&form["requires"])))
        .collect();
    let narrows = |(operations, requires): &(Vec<String>, Vec<String>)| {
        forms.iter().any(|(wider_operations, wider_requires)| {
            operations.iter().all(|op| wider_operations.contains(op))
                && wider_requires.len() < requires.len()
                && wider_requires.iter().all(|field| requires.contains(field))
        })
    };
    // One group per required field set: the operations that need exactly it.
    let mut groups: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    for (operations, requires) in forms.iter().filter(|form| !narrows(form)) {
        let fields: Vec<String> = requires
            .iter()
            .filter(|field| *field != "operation")
            .cloned()
            .collect();
        let same = |group: &&mut (Vec<String>, Vec<String>)| {
            group.1.len() == fields.len() && fields.iter().all(|field| group.1.contains(field))
        };
        match groups.iter_mut().find(same) {
            Some(group) => {
                for op in operations {
                    if !group.0.contains(op) {
                        group.0.push(op.clone());
                    }
                }
            }
            None => groups.push((operations.clone(), fields)),
        }
    }
    let shared: Vec<String> = groups.first().map_or_else(Vec::new, |(_, fields)| {
        fields
            .iter()
            .filter(|field| groups.iter().all(|group| group.1.contains(field)))
            .cloned()
            .collect()
    });
    let mut parts = Vec::new();
    if !shared.is_empty() {
        parts.push(shared.join(", "));
    }
    let mut lead = "operation ";
    for (operations, fields) in &groups {
        let rest = fields
            .iter()
            .filter(|field| !shared.contains(field))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        // A form selected by no operation: its own fields, as an alternative.
        if operations.is_empty() {
            if !rest.is_empty() {
                parts.push(rest);
            }
            continue;
        }
        let ops = operations.join("|");
        parts.push(if rest.is_empty() {
            format!("{lead}{ops}")
        } else {
            format!("{lead}{ops}: {rest}")
        });
        lead = "";
    }
    if parts.is_empty() {
        parts.push("none".to_owned());
    }
    Some(parts)
}

/// The command that prints what `name`'s output fields mean.
pub(super) fn output_command(name: &str) -> String {
    format!("octocode schema {name} --view output")
}

/// `octocode schema <tool> [--view] [--select]`.
pub(super) fn tool_view(
    machine: &Value,
    name: &str,
    view: Option<&str>,
    select: Option<&str>,
) -> Result<Value, SchemaError> {
    let presentation = presentation()?;
    let Some(tool) = tool_entry(presentation, name) else {
        let known = tools(presentation)
            .filter_map(|tool| tool["name"].as_str())
            .filter(|tool| enabled(machine, tool))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(usage(format!("Unknown tool: {name}. Known tools: {known}")));
    };
    if view == Some("output") {
        if select.is_some() {
            return Err(usage("--select requires the query view and a tool name"));
        }
        return output_view(tool);
    }
    let configured = configured_tool(tool, machine);
    let view = view.unwrap_or("query");
    if configured["views"].get(view).is_none() {
        return Err(usage(
            "A tool's views: query (default), variants or output.",
        ));
    }
    let mut value = match select {
        // The query view states the input contract once: its query fields.
        // The call envelope around them (`{"queries":[…]}` or
        // `{"cursor":"…"}`) is the same for every tool and `--help` states
        // it, so the envelope's copy of the fields is not repeated.
        None if view == "query" => {
            let mut query = configured["views"][view].clone();
            if let Some(fields) = query.as_object_mut() {
                fields.remove("inputSchema");
            }
            query
        }
        None => configured["views"][view].clone(),
        Some(_) if view != "query" => {
            return Err(usage("--select requires the query view and a tool name"));
        }
        Some(selection) => selected(tool, configured, selection)?.clone(),
    };
    if octocode_native::id::ToolId::from_name(name)
        .is_some_and(octocode_native::id::ToolId::lists_lsp_servers)
        && let (Some(description), Some(servers)) = (
            value["description"].as_str(),
            machine["lspServers"].as_array(),
        )
    {
        let servers: Vec<&str> = servers.iter().filter_map(Value::as_str).collect();
        value["description"] = Value::String(with_lsp_servers(description, &servers));
    }
    value["availability"] = tool_entry(machine, name)
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

/// One `FIELD=VALUE` pair of a selection; a JSON-quoted value reads as the
/// bare one.
fn pair(text: &str) -> Option<(&str, String)> {
    let (field, raw) = text.split_once('=')?;
    let (field, raw) = (field.trim(), raw.trim());
    if field.is_empty() || raw.is_empty() {
        return None;
    }
    let value = serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_owned());
    Some((field, value))
}

/// The schema a `#/$defs/<name>` reference of `schema` names, through
/// chained references; `at` itself when it is none.
fn resolved<'a>(schema: &'a Value, mut at: &'a Value) -> &'a Value {
    while let Some(target) = at["$ref"]
        .as_str()
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| schema["$defs"].get(name))
    {
        at = target;
    }
    at
}

/// The values the forms of a selected row (`row["querySchema"]`: one object,
/// or alternatives of them) let `field` take: `None` when no form has the
/// field, an empty list when one takes any value.
fn admitted(row: &Value, field: &str) -> Option<Vec<String>> {
    let schema = &row["querySchema"];
    let resolve = |at| resolved(schema, at);
    let forms = ["anyOf", "oneOf"]
        .iter()
        .flat_map(|key| schema[*key].as_array().into_iter().flatten())
        .chain([schema]);
    let mut values = Vec::new();
    let mut open = false;
    let mut found = false;
    for property in forms.filter_map(|form| resolve(form)["properties"].get(field)) {
        found = true;
        let property = resolve(property);
        let closed: Vec<&Value> = property
            .get("const")
            .into_iter()
            .chain(property["enum"].as_array().into_iter().flatten())
            .collect();
        open |= closed.is_empty();
        for value in closed {
            let text = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
            if !values.contains(&text) {
                values.push(text);
            }
        }
    }
    found.then(|| if open { Vec::new() } else { values })
}

/// The `--select operation=X[,FIELD=VALUE]` view: that operation's row, and
/// with a second pair the row the presentation narrows to it
/// (`operation=match,view=content`). Where the presentation tells the
/// operation's forms apart by no such pair, a pair one of the forms admits
/// selects the operation's row: nothing narrower exists. The rows are
/// `configured`'s (the `selection` presentation beside a classification
/// provider); the operation names of an error are `tool`'s.
fn selected<'a>(
    tool: &Value,
    configured: &'a Value,
    selection: &str,
) -> Result<&'a Value, SchemaError> {
    let expects = || {
        usage("--select expects operation=VALUE[,FIELD=VALUE], e.g. operation=match,view=content")
    };
    let mut pairs = selection.split(',').map(pair);
    let (Some(Some((field, operation))), second, None) = (pairs.next(), pairs.next(), pairs.next())
    else {
        return Err(expects());
    };
    if field != "operation" {
        return Err(expects());
    }
    let row = configured["selections"]
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
        })?;
    let Some(second) = second else {
        return Ok(row);
    };
    let (field, value) = second.ok_or_else(expects)?;
    if let Some(narrowed) =
        configured["selections"].get(format!("operation={operation},{field}={value}"))
    {
        return Ok(narrowed);
    }
    match admitted(row, field) {
        None => Err(usage(format!(
            "operation={operation} has no field {field}."
        ))),
        Some(values) if !values.is_empty() && !values.contains(&value) => Err(usage(format!(
            "operation={operation} takes {field}: {}",
            values.join(", ")
        ))),
        Some(_) => Ok(row),
    }
}

#[cfg(test)]
mod tests;
