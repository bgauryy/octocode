//! `octocode mcp` over real stdio, one process per protocol era: the
//! handshake, the listing, a tool call, the protocol error of an unlisted
//! tool, the language-server prewarm after `initialize`, and the exit on
//! stdin close.

use crate::support;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;
use support::Workspace;

const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    replies: Receiver<Value>,
    next_id: u64,
}

impl Server {
    fn start(workspace: &Workspace) -> Self {
        Self::start_with(workspace, &[])
    }

    /// [`Self::start`] with extra environment variables.
    fn start_with(workspace: &Workspace, env: &[(&str, &str)]) -> Self {
        let mut child = workspace
            .cli()
            .envs(env.iter().copied())
            .arg("mcp")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (sender, replies) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(serde_json::from_str(&line).unwrap()).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            replies,
            next_id: 0,
        }
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    /// One request and its reply: the full JSON-RPC message.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        match self.replies.recv_timeout(REPLY_TIMEOUT) {
            Ok(reply) => {
                assert_eq!(reply["id"], id, "{reply}");
                reply
            }
            Err(RecvTimeoutError::Timeout) => panic!("no reply to {method}"),
            Err(RecvTimeoutError::Disconnected) => panic!("server closed before replying"),
        }
    }

    /// Close stdin and wait for the server to leave.
    fn finish(mut self) -> std::process::ExitStatus {
        drop(self.stdin.take());
        self.child.wait().unwrap()
    }
}

fn local_search(path: &str) -> Value {
    json!({"name": "localSearch", "arguments": {"queries": [{
        "matchString": ["needle"], "path": path
    }]}})
}

fn tool_names(list: &Value) -> Vec<&str> {
    list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect()
}

#[test]
fn serves_the_2025_era_over_stdio() {
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("a.txt"), "alpha\nbeta needle\n").unwrap();
    let mut server = Server::start(&workspace);

    let init = server.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    let result = &init["result"];
    assert_eq!(result["serverInfo"]["name"], "octocode-mcp");
    assert_eq!(result["capabilities"]["tools"]["listChanged"], false);
    let instructions = octocode_native::contracts::cli_presentation()
        .expect("embedded presentation")["instructions"]
        .as_str()
        .expect("canonical instructions");
    assert!(!instructions.is_empty());
    assert_eq!(result["instructions"], instructions);
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let list = server.request("tools/list", json!({}));
    let names = tool_names(&list);
    assert!(names.contains(&"localSearch"), "{names:?}");
    assert!(!names.contains(&"run"), "only canonical tools are listed");
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "localSearch")
        .unwrap();
    assert!(
        tool["inputSchema"].is_object(),
        "MCP declares its input schema"
    );
    assert!(list["result"].get("_meta").is_none());

    let call = server.request("tools/call", local_search("."));
    assert_eq!(call["result"]["isError"], false, "{call}");
    let text = call["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("a.txt") && text.contains("beta needle"),
        "{text}"
    );
    assert_eq!(
        serde_json::from_str::<Value>(text).unwrap(),
        call["result"]["structuredContent"]
    );

    let invalid = server.request(
        "tools/call",
        json!({"name":"localSearch","arguments":{"queries":[{"path":".","matchString":"needle"}]}}),
    );
    assert_eq!(
        invalid["result"]["isError"], true,
        "a scalar matcher must not be silently coerced: {invalid}"
    );
    let unlisted = server.request("tools/call", json!({"name": "noSuchTool", "arguments": {}}));
    assert_eq!(unlisted["error"]["code"], -32602);
    assert_eq!(unlisted["error"]["message"], "Tool noSuchTool not found");

    assert!(server.finish().success());
}

#[test]
fn serves_the_2026_era_over_stdio() {
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("a.txt"), "beta needle\n").unwrap();
    let mut server = Server::start(&workspace);
    let meta = json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
    });

    let discover = server.request("server/discover", json!({"_meta": meta}));
    let result = &discover["result"];
    assert!(
        result["supportedVersions"]
            .as_array()
            .unwrap()
            .contains(&json!("2026-07-28")),
        "{result}"
    );
    assert_eq!(
        result["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "octocode-mcp"
    );

    let list = server.request("tools/list", json!({"_meta": meta}));
    assert!(tool_names(&list).contains(&"localSearch"));
    assert_eq!(list["result"]["resultType"], "complete");
    assert_eq!(
        list["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["title"],
        "Octocode MCP"
    );

    let mut params = local_search(".");
    params["_meta"] = meta;
    let call = server.request("tools/call", params);
    assert_eq!(call["result"]["resultType"], "complete", "{call}");
    assert!(
        call["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("beta needle")
    );
    assert!(
        call["result"]["_meta"]
            .get("io.modelcontextprotocol/serverInfo")
            .is_some()
    );

    assert!(server.finish().success());
}

#[test]
fn a_rejected_call_leaves_the_server_serving() {
    let workspace = Workspace::new();
    let mut server = Server::start(&workspace);
    server.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let empty = server.request(
        "tools/call",
        json!({"name": "localSearch", "arguments": {"queries": []}}),
    );
    assert_eq!(empty["result"]["isError"], true, "{empty}");
    assert_eq!(server.request("ping", json!({}))["result"], json!({}));

    assert!(server.finish().success());
}

/// After `initialize`, the server starts the workspace's language server in
/// the background: the configured server runs before any call names a file.
/// `lsp.prewarm` off starts none.
#[cfg(unix)]
#[test]
fn initialize_prewarms_the_workspace_language_server() {
    use std::os::unix::fs::PermissionsExt;
    for (prewarm, starts) in [("targeted", true), ("off", false)] {
        let workspace = Workspace::new();
        let root = &workspace.workspace;
        std::fs::write(root.join("package.json"), "{}\n").unwrap();
        std::fs::write(root.join("a.ts"), "export const a = 1;\n").unwrap();
        let marker = root.join("server-started");
        let script = root.join("fake-lsp.sh");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho started > '{}'\nexec cat > /dev/null\n",
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let config = root.join("lsp-servers.json");
        std::fs::write(
            &config,
            json!({"languageServers": {".ts": {
                "command": script.to_string_lossy(), "args": [], "languageId": "typescript"
            }}})
            .to_string(),
        )
        .unwrap();
        let config = config.to_string_lossy().into_owned();
        let mut server = Server::start_with(
            &workspace,
            &[
                ("OCTOCODE_LSP_CONFIG", config.as_str()),
                ("OCTOCODE_LSP_PREWARM", prewarm),
            ],
        );
        let init = server.request(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "test", "version": "1"}}),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "octocode-mcp");
        server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        let deadline = std::time::Instant::now() + Duration::from_secs(if starts { 30 } else { 3 });
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(marker.exists(), starts, "{prewarm}");
        assert_eq!(server.request("ping", json!({}))["result"], json!({}));
        assert!(server.finish().success(), "{prewarm}");
    }
}
