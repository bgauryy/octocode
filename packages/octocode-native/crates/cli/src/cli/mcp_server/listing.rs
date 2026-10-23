//! What the MCP server advertises: the tool definitions and the server
//! instructions, composed from the embedded presentation (`octocode-config`
//! emits it from the same core Zod contract) and the runtime catalog.
//!
//! The presentation carries every variant core can render for a tool: its
//! `mcp` definition at rest, the context-selection variant (`selection`) and,
//! for clasify, one per set of exposed source readers (`sourceViews`). This
//! module only chooses among them with runtime truth: which tools are
//! available, whether selection is on, which language servers resolve and
//! which tools are deferred behind the `run` dispatcher.
use super::super::schema::{instructions, with_lsp_servers};
use serde_json::Value;

/// The `tools/list` entries (JSON, in catalog order) and `initialize`
/// instructions.
pub(super) struct Listing {
    pub tools: Vec<Value>,
    pub instructions: String,
    /// The dispatcher's name when deferred tools run through it.
    pub dispatcher: Option<String>,
}

/// The tool names the catalog marks available, in catalog order.
fn available(catalog: &Value) -> Vec<&str> {
    catalog["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|tool| tool["available"] == true)
        .filter_map(|tool| tool["name"].as_str())
        .collect()
}

/// The variant of `tool` this runtime lists: selection when on, and for
/// clasify the one for the exposed source readers.
fn variant<'a>(tool: &'a Value, selection: bool, sources: &str) -> Option<&'a Value> {
    if tool["name"] == "clasify" {
        return tool["sourceViews"].get(sources);
    }
    Some(if selection {
        tool.get("selection").unwrap_or(tool)
    } else {
        tool
    })
}

/// One tool's `tools/list` entry from its presentation `mcp` definition.
fn entry(name: &str, mcp: &Value, lsp_servers: Option<&[&str]>) -> Value {
    let mut description = mcp["description"].clone();
    if name == "lspSearch"
        && let (Some(text), Some(servers)) = (mcp["description"].as_str(), lsp_servers)
    {
        description = Value::String(with_lsp_servers(text, servers));
    }
    let mut entry = serde_json::Map::new();
    entry.insert("name".into(), name.into());
    entry.insert("title".into(), mcp["title"].clone());
    entry.insert("description".into(), description);
    entry.insert("inputSchema".into(), mcp["inputSchema"].clone());
    if let Some(annotations) = mcp.get("annotations") {
        entry.insert("annotations".into(), annotations.clone());
    }
    Value::Object(entry)
}

/// The `run` tool for the deferred tools: the dispatcher's intro, then one
/// line per deferred tool, over an input schema that names only them.
fn dispatcher(presentation: &Value, deferred: &[(&str, &Value)]) -> Value {
    let dispatcher = &presentation["mcpDispatcher"];
    let lines = deferred
        .iter()
        .filter_map(|(_, mcp)| mcp["deferredLine"].as_str());
    let description = std::iter::once(dispatcher["intro"].as_str().unwrap_or_default())
        .chain(lines)
        .collect::<Vec<_>>()
        .join("\n");
    let mut input_schema = dispatcher["inputSchema"].clone();
    input_schema["properties"]["tool"]["enum"] =
        Value::Array(deferred.iter().map(|(name, _)| (*name).into()).collect());
    serde_json::json!({
        "name": dispatcher["name"],
        "title": dispatcher["title"],
        "description": description,
        "inputSchema": input_schema,
    })
}

/// The listing for this runtime's catalog.
pub(super) fn build(presentation: &Value, catalog: &Value) -> Result<Listing, String> {
    let names = available(catalog);
    if names.is_empty() {
        return Err("No native tools are available".into());
    }
    let selection = catalog["presentation"]["selection"] == true;
    let lsp_servers = catalog["lspServers"]
        .as_array()
        .map(|servers| servers.iter().filter_map(Value::as_str).collect::<Vec<_>>());
    let sources = presentation["clasifySourceTools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|tool| names.contains(tool))
        .collect::<Vec<_>>()
        .join(",");
    let is_deferred = |name: &str| {
        catalog["presentation"]["deferred"]
            .as_array()
            .is_some_and(|deferred| deferred.iter().any(|tool| tool == name))
    };
    let mut tools = Vec::new();
    let mut deferred = Vec::new();
    for name in &names {
        let mcp = presentation["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|tool| tool["name"] == *name)
            .and_then(|tool| variant(tool, selection, &sources))
            .map(|variant| &variant["mcp"])
            .filter(|mcp| mcp.is_object())
            .ok_or_else(|| format!("Native catalog tool has no contract: {name}"))?;
        if is_deferred(name) {
            deferred.push((*name, mcp));
        } else {
            tools.push(entry(name, mcp, lsp_servers.as_deref()));
        }
    }
    let dispatcher = (!deferred.is_empty()).then(|| dispatcher(presentation, &deferred));
    let dispatcher_name = dispatcher
        .as_ref()
        .and_then(|tool| tool["name"].as_str().map(str::to_owned));
    tools.extend(dispatcher);
    Ok(Listing {
        tools,
        dispatcher: dispatcher_name,
        instructions: instructions(
            &presentation["instructionSections"],
            |name| names.contains(&name),
            selection,
            false,
        ),
    })
}

#[cfg(test)]
mod tests;
