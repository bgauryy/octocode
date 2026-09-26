use crate::{
    catalog,
    cli::emit as output,
    store::{Store, strip_nulls},
    wire::read_frame,
};
use anyhow::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn serve(store: &Store, session: &str, selection: Option<&str>) -> Result<()> {
    // A running server has one embedded contract. Retain it across requests.
    let tools = catalog::selected_tools(selection)?;
    // JSON-RPC request IDs are scoped to this connection, not globally unique.
    let connection = uuid::Uuid::new_v4();
    let mut reader = std::io::stdin().lock();
    loop {
        let line = match read_frame(&mut reader) {
            Ok(Some(line)) => line,
            Ok(None) => break,
            // Reject only the oversized request; the ones queued behind it still get answers.
            Err(error) if error.is::<crate::wire::OversizedFrame>() => {
                crate::wire::skip_line(&mut reader)?;
                output(
                    &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":error.to_string()}}),
                )?;
                continue;
            }
            Err(error) => return Err(error),
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let request: Value = match serde_json::from_slice(&line) {
            Ok(value) => value,
            Err(_) => {
                output(
                    &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid JSON"}}),
                )?;
                continue;
            }
        };
        if request["jsonrpc"] != "2.0"
            || !request["method"].is_string()
            || request
                .get("id")
                .is_some_and(|id| !id.is_number() && !id.is_string())
            || request.get("params").is_some_and(|v| !v.is_object())
        {
            output(
                &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid request"}}),
            )?;
            continue;
        }
        let Some(id) = request.get("id") else {
            continue;
        };
        let result = match request["method"].as_str().unwrap_or("") {
            "initialize" => {
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"octocode-agents-communication","version":env!("CARGO_PKG_VERSION")}})
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools":tools}),
            "tools/call" => {
                let name = request["params"]["name"].as_str().unwrap_or("");
                let allowed = tools.iter().any(|tool| tool["name"] == name);
                let mut arguments = request["params"]
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if matches!(name, "send_message" | "notify_all")
                    && let Some(fields) = arguments.as_object_mut()
                    && !fields.contains_key("key")
                {
                    let digest: String = Sha256::digest(format!("{connection}:{id}"))
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect();
                    let key = format!("mcp:{digest}");
                    fields.insert("key".into(), json!(key));
                }
                let result = if allowed {
                    store.call(session, name, &arguments)
                } else {
                    Err(anyhow::anyhow!("Unknown tool: {name}"))
                };
                match result {
                    Ok(mut value) => {
                        strip_nulls(&mut value);
                        json!({"content":[{"type":"text","text":value.to_string()}]})
                    }
                    Err(error) => {
                        json!({"isError":true,"content":[{"type":"text","text":error.to_string()}]})
                    }
                }
            }
            _ => {
                output(
                    &json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Method not found"}}),
                )?;
                continue;
            }
        };
        output(&json!({"jsonrpc":"2.0","id":id,"result":result}))?;
    }
    Ok(())
}
