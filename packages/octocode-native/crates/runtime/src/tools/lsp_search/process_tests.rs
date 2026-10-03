//! Process-level fixtures: a real child language server (a Node script) that
//! crashes mid-request, and an acquire/stop loop that must leave no child
//! processes behind. Unix-only (process groups, `kill -0`).

use super::failure::LspFailure;
use octocode_engine::error::ErrorKind;
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::pool::{LspClientPool, LspPoolOptions};
use octocode_engine::lsp::types::JsLanguageServerConfig;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Answers `initialize`; exits without replying to `textDocument/hover`
/// (crash mid-request). Records its own pid and a grandchild's pid.
const FAKE_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
const { spawn } = require('child_process');
const pids = process.env.FAKE_LSP_PIDS;
if (pids) {
  const child = spawn('sleep', ['30'], { stdio: 'ignore' });
  fs.appendFileSync(pids, process.pid + '\n' + child.pid + '\n');
}
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'textDocument/hover') {
    process.exit(3);
  } else if (msg.method === 'exit') {
    process.exit(0);
  } else if (msg.id !== undefined && msg.method) {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
  }
}
process.stdin.on('data', (d) => {
  buf = Buffer.concat([buf, d]);
  for (;;) {
    const i = buf.indexOf('\r\n\r\n');
    if (i < 0) return;
    const m = /Content-Length: (\d+)/i.exec(buf.slice(0, i).toString());
    const n = Number(m[1]);
    if (buf.length < i + 4 + n) return;
    const msg = JSON.parse(buf.slice(i + 4, i + 4 + n).toString());
    buf = buf.slice(i + 4 + n);
    handle(msg);
  }
});
"#;

fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn fixture(tag: &str) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("octocode-lsp-proc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let root = root.canonicalize().expect("canonical root");
    let script = root.join("fake-lsp.js");
    std::fs::write(&script, FAKE_SERVER).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::fs::write(root.join("a.ts"), "const a = 1;\n").expect("source");
    (root, script)
}

fn config(root: &Path, script: &Path, pids: Option<&Path>) -> JsLanguageServerConfig {
    JsLanguageServerConfig {
        command: script.to_string_lossy().into_owned(),
        args: Some(Vec::new()),
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("plaintext".into()),
        initialization_options: None,
        env: pids.map(|pids| {
            [(
                "FAKE_LSP_PIDS".to_owned(),
                pids.to_string_lossy().into_owned(),
            )]
            .into_iter()
            .collect()
        }),
        max_memory_mb: None,
    }
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// A server that dies mid-request fails the pending request fast with a
/// typed `ConnectionClosed` (`lsp.serverCrashed`), not a timeout.
#[test]
fn server_crash_mid_request_fails_pending_fast_as_server_crashed() {
    if !node_available() {
        eprintln!("skipping: node not available");
        return;
    }
    let (root, script) = fixture("crash");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("fake server starts");
        let started = Instant::now();
        let error = tokio::time::timeout(
            Duration::from_secs(10),
            client.get_hover(root.join("a.ts").to_string_lossy().into_owned(), 0, 6),
        )
        .await
        .expect("pending request settles well before the request timeout")
        .expect_err("server crashed mid-request");
        assert!(
            matches!(error.kind(), ErrorKind::ConnectionClosed),
            "{error:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(LspFailure::from_engine(&error).code, "lsp.serverCrashed");
        let _ = client.stop().await;
    });
    let _ = std::fs::remove_dir_all(root);
}

/// Repeated acquire/stop through the pool leaves no server or grandchild
/// process running.
#[test]
fn acquire_stop_loop_leaves_no_child_processes() {
    if !node_available() {
        eprintln!("skipping: node not available");
        return;
    }
    let (root, script) = fixture("leak");
    let pids = root.join("pids.txt");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = LspClientPool::new(LspPoolOptions {
            idle_timeout_ms: 60_000,
            max_entries: 4,
        });
        let config = config(&root, &script, Some(&pids));
        for _ in 0..5 {
            let (client, lease) = pool
                .acquire_leased(config.clone())
                .await
                .expect("acquire")
                .expect("client");
            assert!(client.is_alive().await);
            drop(lease);
            assert!(pool.clear(&config).await.expect("clear"));
        }
        assert!(pool.is_empty());
    });
    let recorded = std::fs::read_to_string(&pids).expect("pid log");
    let recorded = recorded.lines().collect::<Vec<_>>();
    assert_eq!(recorded.len(), 10, "5 servers × (server + grandchild)");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let survivors = recorded.iter().filter(|pid| alive(pid)).collect::<Vec<_>>();
        if survivors.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "leaked processes: {survivors:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = std::fs::remove_dir_all(root);
}

/// Linux applies the default 4 GiB `RLIMIT_AS` at spawn: a V8 (Node) server
/// must still start and answer under it.
#[cfg(target_os = "linux")]
#[test]
fn node_server_starts_under_the_default_address_space_cap() {
    if !node_available() {
        eprintln!("skipping: node not available");
        return;
    }
    let (root, script) = fixture("rlimit");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client
            .start()
            .await
            .expect("node initializes under RLIMIT_AS 4 GiB");
        assert!(client.is_alive().await);
        let _ = client.stop().await;
    });
    let _ = std::fs::remove_dir_all(root);
}

/// A real provider answers the first definition with an alias location, then
/// fails the follow-up. Useful locations survive, but identity stays partial.
#[test]
fn nested_definition_failure_keeps_location_with_partial_provenance() {
    use super::ops::Operation;
    use super::source::{SourceCache, snippet_policy};
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    use crate::tools::cancel::NeverCancel;
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    for fail in [true, false] {
        let (root, script) = fixture(if fail {
            "definition-error"
        } else {
            "definition-ok"
        });
        std::fs::write(root.join("b.ts"), "const b = 1;\n").expect("alias source");
        let server = FAKE_SERVER
            .replace("hoverProvider: true", "definitionProvider: true")
            .replace(
                "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
                &format!(r#"  }} else if (msg.method === 'textDocument/definition') {{
    if (msg.params.textDocument.uri.endsWith('/a.ts')) {{
      send({{jsonrpc:'2.0',id:msg.id,result:{{uri:msg.params.textDocument.uri.replace('/a.ts','/b.ts'),range:{{start:{{line:0,character:6}},end:{{line:0,character:7}}}}}}}});
    }} else if ({fail}) {{
      send({{jsonrpc:'2.0',id:msg.id,error:{{code:-32603,message:'injected follow-up failure'}}}});
    }} else {{
      send({{jsonrpc:'2.0',id:msg.id,result:[]}});
    }}"#),
            );
        std::fs::write(&script, server).expect("definition fixture server");
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.clone()),
            ..Default::default()
        })
        .expect("path policy");
        let path = root.join("a.ts").to_string_lossy().into_owned();
        let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
        let query = serde_json::from_value(json!({
            "operation":"definition", "mainGoal":"test", "reasoning":"test",
            "uri":uri, "position":{"line":0,"character":6}
        }))
        .expect("query");
        tokio::runtime::Runtime::new().expect("rt").block_on(async {
            let client = NativeLspClient::new(config(&root, &script, None));
            client.start().await.expect("server starts");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let row = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                workspace_root: root.to_str().expect("root"),
                root_only: false,
                line: 0,
                character: 6,
                language_id: Some("typescript"),
            }
            .run()
            .await
            .expect("definition row retains earlier evidence");
            let text = row.to_string();
            assert!(text.contains("b.ts"), "{row}");
            if fail {
                assert_eq!(row["isPartial"], true, "{row}");
                assert!(
                    row["partialReasons"]
                        .as_array()
                        .expect("reasons")
                        .contains(&json!("definitionHopFailed")),
                    "{row}"
                );
                assert!(text.contains("injected follow-up failure"), "{row}");
                assert!(text.contains("not verified terminal identity"), "{row}");
                assert_eq!(row["next"]["retry"]["tool"], "lspSearch");
            } else {
                assert!(
                    !row["partialReasons"]
                        .as_array()
                        .is_some_and(|reasons| reasons.contains(&json!("definitionHopFailed"))),
                    "{row}"
                );
                assert!(!text.contains("injected follow-up failure"), "{row}");
            }
            client.stop().await.expect("fixture server closed");
        });
        std::fs::remove_dir_all(root).expect("fixture removed");
    }
}
