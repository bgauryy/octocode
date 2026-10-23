use super::super::mcp_clients::ClientSpec;
use super::projection;
use octocode_native::config::{config_revision, read_private_config};
use serde_json::{Value, json};
use std::path::Path;

/// Read-only row for Continue's main `config.json`, when it exists.
pub(super) fn continue_main_rows(client: &ClientSpec, directory: &Path) -> Vec<Value> {
    let path = directory.join("config.json");
    std::fs::symlink_metadata(&path)
        .is_ok()
        .then(|| inspect_continue_main(client, &path))
        .into_iter()
        .collect()
}

pub(super) fn inspect_continue_main(client: &ClientSpec, path: &Path) -> Value {
    let mut row = json!({"client":client.id,"scope":"home-config-json","path":path,"writable":false,"supportsEnabled":false,"restartRequired":false,"configured":false,"status":"absent","entries":[],"error":"Continue main configuration uses an array of servers. Manage these entries in Continue; the Octocode standalone block can be edited here."});
    let original = match read_private_config(path) {
        Ok(text) => text,
        Err(error) => {
            row["status"] = json!("invalid");
            row["error"] = json!(format!(
                "Cannot read Continue configuration safely: {error}"
            ));
            return row;
        }
    };
    row["revision"] = json!(config_revision(original.as_deref()));
    let Some(text) = original else {
        return row;
    };
    let root = match octocode_native::config::parse_config_json(&text) {
        Ok(root) => root,
        Err(error) => {
            row["status"] = json!("invalid");
            row["error"] = json!(error);
            return row;
        }
    };
    let mut entries = Vec::new();
    let mut unresolved = false;
    for servers in [
        root.get("mcpServers"),
        root.pointer("/experimental/modelContextProtocolServers"),
    ] {
        if let Some(servers) = servers.and_then(Value::as_array) {
            for server in servers {
                unresolved |= server.get("uses").is_some();
                let transport = server.get("transport").unwrap_or(server);
                if transport
                    .get("args")
                    .and_then(Value::as_array)
                    .is_some_and(|args| {
                        args.iter().any(|arg| {
                            arg.as_str().is_some_and(|arg| {
                                arg == "octocode-mcp" || arg.starts_with("octocode-mcp@")
                            })
                        })
                    })
                    || server
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.eq_ignore_ascii_case("octocode"))
                {
                    entries.push(projection(client, Some(transport)));
                }
            }
        }
    }
    row["configured"] = if unresolved && entries.is_empty() {
        Value::Null
    } else {
        json!(!entries.is_empty())
    };
    row["status"] = json!(if !entries.is_empty() {
        "configured"
    } else if unresolved {
        "unknown"
    } else {
        "absent"
    });
    row["coverage"] =
        json!("Inline Octocode package/name entries; imported MCP blocks are not resolved.");
    if unresolved {
        row["error"] = json!(
            "Continue imports MCP blocks whose contents are not inspected. Manage this main configuration in Continue."
        );
    }
    row["entries"] = json!(entries);
    row
}
