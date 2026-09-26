use crate::{
    catalog,
    cli::{Args, output},
    database,
    store::Store,
    wire::Wire,
};
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

const PROXY_INSTRUCTIONS: &str = "You are a managed communication worker; the host owns identity, presence and delivery. Use only bound tools to execute the user task and its explicit response rules. Never initiate messages, broadcasts or subscriptions, and reply only as the task authorizes, never to acknowledgements or notices. Call inbox only for requested recovery. Handle each injected ID once, ack it, then end the turn.";

pub fn run(args: &Args) -> Result<()> {
    let vendor = args
        .vendor
        .as_deref()
        .ok_or_else(|| anyhow!("--vendor required"))?;
    let mut input = json!({"vendor":vendor,"model":args.model,"prompt":args.prompt});
    if let Some(duration) = args.duration_ms {
        input["durationMs"] = json!(duration);
    }
    if let Some(name) = &args.name {
        input["name"] = json!(name);
    }
    catalog::command("run", &input)?;
    let model = catalog::text(&input, "model")?;
    let prompt = catalog::text(&input, "prompt")?;
    let store = Store::open(
        database::path(args.database.as_deref())?,
        &args.workspace,
        false,
        true,
    )?;
    let session = if let Some(session) = &args.session {
        store.call(session, "resume", &json!({"vendor":vendor}))?
    } else {
        store.call(
            "",
            "join",
            &json!({"vendor":vendor,"name":args.name.as_deref().unwrap_or(vendor)}),
        )?
    };
    let id = session["id"]
        .as_str()
        .ok_or_else(|| anyhow!("Missing session ID"))?;
    let task: String = prompt
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(256)
        .collect();
    store.call(id, "heartbeat", &json!({"task":task,"status":"busy"}))?;
    let result = worker(args, &store, id, vendor, model, prompt);
    let cleanup = store.call(id, "leave", &json!({}));
    result?;
    cleanup?;
    Ok(())
}
fn worker(
    args: &Args,
    store: &Store,
    id: &str,
    vendor: &str,
    model: &str,
    prompt: &str,
) -> Result<()> {
    // This worker is the identity's only delivery owner for as long as it runs.
    let _owner = crate::dispatch::claim_delivery_owner(&store.database, id)?;
    let stop = Arc::new(AtomicBool::new(false));
    let signal = stop.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
    let deadline = args
        .duration_ms
        .map(|ms| Instant::now() + Duration::from_millis(ms));
    // A reparented worker lost its launcher; stop instead of running unobserved.
    #[cfg(unix)]
    let parent = nix::unistd::getppid();
    let tools = catalog::selected_tools(args.tools.as_deref())?;
    let mut mcp_args = vec![
        json!("mcp"),
        json!("--workspace"),
        json!(store.workspace),
        json!("--database"),
        json!(store.database),
        json!("--session"),
        json!(id),
    ];
    if let Some(selection) = &args.tools {
        mcp_args.extend([json!("--tools"), json!(selection)]);
    }
    let mcp = json!({"command":std::env::current_exe()?,"args":mcp_args});
    let tool_names: Vec<Value> = tools.iter().map(|t| t["name"].clone()).collect();
    // Vendor discovery must not inherit repository files; bound tools retain the real workspace.
    let worker_dir = tempfile::tempdir()?;
    let worker_cwd = std::fs::canonicalize(worker_dir.path())?;
    let mut environment = Vec::new();
    let vendor_args = if vendor == "pi" {
        let extension = worker_dir.path().join("communication.mjs");
        std::fs::write(&extension, include_str!("runtime/pi-extension.mjs"))?;
        environment.push(("OCTOCODE_COMMUNICATION_BINDING", json!({"binary":std::env::current_exe()?,"workspace":store.workspace,"database":store.database,"session":id,"tools":tools}).to_string()));
        vec![
            "--mode".into(),
            "rpc".into(),
            "--model".into(),
            model.into(),
            "--thinking".into(),
            "off".into(),
            "--system-prompt".into(),
            PROXY_INSTRUCTIONS.into(),
            "--no-session".into(),
            "--no-extensions".into(),
            "--no-skills".into(),
            "--no-prompt-templates".into(),
            "--no-context-files".into(),
            "--no-builtin-tools".into(),
            "--extension".into(),
            extension.to_string_lossy().into_owned(),
        ]
    } else if vendor == "codex" {
        vec!["app-server".to_owned()]
    } else {
        vec![
            "-p".into(),
            "--settings".into(),
            json!({"disableAllHooks":true,"autoMemoryEnabled":false}).to_string(),
            "--system-prompt".into(),
            PROXY_INSTRUCTIONS.into(),
            "--model".into(),
            model.into(),
            "--input-format".into(),
            "stream-json".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--no-session-persistence".into(),
            "--setting-sources".into(),
            "".into(),
            "--strict-mcp-config".into(),
            "--mcp-config".into(),
            json!({"mcpServers":{"agents_communication":mcp}}).to_string(),
            "--tools".into(),
            "".into(),
            "--allowedTools".into(),
            "mcp__agents_communication__*".into(),
            "--permission-mode".into(),
            "dontAsk".into(),
            "--disable-slash-commands".into(),
        ]
    };
    let mut host = Wire::start(
        vendor,
        &vendor_args,
        &worker_cwd,
        &environment,
        deadline,
        stop.clone(),
    )?;
    let mut vendor_session = String::new();
    if vendor == "codex" {
        host.request("initialize",json!({"clientInfo":{"name":"octocode-agents-communication","version":env!("CARGO_PKG_VERSION")}}),&stop)?;
        host.send(&json!({"method":"initialized","params":{}}))?;
        store.call(id, "heartbeat", &json!({}))?;
        let configured = host.request("config/read", json!({"includeLayers":false}), &stop)?;
        let mut servers = serde_json::Map::new();
        let mut plugins = serde_json::Map::new();
        if let Some(existing) = configured["config"]["plugins"].as_object() {
            for name in existing.keys() {
                plugins.insert(name.clone(), json!({"enabled":false}));
            }
        }
        if let Some(existing) = configured["config"]["mcp_servers"].as_object() {
            for name in existing.keys() {
                servers.insert(name.clone(), json!({"enabled":false}));
            }
        }
        let mut owned = mcp.clone();
        owned["enabled"] = json!(true);
        owned["enabled_tools"] = json!(tool_names);
        owned["default_tools_approval_mode"] = json!("approve");
        servers.insert("agents_communication".into(), owned);
        let discovered = host.request(
            "skills/list",
            json!({"cwds":[worker_cwd],"forceReload":true}),
            &stop,
        )?;
        let mut skills = Vec::new();
        for entry in discovered["data"]
            .as_array()
            .ok_or_else(|| anyhow!("Invalid Codex skills list"))?
        {
            for skill in entry["skills"]
                .as_array()
                .ok_or_else(|| anyhow!("Invalid Codex skills entry"))?
            {
                let path = skill["path"]
                    .as_str()
                    .ok_or_else(|| anyhow!("Missing Codex skill path"))?;
                skills.push(json!({"path":path,"enabled":false}));
            }
        }
        store.call(id, "heartbeat", &json!({}))?;
        let started=host.request("thread/start",json!({
            "model":model,"cwd":worker_cwd,"approvalPolicy":"never","sandbox":"read-only","ephemeral":true,
            "baseInstructions":PROXY_INSTRUCTIONS,
            "developerInstructions":"",
            "config":{"mcp_servers":servers,"plugins":plugins,"project_doc_max_bytes":0,"skills":{"config":skills},"web_search":"disabled",
                "features":{"code_mode":{"enabled":false},"shell_tool":false,"apply_patch_freeform":false,"multi_agent":false,"memories":false,"hooks":false,"apps":false,"skill_search":false}}
        }),&stop)?;
        vendor_session = started["thread"]["id"]
            .as_str()
            .ok_or_else(|| anyhow!("No vendor session ID"))?
            .into();
        store.call(id, "heartbeat", &json!({"vendorSession":vendor_session}))?;
    } else if vendor == "pi" {
        let state = host.pi_request("get_state", json!({}), &stop)?;
        vendor_session = state["sessionId"]
            .as_str()
            .ok_or_else(|| anyhow!("No Pi session ID"))?
            .into();
        store.call(id, "heartbeat", &json!({"vendorSession":vendor_session}))?;
    }
    output(&json!({"type":"ready","session":id,"vendor":vendor,"pid":host.pid()}))?;
    let guidance = format!(
        "Available communication tools: {}\n\n{}\n\nBound communication identity: {id}. {}\n\nUser task:\n{prompt}",
        serde_json::to_string(&tool_names)?,
        catalog::worker_skill(),
        store.peer_context(id, "worker", "session")?
    );
    let initial = store.stage(id, "initial")?;
    let guidance = if initial.is_empty() {
        guidance
    } else {
        format!("{guidance}\n\n{}", crate::dispatch::context(&initial))
    };
    let delivered = deliver(&mut host, vendor, &vendor_session, &guidance, &stop);
    store.finish_dispatch(
        id,
        &initial,
        delivered.as_ref().err().map(ToString::to_string).as_deref(),
    )?;
    delivered?;
    let mut busy = true;
    let mut heartbeat = Instant::now();
    let mut poll = Instant::now();
    let mut calls = HashMap::new();
    let mut pi_error = None;
    while !stop.load(Ordering::Relaxed) && deadline.is_none_or(|d| Instant::now() < d) {
        #[cfg(unix)]
        if nix::unistd::getppid() != parent {
            break;
        }
        if heartbeat.elapsed() >= Duration::from_secs(15) {
            store.call(id, "heartbeat", &json!({}))?;
            heartbeat = Instant::now();
        }
        if let Some(event) = host.event(Duration::from_millis(100))? {
            persist_usage(store, id, vendor, &vendor_session, &event)?;
            if args.trace {
                trace_usage(vendor, &event)?;
                for record in trace_tools(id, &event, &mut calls) {
                    output(&record)?;
                }
                output(
                    &json!({"type":"protocol","method":event.get("method").or_else(||event.get("type")),"itemType":event["params"]["item"]["type"],"status":event["params"]["turn"]["status"]}),
                )?;
            }
            if vendor == "pi" {
                if event["type"] == "response" && event["success"] == false {
                    bail!("Pi command failed: {}", event["error"]);
                }
                if event["type"] == "message_end" && event["message"]["role"] == "assistant" {
                    let message = &event["message"];
                    pi_error =
                        if message["stopReason"] == "error" || message["stopReason"] == "aborted" {
                            Some(message["errorMessage"].clone())
                        } else {
                            None
                        };
                    if let Some(content) = message["content"].as_array() {
                        for item in content.iter().filter(|item| item["type"] == "text") {
                            output(&json!({"type":"text","text":item["text"]}))?;
                        }
                    }
                }
                if event["type"] == "agent_settled" {
                    if let Some(error) = &pi_error {
                        bail!("Pi turn failed: {error}");
                    }
                    busy = false;
                    output(&json!({"type":"turn-completed","vendor":vendor}))?;
                }
            }
            if event["method"] == "item/completed"
                && event["params"]["item"]["type"] == "agentMessage"
            {
                output(&json!({"type":"text","text":event["params"]["item"]["text"]}))?;
            }
            if event["method"] == "turn/completed" {
                if event["params"]["turn"]["status"] == "failed" {
                    bail!("Vendor turn failed: {}", event["params"]["turn"]["error"]);
                }
                busy = false;
                output(&json!({"type":"turn-completed","vendor":vendor}))?;
            }
            if event["method"] == "error" && event["params"]["willRetry"] != true {
                bail!("Vendor error: {}", event["params"]);
            }
            if event.get("method").is_some() && event.get("id").is_some() {
                host.send(&crate::wire::refusal(&event["id"]))?;
            }
            if event["type"] == "system" && event["subtype"] == "init" {
                vendor_session = event["session_id"]
                    .as_str()
                    .ok_or_else(|| anyhow!("No vendor session ID"))?
                    .into();
                store.call(id, "heartbeat", &json!({"vendorSession":vendor_session}))?;
            }
            if event["type"] == "assistant"
                && let Some(content) = event["message"]["content"].as_array()
            {
                for item in content {
                    if item["type"] == "text" {
                        output(&json!({"type":"text","text":item["text"]}))?;
                    }
                }
            }
            if event["type"] == "result" {
                if event["is_error"] == true {
                    bail!("Vendor turn failed: {event}");
                }
                busy = false;
                output(&json!({"type":"turn-completed","vendor":vendor}))?;
            }
        }
        if !busy && poll.elapsed() >= Duration::from_millis(500) {
            poll = Instant::now();
            let items = store.stage(id, &format!("managed:{vendor}"))?;
            if !items.is_empty() {
                busy = true;
                output(
                    &json!({"type":"delivery","messages":items.iter().map(|v|v["id"].clone()).collect::<Vec<_>>()}),
                )?;
                let delivered = deliver(
                    &mut host,
                    vendor,
                    &vendor_session,
                    &crate::dispatch::with_peers(
                        &items,
                        &store.peer_context(id, "worker", "session")?,
                    ),
                    &stop,
                );
                store.finish_dispatch(
                    id,
                    &items,
                    delivered.as_ref().err().map(ToString::to_string).as_deref(),
                )?;
                delivered?;
            }
        }
    }
    host.close()?;
    Ok(())
}

fn trace_usage(vendor: &str, event: &Value) -> Result<()> {
    let (scope, usage) = if event["method"] == "thread/tokenUsage/updated" {
        ("thread", &event["params"]["tokenUsage"])
    } else if event["type"] == "result" {
        ("result", &event["usage"])
    } else if event["type"] == "assistant"
        || (event["type"] == "message_end" && event["message"]["role"] == "assistant")
    {
        ("message", &event["message"]["usage"])
    } else {
        return Ok(());
    };
    if !usage.is_null() {
        output(
            &json!({"type":"usage","vendor":vendor,"scope":scope,"messageId":event["message"]["id"],"usage":usage}),
        )?;
    }
    Ok(())
}
fn deliver(
    host: &mut Wire,
    vendor: &str,
    session: &str,
    input: &str,
    stop: &AtomicBool,
) -> Result<()> {
    if vendor == "codex" {
        host.request(
            "turn/start",
            json!({"threadId":session,"input":[{"type":"text","text":input}],"effort":"low"}),
            stop,
        )?;
    } else if vendor == "pi" {
        host.pi_request("prompt", json!({"message":input}), stop)?;
    } else {
        host.send(&json!({"type":"user","message":{"role":"user","content":input},"session_id":session,"parent_tool_use_id":null}))?;
    }
    Ok(())
}

fn persist_usage(
    store: &Store,
    id: &str,
    vendor: &str,
    vendor_session: &str,
    event: &Value,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    let (usage, scope) = if vendor == "codex" && event["method"] == "thread/tokenUsage/updated" {
        (&event["params"]["tokenUsage"]["total"], "cumulative")
    } else if vendor == "claude" && event["type"] == "result" {
        (&event["usage"], "turn")
    } else if vendor == "pi"
        && event["type"] == "message_end"
        && event["message"]["role"] == "assistant"
    {
        (&event["message"]["usage"], "request")
    } else {
        return Ok(());
    };
    if !usage.is_object() {
        return Ok(());
    }
    let identity = if scope == "cumulative" {
        json!([vendor_session, usage])
    } else if !event["uuid"].is_null() || !event["message"]["timestamp"].is_null() {
        json!([vendor_session, event["uuid"], event["message"]["timestamp"]])
    } else {
        json!([vendor_session, Uuid::new_v4().to_string()])
    };
    let key = Sha256::digest(identity.to_string())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut record = json!({"key":key,"scope":scope});
    for (output, keys) in [
        ("inputTokens", ["inputTokens", "input_tokens", "input"]),
        ("outputTokens", ["outputTokens", "output_tokens", "output"]),
        (
            "cachedInputTokens",
            ["cachedInputTokens", "cache_read_input_tokens", "cacheRead"],
        ),
        (
            "cacheWriteTokens",
            [
                "cacheWriteTokens",
                "cache_creation_input_tokens",
                "cacheWrite",
            ],
        ),
    ] {
        if let Some(value) = keys.iter().find_map(|key| usage[*key].as_u64()) {
            record[output] = json!(value);
        }
    }
    if let Some(value) = event["params"]["tokenUsage"]["last"]["inputTokens"].as_u64() {
        record["contextTokens"] = json!(value);
    }
    if vendor == "pi"
        && let (Some(input), Some(read), Some(write)) = (
            usage["input"].as_u64(),
            usage["cacheRead"].as_u64(),
            usage["cacheWrite"].as_u64(),
        )
        && let Some(total) = input.checked_add(read).and_then(|n| n.checked_add(write))
    {
        record["contextTokens"] = json!(total);
    }
    store.record_usage(id, &record)?;
    Ok(())
}

fn trace_tools(session: &str, event: &Value, calls: &mut HashMap<String, String>) -> Vec<Value> {
    let mut records = Vec::new();
    let item = &event["params"]["item"];
    if item["type"] == "mcpToolCall" {
        if event["method"] == "item/started" {
            records.push(json!({"type":"tool-call","callId":item["id"],"server":item["server"],"tool":item["tool"],"input":item["arguments"]}));
        } else if event["method"] == "item/completed" {
            records.push(json!({"type":"tool-result","callId":item["id"],"server":item["server"],"tool":item["tool"],"result":item["result"],"error":item["error"]}));
        }
    }
    if event["type"] == "tool_execution_start" {
        records.push(json!({"type":"tool-call","callId":event["toolCallId"],"tool":event["toolName"],"input":event["args"]}));
    }
    if event["type"] == "tool_execution_end" {
        records.push(json!({"type":"tool-result","callId":event["toolCallId"],"tool":event["toolName"],"result":event["result"],"isError":event["isError"]}));
    }
    if let Some(content) = event["message"]["content"].as_array() {
        for item in content {
            if item["type"] == "tool_use" {
                if let (Some(id), Some(name)) = (item["id"].as_str(), item["name"].as_str()) {
                    calls.insert(id.to_owned(), name.to_owned());
                    records.push(
                        json!({"type":"tool-call","callId":id,"tool":name,"input":item["input"]}),
                    );
                }
            } else if item["type"] == "tool_result" {
                let tool = item["tool_use_id"].as_str().and_then(|id| calls.remove(id));
                records.push(json!({"type":"tool-result","callId":item["tool_use_id"],"tool":tool,"result":item["content"],"isError":item["is_error"]}));
            }
        }
    }
    for record in &mut records {
        record["session"] = json!(session);
        record["at"] = json!(crate::store::now());
    }
    records
}

#[cfg(test)]
mod trace_tests {
    use super::*;
    #[test]
    fn concurrent_calls_keep_identity_for_each_protocol() {
        let mut calls = HashMap::new();
        for id in ["a", "b"] {
            let rows = trace_tools(
                "agent",
                &json!({"type":"tool_execution_start","toolCallId":id,"toolName":"peers","args":{}}),
                &mut calls,
            );
            assert_eq!(rows[0]["callId"], id);
            assert_eq!(rows[0]["session"], "agent");
            trace_tools(
                "agent",
                &json!({"message":{"content":[{"type":"tool_use","id":id,"name":"peers","input":{}}]}}),
                &mut calls,
            );
        }
        for id in ["b", "a"] {
            for event in [
                json!({"type":"tool_execution_end","toolCallId":id,"toolName":"peers","result":{}}),
                json!({"message":{"content":[{"type":"tool_result","tool_use_id":id,"content":[]}]}}),
                json!({"method":"item/completed","params":{"item":{"type":"mcpToolCall","id":id,"tool":"peers","result":{}}}}),
            ] {
                let rows = trace_tools("agent", &event, &mut calls);
                assert_eq!(rows[0]["callId"], id);
                assert_eq!(rows[0]["tool"], "peers");
            }
        }
        assert!(calls.is_empty());
    }
}
