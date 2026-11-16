//! `octocode mcp` on the workspace's warm server, over real stdio: the
//! read-only tools of a persistent-storage session run on `octocode serve`
//! (one server, one language-server pool for MCP and CLI alike), answer the
//! bytes the in-process server answers, keep MCP's tool policy, relay
//! progress and cancellation, and fall back in process when the server dies.
//! Every test owns its socket namespace (`PersistentCli`), which stops the
//! servers it started, by PID.

use super::server::{REPLY_TIMEOUT, Server, local_search};
use crate::support::{PersistentCli, Workspace};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn initialize(server: &mut Server) {
    server.request(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {},
               "clientInfo": {"name": "test", "version": "1"}}),
    );
    server.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
}

fn text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tool text in {reply}"))
}

/// The server sockets in `persistent`'s namespace.
fn server_sockets(persistent: &PersistentCli) -> Vec<PathBuf> {
    std::fs::read_dir(persistent.sockets())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|user| std::fs::read_dir(user.path()).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("serve-") && name.ends_with(".sock"))
        })
        .collect()
}

/// The calls the server on `socket` is running: it answers a request to
/// stop with the count it drains. The server leaves once they finish.
fn in_flight(socket: &std::path::Path) -> u64 {
    use std::io::{BufRead, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(socket).expect("connect");
    stream
        .set_read_timeout(Some(REPLY_TIMEOUT))
        .expect("read timeout");
    stream
        .write_all(b"{\"stop\":true,\"drain\":true}\n")
        .expect("ask to stop");
    for line in std::io::BufReader::new(stream).lines() {
        let frame: Value = serde_json::from_str(&line.expect("a frame")).expect("JSON");
        if let Some(count) = frame["draining"].as_u64() {
            return count;
        }
    }
    panic!("the server never said what it drains");
}

fn kill(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .output();
}

/// A session on persistent storage runs the read-only tools on the warm
/// server (a server appears) and answers the bytes the in-process MCP
/// server answers for the same call.
#[test]
fn a_warm_mcp_call_answers_the_bytes_of_an_in_process_call() {
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("a.txt"), "alpha\nbeta needle\n").unwrap();
    let call = || {
        json!({"name": "localSearch", "arguments": {"queries": [{
            "matchString": ["needle"], "path": "."}]}})
    };
    let mut local = Server::start(&workspace);
    initialize(&mut local);
    let expected = local.request("tools/call", call());
    assert!(local.finish().success());
    assert_eq!(expected["result"]["isError"], false, "{expected}");

    let persistent = workspace.persistent_cli();
    let mut warm = Server::start_persistent(&persistent, &[]);
    initialize(&mut warm);
    let served = warm.request("tools/call", call());
    assert_eq!(
        persistent.server_pids().len(),
        1,
        "the call ran on a warm server"
    );
    assert_eq!(served["result"]["isError"], expected["result"]["isError"]);
    assert_eq!(
        text(&served).as_bytes(),
        text(&expected).as_bytes(),
        "the daemon and the process answer alike"
    );
    assert!(warm.finish().success());
    assert!(persistent.stop(), "the test's server is stopped");
}

/// MCP's tool policy holds on the shared server: `DISABLE_TOOLS` removes a
/// tool from MCP and not from the CLI, and both reach one server.
#[test]
fn mcp_policy_holds_on_the_server_the_cli_shares() {
    let workspace = Workspace::new();
    std::fs::write(workspace.workspace.join("a.txt"), "alpha\nbeta needle\n").unwrap();
    let persistent = workspace.persistent_cli();
    let mut server = Server::start_persistent(&persistent, &[("DISABLE_TOOLS", "localSearch")]);
    initialize(&mut server);
    let blocked = server.request("tools/call", local_search("."));
    assert_eq!(blocked["result"]["isError"], true, "{blocked}");
    assert!(text(&blocked).contains("DISABLE_TOOLS"), "{blocked}");
    assert_eq!(persistent.server_pids().len(), 1, "refused on the server");
    let cli = persistent
        .cli()
        .env("DISABLE_TOOLS", "localSearch")
        .args([
            "localSearch",
            r#"{"queries":[{"matchString":["needle"],"path":"."}]}"#,
            "--json",
        ])
        .output()
        .expect("localSearch");
    assert_eq!(cli.status.code(), Some(0), "{cli:?}");
    assert_eq!(persistent.server_pids().len(), 1, "one server for both");
    assert_eq!(server_sockets(&persistent).len(), 1);
    assert!(server.finish().success());
    assert!(persistent.stop(), "the test's server is stopped");
}

/// A GitHub whose code-search quota is spent for `reset_in`, then answers
/// an empty page; every call after that waits again.
struct SpentThenPage {
    calls: AtomicUsize,
    reset_in: Duration,
}

impl Respond for SpentThenPage {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        if self.calls.fetch_add(1, Ordering::SeqCst) % 2 == 1 {
            return ResponseTemplate::new(200).set_body_json(json!({
                "total_count": 0, "incomplete_results": false, "items": []}));
        }
        let reset = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_secs()
            + self.reset_in.as_secs();
        ResponseTemplate::new(403)
            .insert_header("x-ratelimit-remaining", "0")
            .insert_header("x-ratelimit-reset", reset.to_string().as_str())
            .insert_header("x-ratelimit-resource", "code_search")
            .set_body_json(json!({"message": "API rate limit exceeded"}))
    }
}

async fn github(reset_in: Duration) -> MockServer {
    let github = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(SpentThenPage {
            calls: AtomicUsize::new(0),
            reset_in,
        })
        .mount(&github)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"a/b"})),
        )
        .mount(&github)
        .await;
    github
}

fn search(token: &str) -> Value {
    json!({"name": "ghSearchCode", "_meta": {"progressToken": token},
           "arguments": {"queries": [{"owner": "a", "repo": "b", "keywords": ["needle"]}]}})
}

/// A persistent session whose GitHub is `api`, initialized, with the search
/// sent: the server and the search's id.
fn start_search(persistent: &PersistentCli, api: &str, token: &str) -> (Server, u64) {
    let mut server = Server::start_persistent(
        persistent,
        &[("GITHUB_API_URL", api), ("GITHUB_TOKEN", token)],
    );
    initialize(&mut server);
    server.next_id += 1;
    let id = server.next_id;
    server.send(
        &json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": search("wait")}),
    );
    (server, id)
}

/// The messages a server sends until `done` holds of one, or `None` when it
/// sends none in time.
fn until(server: &Server, done: impl Fn(&Value) -> bool) -> Option<Vec<Value>> {
    let mut heard = Vec::new();
    loop {
        let message = server.replies.recv_timeout(REPLY_TIMEOUT).ok()?;
        let end = done(&message);
        heard.push(message);
        if end {
            return Some(heard);
        }
    }
}

/// A wait the warm server reports reaches the MCP client as progress under
/// its token before the result, and the result is the page waited for.
#[tokio::test(flavor = "multi_thread")]
async fn a_warm_calls_progress_reaches_the_client_before_its_result() {
    let github = github(Duration::from_secs(2)).await;
    let api = format!("{}/api/v3", github.uri());
    let heard = tokio::task::spawn_blocking(move || {
        let workspace = Workspace::new();
        let persistent = workspace.persistent_cli();
        let token = workspace.token.clone();
        let (server, id) = start_search(&persistent, &api, &token);
        let heard = until(&server, |message| message["id"] == id).expect("a result");
        assert_eq!(
            persistent.server_pids().len(),
            1,
            "the call ran on the server"
        );
        assert!(server.finish().success());
        assert!(persistent.stop());
        heard
    })
    .await
    .expect("stdio exchange");
    let progress: Vec<&Value> = heard
        .iter()
        .filter(|message| message["method"] == "notifications/progress")
        .collect();
    assert!(
        !progress.is_empty(),
        "no progress before the result: {heard:?}"
    );
    assert_eq!(progress[0]["params"]["progressToken"], "wait");
    let line = progress[0]["params"]["message"].as_str().expect("a line");
    assert!(
        line.contains("code_search") && line.contains("reset"),
        "{line}"
    );
    let result = heard.last().expect("result");
    assert!(text(result).contains("\"status\":\"empty\""), "{result}");
}

/// `notifications/cancelled` cancels the call on the warm server: nothing
/// is in flight there afterwards, and the search never runs again at the
/// reset it was waiting for.
#[tokio::test(flavor = "multi_thread")]
async fn an_mcp_cancel_cancels_the_call_on_the_server() {
    let reset_in = Duration::from_secs(3);
    let github = github(reset_in).await;
    let api = format!("{}/api/v3", github.uri());
    let left = tokio::task::spawn_blocking(move || {
        let workspace = Workspace::new();
        let persistent = workspace.persistent_cli();
        let token = workspace.token.clone();
        let (mut server, id) = start_search(&persistent, &api, &token);
        until(&server, |message| {
            message["method"] == "notifications/progress"
        })
        .expect("the wait is reported");
        server.send(
            &json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
                    "params": {"requestId": id, "reason": "test"}}),
        );
        // Past the reset, a call still running would have searched again
        // and held the server's connection until then.
        std::thread::sleep(reset_in * 2);
        let sockets = server_sockets(&persistent);
        assert_eq!(sockets.len(), 1);
        let left = in_flight(&sockets[0]);
        assert!(server.finish().success());
        assert!(persistent.stop());
        left
    })
    .await
    .expect("stdio exchange");
    assert_eq!(left, 0, "a call is still in flight on the server");
    let searches = github
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path() == "/api/v3/search/code")
        .count();
    assert_eq!(searches, 1, "the cancelled call searched again");
}

/// A server that dies mid-call (killed by PID) costs the call nothing: it
/// runs in process and answers the page.
#[tokio::test(flavor = "multi_thread")]
async fn a_server_killed_mid_call_falls_back_in_process() {
    let github = github(Duration::from_secs(3)).await;
    let api = format!("{}/api/v3", github.uri());
    let heard = tokio::task::spawn_blocking(move || {
        let workspace = Workspace::new();
        let persistent = workspace.persistent_cli();
        let token = workspace.token.clone();
        let (server, id) = start_search(&persistent, &api, &token);
        until(&server, |message| {
            message["method"] == "notifications/progress"
        })
        .expect("the wait is reported");
        let pids = persistent.server_pids();
        assert_eq!(pids.len(), 1, "the call is on the server");
        kill(pids[0]);
        let heard = until(&server, |message| message["id"] == id);
        assert!(server.finish().success());
        assert!(persistent.stop());
        heard
    })
    .await
    .expect("stdio exchange");
    let heard = heard.expect("the call is answered after the server died");
    let result = heard.last().expect("result");
    assert_eq!(result["result"]["isError"], false, "{result}");
    assert!(text(result).contains("\"status\":\"empty\""), "{result}");
}

/// With `lsp.prewarm` `all` the session's workspace is warmed by the warm
/// server: the language server it starts is the server's child, before any
/// call names a file.
#[test]
fn the_workspace_prewarm_runs_on_the_warm_server() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    let root = workspace.workspace.clone();
    std::fs::write(root.join("package.json"), "{}\n").unwrap();
    std::fs::write(root.join("a.ts"), "export function target() {}\n").unwrap();
    let marker = root.join("server-started");
    let script = root.join("fake-lsp.sh");
    // The marker holds the language server's parent: the process that
    // started it.
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho $PPID > '{}'\nexec cat > /dev/null\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        workspace.home.join("lsp-servers.json"),
        json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": "typescript"
        }}})
        .to_string(),
    )
    .unwrap();
    let persistent = workspace.persistent_cli();
    let mut server = Server::start_persistent(&persistent, &[("OCTOCODE_LSP_PREWARM", "all")]);
    initialize(&mut server);
    let began = std::time::Instant::now();
    while !marker.exists() && began.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let parent = std::fs::read_to_string(&marker).expect("the language server started");
    let pids = persistent.server_pids();
    assert_eq!(pids.len(), 1, "a warm server exists");
    assert_eq!(
        parent.trim(),
        pids[0].to_string(),
        "the warm server started the language server"
    );
    assert!(server.finish().success());
    assert!(persistent.stop(), "the test's server is stopped");
}

/// A language-server script that records each start (its pid) and answers
/// `definition` with a declaration in `a.ts`.
fn recording_server(pids: &std::path::Path) -> String {
    format!(
        r#"#!/usr/bin/env node
const fs = require('fs');
fs.appendFileSync('{}', process.pid + '\n');
let buf = Buffer.alloc(0);
function send(m) {{
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}}
function handle(msg) {{
  if (msg.method === 'initialize') {{
    send({{ jsonrpc: '2.0', id: msg.id, result: {{ capabilities: {{ definitionProvider: true, textDocumentSync: 1 }} }} }});
  }} else if (msg.method === 'exit') {{
    process.exit(0);
  }} else if (msg.id !== undefined && msg.method) {{
    const uri = msg.params && msg.params.textDocument && msg.params.textDocument.uri;
    const at = {{ start: {{ line: 0, character: 16 }}, end: {{ line: 0, character: 22 }} }};
    send({{ jsonrpc: '2.0', id: msg.id, result: msg.method === 'textDocument/definition' ? [{{ uri, range: at }}] : null }});
  }}
}}
process.stdin.on('data', (d) => {{
  buf = Buffer.concat([buf, d]);
  for (;;) {{
    const i = buf.indexOf('\r\n\r\n');
    if (i < 0) return;
    const m = /Content-Length: (\d+)/i.exec(buf.slice(0, i).toString());
    const n = Number(m[1]);
    if (buf.length < i + 4 + n) return;
    const msg = JSON.parse(buf.slice(i + 4, i + 4 + n).toString());
    buf = buf.slice(i + 4 + n);
    handle(msg);
  }}
}});
"#,
        pids.display()
    )
}

/// An MCP session and a CLI call in one workspace share one language
/// server: the pool belongs to the warm server both reach.
#[test]
fn an_mcp_session_and_the_cli_share_one_language_server() {
    use std::os::unix::fs::PermissionsExt;
    let workspace = Workspace::new();
    let root = workspace.workspace.clone();
    std::fs::write(root.join("package.json"), "{}\n").unwrap();
    std::fs::write(root.join("a.ts"), "export function target() {}\n").unwrap();
    let pids = root.join("lsp-pids");
    let script = root.join("fake-lsp.js");
    std::fs::write(&script, recording_server(&pids)).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        workspace.home.join("lsp-servers.json"),
        json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": "typescript"
        }}})
        .to_string(),
    )
    .unwrap();
    let query = json!({"queries": [{
        "operation": "definition", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1}]});
    // The fixture's `PATH` has no `node`, which the script runs on; both
    // clients get one `PATH`, so they share a server.
    let node = std::env::split_paths(&std::env::var_os("PATH").expect("PATH"))
        .find(|dir| dir.join("node").is_file())
        .expect("node on the test's PATH");
    let path = format!("/usr/bin:/bin:{}", node.display());
    let persistent = workspace.persistent_cli();
    let mut server = Server::start_persistent(&persistent, &[("PATH", &path)]);
    initialize(&mut server);
    let served = server.request(
        "tools/call",
        json!({"name": "lspSearch", "arguments": query}),
    );
    assert_eq!(served["result"]["isError"], false, "{served}");
    let cli = persistent
        .cli()
        .env("PATH", &path)
        .args(["lspSearch", &query.to_string(), "--json"])
        .output()
        .expect("lspSearch");
    assert_eq!(cli.status.code(), Some(0), "{cli:?}");
    let started = std::fs::read_to_string(&pids).expect("a language server started");
    assert_eq!(started.lines().count(), 1, "one start for both: {started}");
    assert!(server.finish().success());
    assert!(persistent.stop(), "the test's server is stopped");
}
