use crate::{
    catalog,
    cli::{compact, emit as output},
    store::Store,
    wire::read_frame,
};
use anyhow::Result;
use serde_json::{Value, json};

pub fn serve(store: &Store, session: &str, selection: Option<&str>) -> Result<()> {
    // A running server has one embedded contract. Retain it across requests.
    let tools = catalog::selected_tools(selection)?;
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
                let empty = json!({});
                let result = if allowed {
                    store.call(
                        session,
                        name,
                        request["params"].get("arguments").unwrap_or(&empty),
                    )
                } else {
                    Err(anyhow::anyhow!("Unknown tool: {name}"))
                };
                match result {
                    Ok(value) => {
                        json!({"content":[{"type":"text","text":compact(&value).to_string()}]})
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
