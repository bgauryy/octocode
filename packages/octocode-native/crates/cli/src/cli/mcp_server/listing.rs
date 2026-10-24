//! What the MCP server advertises: the tool definitions and the server
//! instructions, composed from the embedded presentation (`octocode-config`
//! emits it from the same core Zod contract) and the runtime catalog.
//!
//! The presentation carries each tool's `mcp` definition at rest and its
//! context-selection variant (`selection`). This module only chooses among
//! them with runtime truth: which tools are available, whether selection is
//! on and which language servers resolve.
use super::super::schema::{instructions, with_lsp_servers};
use serde_json::Value;

/// The `tools/list` entries (JSON, in catalog order) and `initialize`
/// instructions.
pub(super) struct Listing {
    pub tools: Vec<Value>,
    pub instructions: String,
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

/// The variant of `tool` this runtime lists: selection when on.
fn variant(tool: &Value, selection: bool) -> &Value {
    if selection {
        tool.get("selection").unwrap_or(tool)
    } else {
        tool
    }
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
    let mut tools = Vec::new();
    for name in &names {
        let mcp = presentation["tools"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|tool| tool["name"] == *name)
            .map(|tool| &variant(tool, selection)["mcp"])
            .filter(|mcp| mcp.is_object())
            .ok_or_else(|| format!("Native catalog tool has no contract: {name}"))?;
        tools.push(entry(name, mcp, lsp_servers.as_deref()));
    }
    Ok(Listing {
        tools,
        instructions: instructions(presentation).to_owned(),
    })
}

#[cfg(test)]
mod tests;
