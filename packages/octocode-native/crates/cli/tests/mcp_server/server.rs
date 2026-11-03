//! The `octocode mcp` child process and its JSON-RPC line protocol, shared by
//! the era tests and `progress`.

use crate::support::Workspace;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

pub(super) const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

pub(super) struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    pub(super) replies: Receiver<Value>,
    pub(super) next_id: u64,
}

impl Server {
    pub(super) fn start(workspace: &Workspace) -> Self {
        Self::start_with(workspace, &[])
    }

    /// [`Self::start`] with extra environment variables.
    pub(super) fn start_with(workspace: &Workspace, env: &[(&str, &str)]) -> Self {
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

    pub(super) fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    /// One request and its reply: the full JSON-RPC message.
    pub(super) fn request(&mut self, method: &str, params: Value) -> Value {
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
    pub(super) fn finish(mut self) -> std::process::ExitStatus {
        drop(self.stdin.take());
        self.child.wait().unwrap()
    }
}

pub(super) fn local_search(path: &str) -> Value {
    json!({"name": "localSearch", "arguments": {"queries": [{
        "matchString": ["needle"], "path": path
    }]}})
}

pub(super) fn tool_names(list: &Value) -> Vec<&str> {
    list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect()
}

/// MCP's one text block is exactly the compact JSON CLI `--json` prints.
pub(super) fn assert_cli_json_parity(
    workspace: &Workspace,
    server: &mut Server,
    tool: &str,
    input: Value,
) {
    let call = server.request("tools/call", json!({"name": tool, "arguments": input}));
    let result = &call["result"];
    assert!(result.get("structuredContent").is_none(), "{call}");
    let content = result["content"].as_array().expect("content");
    assert_eq!(content.len(), 1, "{call}");
    assert_eq!(content[0]["type"], "text", "{call}");
    let text = content[0]["text"].as_str().expect("JSON text");
    let cli = workspace
        .cli()
        .args([tool, &input.to_string(), "--json"])
        .output()
        .expect("CLI --json");
    let stdout = String::from_utf8(cli.stdout).expect("UTF-8 CLI stdout");
    assert_eq!(
        text.as_bytes(),
        stdout.strip_suffix('\n').unwrap_or(&stdout).as_bytes(),
        "MCP and CLI differ: {call}"
    );
}
