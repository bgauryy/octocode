//! `octocode mcp` over real stdio, one process per protocol era: the
//! handshake, the listing, a tool call, the protocol error of an unlisted
//! tool and the exit on stdin close.

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
        let mut child = workspace
            .cli()
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
        "mainGoal": "find", "reasoning": "test", "matchString": "needle", "path": path
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
    assert!(result["instructions"].as_str().unwrap().contains("<flow>"));
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let list = server.request("tools/list", json!({}));
    let names = tool_names(&list);
    assert!(names.contains(&"localSearch"), "{names:?}");
    assert!(
        !names.contains(&"run"),
        "no dispatcher without deferred tools"
    );
    let tool = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "localSearch")
        .unwrap();
    assert_eq!(tool["inputSchema"]["type"], "object");
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
