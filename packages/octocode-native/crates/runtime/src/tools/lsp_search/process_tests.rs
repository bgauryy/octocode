//! Process-level fixtures: a real child language server (a Node script) that
//! crashes mid-request, and an acquire/stop loop that must leave no child
//! processes behind. Unix-only (process groups, `kill -0`).

use super::failure::LspFailure;
use super::tests::{FAKE_SERVER_CLIENT, fake_server_pool};
use crate::id::ToolId;
use crate::tools::lsp_search::lsp_search_query;
use crate::tools::test_support::checks::{Page, Walker, assert_absent, paths_in};
use crate::tools::test_support::{Fixture, assert_row_contract, node_available, require_node};
use octocode_engine::error::ErrorKind;
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::pool::{LspClientPool, LspPoolOptions};
use octocode_engine::lsp::types::LspLanguageServerConfig;
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

/// A canonical workspace holding the fake server script and `a.ts`; the
/// directory goes when the [`Fixture`] drops.
fn fixture() -> (Fixture, PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = Fixture::new();
    let root = dir.0.canonicalize().expect("canonical root");
    let script = root.join("fake-lsp.js");
    std::fs::write(&script, FAKE_SERVER).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::fs::write(root.join("a.ts"), "const a = 1;\n").expect("source");
    (dir, root, script)
}

fn config(root: &Path, script: &Path, pids: Option<&Path>) -> LspLanguageServerConfig {
    LspLanguageServerConfig {
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
        source: Default::default(),
    }
}

fn execution_config(root: &Path, script: &Path, language_id: &str) -> super::LspExecutionConfig {
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": language_id
        }}})
        .to_string(),
    )
    .expect("config");
    super::LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..super::LspExecutionConfig::default()
    }
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Announced startup indexing is a live, incomplete answer with an exact
/// retry. The retry reuses the same server after its progress ends; a request
/// during the first response would crash this fixture instead.
#[test]
fn initial_indexing_returns_partial_then_retries_on_the_same_server() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let server = FAKE_SERVER
        .replace(
            "const pids = process.env.FAKE_LSP_PIDS;",
            "const pids = __dirname + '/pids.txt';\nconst gate = __dirname + '/indexing-ready';",
        )
        .replace(
            "let buf = Buffer.alloc(0);",
            "let indexingDone = false;\nlet buf = Buffer.alloc(0);",
        )
        .replace(
            "    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });",
            "    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });\n    send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'index', value: { kind: 'begin', title: 'indexing' } } });\n    const waitForGate = setInterval(() => {\n      if (!fs.existsSync(gate)) return;\n      clearInterval(waitForGate);\n      indexingDone = true;\n      send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'index', value: { kind: 'end', message: 'ready' } } });\n    }, 10);",
        )
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            "  } else if (msg.method === 'textDocument/hover') {\n    if (!indexingDone) process.exit(3);\n    fs.appendFileSync(pids, 'hover\\n');\n    send({ jsonrpc: '2.0', id: msg.id, result: { contents: 'ready' } });",
        );
    assert_ne!(server, FAKE_SERVER, "the indexing fixture is injected");
    std::fs::write(&script, server).expect("indexing server");

    let execution = execution_config(&root, &script, "shellscript");
    // The startup settle is the window in which the client reads the
    // server's `begin` progress; a zero cap races the hover past it.
    let pool = LspClientPool::new(LspPoolOptions {
        client: octocode_engine::lsp::client::LspClientOptions {
            settle_cap: Some(std::time::Duration::from_secs(1)),
        },
        ..LspPoolOptions::default()
    });
    let query_row = serde_json::json!({
        "operation": "hover", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "a", "line": 1
    });
    let query: super::LspSearchQuery = serde_json::from_value(query_row.clone()).expect("query");
    let expected_retry = lsp_search_query::to_row(&query);
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let first = runtime
        .block_on(super::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
        ))
        .expect("indexing row");
    assert_row_contract(ToolId::LspSearch, &query_row, &first);
    assert_eq!(first["status"], "empty", "{first}");
    assert_eq!(first["category"], "languageServerIndexing", "{first}");
    assert!(
        first["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("still indexing")),
        "{first}"
    );
    assert_eq!(first["lsp"]["serverAvailable"], true, "{first}");
    assert_eq!(first["isPartial"], true, "{first}");
    let retry = first["next"]["retry"]["query"]["queries"][0].clone();
    assert_eq!(retry, expected_retry, "{first}");
    let pids = std::fs::read_to_string(root.join("pids.txt")).expect("server pid");
    assert_eq!(
        pids.lines().count(),
        2,
        "no semantic request before retry: {pids}"
    );
    std::fs::write(root.join("indexing-ready"), "").expect("indexing gate");

    let retried = serde_json::from_value(retry).expect("retry query");
    let second = runtime
        .block_on(super::execute(
            retried,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
        ))
        .expect("retry row");
    assert_row_contract(ToolId::LspSearch, &query_row, &second);
    assert_eq!(second["kind"], "hover", "{second}");
    assert_eq!(second["hover"]["contents"], "ready", "{second}");
    let pids = std::fs::read_to_string(root.join("pids.txt")).expect("server pid");
    assert_eq!(
        pids.lines().count(),
        3,
        "one server and one retry request: {pids}"
    );
    runtime.block_on(pool.clear_all());
}

/// A server that dies mid-request fails the pending request fast with a
/// typed `ConnectionClosed` (`lsp.serverCrashed`), not a timeout.
#[test]
fn server_crash_mid_request_fails_pending_fast_as_server_crashed() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
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
        assert_eq!(LspFailure::from_engine(&error).code, "serverCrashed");
        let _ = client.stop().await;
    });
}

/// Repeated acquire/stop through the pool leaves no server or grandchild
/// process running.
#[test]
fn acquire_stop_loop_leaves_no_child_processes() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let pids = root.join("pids.txt");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = LspClientPool::new(LspPoolOptions {
            idle_timeout_ms: 60_000,
            max_entries: 4,
            max_resident_bytes: 0,
            client: FAKE_SERVER_CLIENT,
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
}

/// A prewarm whose file is over the open cap (or unreadable) warmed
/// only the server, so it reports not-warm and frees its key; a small file
/// is opened and the warm completes.
#[test]
fn prewarm_frees_its_key_when_the_file_is_not_opened() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let large = root.join("large.ts");
    std::fs::write(&large, "x".repeat(2 * 1024 * 1024 + 1)).expect("large source");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = fake_server_pool();
        let config = config(&root, &script, None);
        let file = |path: PathBuf| path.to_string_lossy().into_owned();
        assert!(
            !super::prewarm::warm(&pool, config.clone(), file(large.clone())).await,
            "an over-cap file is not a completed warm"
        );
        assert!(
            !super::prewarm::warm(&pool, config.clone(), file(root.join("missing.ts"))).await,
            "an unreadable file is not a completed warm"
        );
        assert!(super::prewarm::warm(&pool, config.clone(), file(root.join("a.ts"))).await);
        assert_eq!(pool.len(), 1, "every warm reused one pooled server");
        pool.clear_all().await;
    });
}

/// A V8 (Node) server reserves address space far past its resident use; it
/// must start and answer under the default Linux memory policy (RSS watchdog,
/// no address-space cap).
#[cfg(target_os = "linux")]
#[test]
fn node_server_starts_under_the_default_address_space_cap() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client
            .start()
            .await
            .expect("node initializes under the default memory policy");
        assert!(client.is_alive().await);
        let _ = client.stop().await;
    });
}

/// A real provider answers the first definition with an alias location, then
/// fails the follow-up. Useful locations survive, but identity stays partial.
#[test]
fn nested_definition_failure_keeps_location_with_partial_provenance() {
    use super::ops::Operation;
    use super::source::{SourceCache, snippet_policy};
    use crate::execution::cancel::NeverCancel;
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    for fail in [true, false] {
        let (_dir, root, script) = fixture();
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
        let paths = crate::tools::test_support::workspace_policy(&root);
        let path = root.join("a.ts").to_string_lossy().into_owned();
        let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
        let query = serde_json::from_value(json!({
            "operation":"definition", "mainGoal":"test", "reasoning":"test",
            "path":uri, "symbolName":"x", "line":1
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
                scope: &super::scope::Scope::new(root.to_string_lossy().into_owned(), Vec::new()),
                root_only: false,
                line: 0,
                character: 6,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await
            .expect("definition row retains earlier evidence");
            assert!(lists_path(&row, "b.ts"), "{row}");
            if fail {
                assert_eq!(row["isPartial"], true, "{row}");
                assert!(
                    row["partialReasons"]
                        .as_array()
                        .expect("reasons")
                        .contains(&json!("definitionHopFailed")),
                    "{row}"
                );
                // The server's failure reaches the caller in a warning.
                assert!(
                    row["warnings"].as_array().is_some_and(|warnings| warnings
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .any(|warning| warning.contains("injected follow-up failure"))),
                    "{row}"
                );
                assert_eq!(row["next"]["retry"]["tool"], "lspSearch");
            } else {
                assert!(
                    !row["partialReasons"]
                        .as_array()
                        .is_some_and(|reasons| reasons.contains(&json!("definitionHopFailed"))),
                    "{row}"
                );
                assert_absent(&row, "injected follow-up failure");
            }
            client.stop().await.expect("fixture server closed");
        });
    }
}

/// A server with no `typeHierarchyProvider` still answers supertypes and
/// subtypes: subtypes are its implementation answer (never the queried
/// declaration itself), supertypes are the definitions of the heritage
/// clause, and each row names its source.
#[test]
fn type_hierarchy_falls_back_when_the_server_has_no_type_hierarchy() {
    use super::ops::Operation;
    use super::source::{SourceCache, snippet_policy};
    use crate::execution::cancel::NeverCancel;
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(root.join("b.ts"), "export interface Base {}\n").expect("base");
    std::fs::write(root.join("a.ts"), "export class Child extends Base {}\n").expect("child");
    let server = FAKE_SERVER
        .replace(
            "hoverProvider: true",
            "implementationProvider: true, definitionProvider: true",
        )
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/implementation') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    const at = (name, character, length) => ({ uri: dir + name, range: { start: { line: 0, character }, end: { line: 0, character: character + length } } });
    send({ jsonrpc: '2.0', id: msg.id, result: [at('a.ts', 13, 5), at('b.ts', 17, 4)] });
  } else if (msg.method === 'textDocument/definition') {
    const dir = msg.params.textDocument.uri.slice(0, msg.params.textDocument.uri.lastIndexOf('/') + 1);
    send({ jsonrpc: '2.0', id: msg.id, result: [{ uri: dir + 'b.ts', range: { start: { line: 0, character: 17 }, end: { line: 0, character: 21 } } }] });"#,
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        for (operation, source, file, symbol, character, expected) in [
            ("subtypes", "implementation", "b.ts", "Base", 17, "a.ts"),
            ("supertypes", "heritage", "a.ts", "Child", 13, "b.ts"),
        ] {
            let path = root.join(file).to_string_lossy().into_owned();
            let query = serde_json::from_value(json!({
                "operation":operation, "mainGoal":"test", "reasoning":"test",
                "path":path, "symbolName":symbol, "line":1
            }))
            .expect("query");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let row = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &super::scope::Scope::new(root.to_string_lossy().into_owned(), Vec::new()),
                root_only: false,
                line: 0,
                character,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await
            .unwrap_or_else(|error| panic!("{operation} fails without a hierarchy: {error:?}"));
            assert_eq!(row["lsp"]["source"], source, "{row}");
            let paths = row["matches"]
                .as_array()
                .expect("matches")
                .iter()
                .map(|found| found["path"].as_str().unwrap_or_default().to_owned())
                .collect::<Vec<_>>();
            assert!(
                paths.iter().all(|found| found.ends_with(expected)),
                "{operation} lists only other declarations: {row}"
            );
            assert_eq!(paths.len(), 1, "{row}");
            assert!(row.get("isPartial").is_none(), "{row}");
        }
        client.stop().await.expect("fixture server closed");
    });
}

/// When the implementation request cannot be answered (here a server error;
/// a timeout takes the same path), `subtypes` is the heritage-clause scan
/// of every workspace file spelling the base: labelled rows, never a bare
/// failure.
#[test]
fn subtypes_fall_back_to_a_labelled_heritage_scan_when_implementation_fails() {
    use super::ops::Operation;
    use super::source::{SourceCache, snippet_policy};
    use crate::execution::cancel::NeverCancel;
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(root.join("b.ts"), "export interface Base {}\n").expect("base");
    std::fs::write(root.join("a.ts"), "export class Child extends Base {}\n").expect("child");
    std::fs::write(
        root.join("c.ts"),
        "export interface Wide extends Other, Base {}\n",
    )
    .expect("wide");
    std::fs::write(root.join("d.ts"), "export class Skip extends Other {}\n").expect("other");
    std::fs::write(root.join("e.ts"), "// Base\nconst x: Base = {};\n").expect("mention");
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "implementationProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/implementation') {
    send({ jsonrpc: '2.0', id: msg.id, error: { code: -32603, message: 'Debug Failure' } });"#,
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("b.ts").to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        let query = serde_json::from_value(json!({
            "operation":"subtypes", "mainGoal":"test", "reasoning":"test",
            "path":uri, "symbolName":"Base", "line":1
        }))
        .expect("query");
        let mut sources = SourceCache::new(&paths);
        let policy = snippet_policy(&paths);
        let row = Operation {
            client: &client,
            query: &query,
            sources: &mut sources,
            snippet_policy: &policy,
            cancel: &NeverCancel,
            path: &path,
            scope: &super::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                vec!["*.ts".into()],
            ),
            root_only: false,
            line: 0,
            character: 17,
            language_id: Some("typescript"),
            budget: 16_000,
            response_window: None,
        }
        .run()
        .await
        .unwrap_or_else(|error| panic!("a failed implementation request falls back: {error:?}"));
        assert_eq!(row["lsp"]["source"], "native-graph-facts", "{row}");
        let found = row["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|found| {
                Path::new(found["path"].as_str().unwrap_or_default())
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        assert_eq!(found, ["a.ts", "c.ts"], "{row}");
        assert!(
            row["warnings"].to_string().contains("unverified"),
            "the rows are labelled unverified: {row}"
        );
        let query = serde_json::to_value(&query).expect("query json");
        assert_row_contract(ToolId::LspSearch, &query, &row);
        // The scan is a labelled stand-in: the same query, retried, asks the
        // server again.
        assert_eq!(row["next"]["retry"]["tool"], "lspSearch", "{row}");
        assert_eq!(row["isPartial"], true, "{row}");
        client.stop().await.expect("fixture server closed");
    });
}

/// Runs `operation` on `file`'s symbol (zero-based `line`/`character`)
/// against a server whose `textDocument/implementation` fails, over a
/// workspace where `Child` extends `Base` and `Impl` implements it.
fn implementation_failing_row(
    operation: &str,
    (file, symbol, line, character): (&str, &str, u32, u32),
) -> Result<serde_json::Value, super::LspFailure> {
    use super::ops::Operation;
    use super::source::{SourceCache, snippet_policy};
    use crate::execution::cancel::NeverCancel;
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(
        root.join("b.ts"),
        "export interface Base {\n  run(): void;\n}\nexport class Plain {\n  go(): void {}\n}\n",
    )
    .expect("base");
    std::fs::write(root.join("a.ts"), "export class Child extends Base {}\n").expect("child");
    std::fs::write(
        root.join("c.ts"),
        "export class Impl implements Other, Base { run(): void {} }\n",
    )
    .expect("impl");
    std::fs::write(root.join("e.ts"), "// Base\nconst x: Base = {};\n").expect("mention");
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "implementationProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/implementation') {
    send({ jsonrpc: '2.0', id: msg.id, error: { code: -32603, message: 'Debug Failure' } });"#,
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join(file).to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        let query = serde_json::from_value(json!({
            "operation": operation, "mainGoal": "test", "reasoning": "test",
            "path": uri, "symbolName": symbol, "line": line + 1
        }))
        .expect("query");
        let mut sources = SourceCache::new(&paths);
        let policy = snippet_policy(&paths);
        let outcome = Operation {
            client: &client,
            query: &query,
            sources: &mut sources,
            snippet_policy: &policy,
            cancel: &NeverCancel,
            path: &path,
            scope: &super::scope::Scope::new(
                root.to_string_lossy().into_owned(),
                vec!["*.ts".into()],
            ),
            root_only: false,
            line,
            character,
            language_id: Some("typescript"),
            budget: 16_000,
            response_window: None,
        }
        .run()
        .await;
        client.stop().await.expect("fixture server closed");
        outcome
    })
}

/// `implementation` on a type is answered by the same labelled heritage
/// scan as `subtypes` when the server cannot answer in time (a failed
/// request stands in for the timeout): rows by name, labelled unverified,
/// with a retry that asks the server again.
#[test]
fn implementation_on_a_type_falls_back_to_the_labelled_heritage_scan() {
    let row = implementation_failing_row("implementation", ("b.ts", "Base", 0, 17))
        .unwrap_or_else(|error| panic!("a type anchor falls back: {error:?}"));
    assert_eq!(row["lsp"]["source"], "native-graph-facts", "{row}");
    assert_eq!(row["kind"], "implementation", "{row}");
    let files = row["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .map(|found| {
            Path::new(found["path"].as_str().unwrap_or_default())
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>();
    assert_eq!(files, ["a.ts", "c.ts"], "{row}");
    let warnings = row["warnings"].to_string();
    assert!(warnings.contains("unverified"), "{row}");
    assert!(
        warnings.contains("implementations") && !warnings.contains("subtypes are"),
        "the note speaks of the operation asked: {row}"
    );
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(row["next"]["retry"]["tool"], "lspSearch", "{row}");
    assert_eq!(
        row["next"]["retry"]["query"]["queries"][0]["operation"], "implementation",
        "{row}"
    );
    let query = serde_json::json!({"operation": "implementation", "path": "b.ts", "symbolName": "Base", "line": 1});
    assert_row_contract(ToolId::LspSearch, &query, &row);
}

/// A member has no heritage clause to scan: `implementation` on it keeps
/// the server failure (a scan would claim "no implementations").
#[test]
fn implementation_on_a_member_keeps_the_server_failure() {
    let failure = implementation_failing_row("implementation", ("b.ts", "run", 1, 2))
        .expect_err("no scan can answer a member");
    assert_eq!(failure.code, "requestFailed");
    assert!(!failure.hint.contains("workspaceRoot"), "{}", failure.hint);
}

/// A TS-like server that logs every request it answers: references from
/// the declaration miss `c.ts` (importer recovery finds it), and every
/// definition resolves to the declaration in `a.ts`.
const REUSE_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
const log = process.env.FAKE_LSP_LOG;
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function at(uri, line, start, end) {
  return { uri, range: { start: { line, character: start }, end: { line, character: end } } };
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { referencesProvider: true, definitionProvider: true, textDocumentSync: 1 } } });
    return;
  }
  if (msg.method === 'exit') process.exit(0);
  if (msg.id === undefined || !msg.method) return;
  fs.appendFileSync(log, msg.method + '\n');
  const uri = msg.params && msg.params.textDocument && msg.params.textDocument.uri;
  const dir = uri ? uri.slice(0, uri.lastIndexOf('/') + 1) : '';
  const decl = at(dir + 'a.ts', 0, 16, 19);
  if (msg.method === 'textDocument/definition') {
    send({ jsonrpc: '2.0', id: msg.id, result: [decl] });
  } else if (msg.method === 'textDocument/references') {
    const found = [decl, at(dir + 'b.ts', 1, 0, 3), at(dir + 'b.ts', 2, 0, 3)];
    if (uri.endsWith('/c.ts')) found.push(at(dir + 'c.ts', 1, 0, 3));
    send({ jsonrpc: '2.0', id: msg.id, result: found });
  } else {
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

/// A warm repeat of an incoming TS operation under an unchanged response
/// generation asks the server nothing, authorizes and reads each snippet
/// file once per request (however many responses name it), and answers
/// exactly what the cold call did.
#[test]
fn warm_references_repeat_sends_no_requests_and_reads_each_file_once() {
    use super::ops::Operation;
    use super::source::SourceCache;
    use crate::execution::cancel::NeverCancel;
    use octocode_engine::lsp::client::{RESPONSE_SCOPE, ResponseScope, SnippetReadPolicy};
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(&script, REUSE_SERVER).expect("server");
    std::fs::write(root.join("a.ts"), "export function foo() {}\n").expect("a");
    std::fs::write(
        root.join("b.ts"),
        "import { foo } from './a';\nfoo();\nfoo();\n",
    )
    .expect("b");
    std::fs::write(root.join("c.ts"), "import { foo } from './a';\nfoo();\n").expect("c");
    let log = root.join("requests.log");
    let mut server = config(&root, &script, None);
    server.language_id = Some("typescript".into());
    server.env = Some(
        [(
            "FAKE_LSP_LOG".to_owned(),
            log.to_string_lossy().into_owned(),
        )]
        .into_iter()
        .collect(),
    );
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
    let query = serde_json::from_value(json!({
        "operation":"references", "mainGoal":"test", "reasoning":"test",
        "path":uri, "symbolName":"foo", "line":1
    }))
    .expect("query");
    let requests = || {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .count()
    };
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(server);
        client.start().await.expect("server starts");
        let authorized = Arc::new(AtomicUsize::new(0));
        // One request: its own snippet policy, canonical memo, and scope;
        // the response generation (anchor + scope fingerprint) is unchanged.
        let request = || {
            let (client, paths, query, path, root) = (&client, &paths, &query, &path, &root);
            let authorized = Arc::clone(&authorized);
            let generation = ResponseScope {
                reuse: true,
                generation: "anchor+fingerprint".into(),
            };
            RESPONSE_SCOPE.scope(
                std::cell::RefCell::new(generation),
                super::scope::with_canonical_memo(async move {
                    let policy = {
                        let paths = paths.clone();
                        SnippetReadPolicy::with_authorizer(move |file| {
                            authorized.fetch_add(1, Ordering::SeqCst);
                            paths.validate_read(file).ok().map(|valid| valid.canonical)
                        })
                        .shared_reads()
                    };
                    let scope = super::scope::Scope::new(
                        root.to_string_lossy().into_owned(),
                        vec!["*.ts".into()],
                    );
                    scope.set_fingerprint(format!("warm-reuse-{}", std::process::id()));
                    let mut sources = SourceCache::new(paths);
                    Operation {
                        client,
                        query,
                        sources: &mut sources,
                        snippet_policy: &policy,
                        cancel: &NeverCancel,
                        path,
                        scope: &scope,
                        root_only: false,
                        line: 0,
                        character: 16,
                        language_id: Some("typescript"),
                        budget: 16_000,
                        response_window: None,
                    }
                    .run()
                    .await
                    .expect("references row")
                }),
            )
        };
        let cold = request().await;
        let cold_requests = requests();
        let cold_reads = authorized.swap(0, Ordering::SeqCst);
        assert!(lists_path(&cold, "c.ts"), "importer recovered: {cold}");
        assert!(cold_requests > 0);
        // a.ts, b.ts, c.ts: once each, though several responses name them.
        assert_eq!(cold_reads, 3, "{cold}");
        let warm = request().await;
        assert_eq!(warm, cold);
        assert_eq!(
            requests(),
            cold_requests,
            "the warm repeat asked the server"
        );
        assert_eq!(authorized.load(Ordering::SeqCst), 3);
        client.stop().await.expect("fixture server closed");
    });
}

/// A TypeScript-like server whose project sees only the anchor file (an
/// inferred project): `references` from the anchor lists the declaration
/// alone; from an importer it lists the importer's own two sites plus a
/// shared site in `f000.ts`. Every position resolves to the declaration.
const IMPORTER_SERVER: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const declUri = 'file://' + root + '/a.ts';
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const decl = { uri: declUri, range: at(0, 16, 6) };
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function result(msg) {
  const p = msg.params || {};
  switch (msg.method) {
    case 'initialize':
      return { capabilities: { textDocumentSync: 1, definitionProvider: true, referencesProvider: true, callHierarchyProvider: true, documentSymbolProvider: true } };
    case 'textDocument/definition': {
      // `n*.ts` files mention the name but bind another declaration.
      const uri = p.textDocument.uri;
      const other = 'file://' + root + '/other.ts';
      if (/\/n\d+\.ts$/.test(uri)) return [{ uri: other, range: at(0, 16, 5) }];
      if (uri === other) return [{ uri, range: at(0, 16, 5) }];
      return [decl];
    }
    case 'textDocument/references': {
      const uri = p.textDocument.uri;
      if (uri === declUri) return [decl];
      return [{ uri, range: at(0, 9, 6) }, { uri, range: at(1, 0, 6) }, { uri: 'file://' + root + '/f000.ts', range: at(1, 0, 6) }];
    }
    case 'textDocument/prepareCallHierarchy':
      return [{ name: 'target', kind: 12, uri: declUri, range: at(0, 0, 28), selectionRange: decl.range }];
    case 'callHierarchy/incomingCalls':
    case 'textDocument/documentSymbol':
      return [];
    default:
      return null;
  }
}
function handle(msg) {
  if (msg.method === 'exit') process.exit(0);
  if (msg.id === undefined || !msg.method) return;
  const answer = result(msg);
  // `{ __error }` answers the request with an internal error.
  if (answer && answer.__error) send({ jsonrpc: '2.0', id: msg.id, error: { code: -32603, message: answer.__error } });
  else send({ jsonrpc: '2.0', id: msg.id, result: answer });
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

/// Importer candidates in the fixture: 24 + 24 + 12 (three windows).
const IMPORTERS: usize = 60;

/// A workspace with `a.ts` declaring `target`, [`IMPORTERS`] files that
/// import and call it, the fake server, and an explicit server config.
fn importer_fixture() -> (Fixture, PathBuf, super::LspExecutionConfig) {
    importer_fixture_of(&[("f", IMPORTERS)])
}

/// Like [`importer_fixture`], with `layout` listing `(prefix, count)` runs of
/// files that spell `target`; candidates sort by name, so runs fill importer
/// windows in order. An `n` file spells the name but binds another
/// declaration (no reference); any other prefix imports and calls it.
fn importer_fixture_of(layout: &[(&str, usize)]) -> (Fixture, PathBuf, super::LspExecutionConfig) {
    importer_fixture_serving(layout, IMPORTER_SERVER)
}

/// [`importer_fixture_of`] with its own fake server script.
fn importer_fixture_serving(
    layout: &[(&str, usize)],
    server: &str,
) -> (Fixture, PathBuf, super::LspExecutionConfig) {
    use std::os::unix::fs::PermissionsExt;
    let dir = Fixture::new();
    let outer = dir.0.canonicalize().expect("canonical fixture");
    let root = outer.join("ws");
    std::fs::create_dir_all(&root).expect("workspace");
    // The server script lives beside the workspace: it spells the symbol,
    // and a candidate scan must not count it.
    let script = outer.join("fake-ts-lsp.js");
    let source = server.replace(
        "__ROOT__",
        &serde_json::to_string(&root.to_string_lossy()).expect("root literal"),
    );
    std::fs::write(&script, source).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::fs::write(root.join("tsconfig.json"), "{}\n").expect("tsconfig");
    std::fs::write(root.join("a.ts"), "export function target() {}\n").expect("anchor");
    std::fs::write(root.join("other.ts"), "export function other() {}\n").expect("other");
    for (prefix, count) in layout {
        for index in 0..*count {
            write_importer(&root, &format!("{prefix}{index:03}.ts"));
        }
    }
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": "typescript"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = super::LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..super::LspExecutionConfig::default()
    };
    (dir, root, execution)
}

fn write_importer(root: &Path, name: &str) {
    std::fs::write(
        root.join(name),
        "import { target } from './a';\ntarget();\n",
    )
    .expect("importer");
}

/// Whether a row names a file ending in `name` under a `path` key.
fn lists_path(row: &serde_json::Value, name: &str) -> bool {
    paths_in(row).iter().any(|path| path.ends_with(name))
}

/// The single query row of a continuation.
fn next_row(row: &serde_json::Value, name: &str) -> Option<serde_json::Value> {
    row.pointer(&format!("/next/{name}/query/queries/0"))
        .cloned()
}

/// One walked page: the row and the importer page it belongs to.
struct Walked {
    importer_page: u64,
    row: serde_json::Value,
}

/// Run `start` and every page lead its pages offer (`checks::Walker`: each
/// call once, each match row once, every output and lead within the
/// contract). A page offers `next.nextPage` or `next.nextImporterPage`,
/// never both. Also returns those continuations, by name, for stale checks.
async fn walk_pages(
    pool: &LspClientPool,
    root: &Path,
    execution: &super::LspExecutionConfig,
    start: serde_json::Value,
) -> (Vec<Walked>, Vec<(String, serde_json::Value)>) {
    let paths = crate::tools::test_support::workspace_policy(root);
    let mut pages = Vec::new();
    let mut continuations = Vec::new();
    let mut walker = Walker::new(
        ToolId::LspSearch.as_str(),
        serde_json::json!({ "queries": [start] }),
    );
    while let Some(call) = walker.next_query() {
        let row = call["queries"][0].clone();
        let importer_page = row
            .get("importerPage")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let query = serde_json::from_value(row.clone()).expect("continuation is a valid query");
        let result = super::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            pool,
            &paths,
            execution,
            None,
        )
        .await
        .expect("page");
        assert_ne!(result["status"], "error", "{result}");
        let page = next_row(&result, "nextPage");
        let importer = next_row(&result, "nextImporterPage");
        assert!(
            page.is_none() || importer.is_none(),
            "nextImporterPage only on a window's last location page: {result}"
        );
        for (name, call) in [("nextPage", page), ("nextImporterPage", importer)] {
            if let Some(call) = call {
                continuations.push((name.to_owned(), call));
            }
        }
        walker.accept(Page {
            output: crate::tools::test_support::published(ToolId::LspSearch, &row, &result),
            rows: match_rows(&result),
        });
        pages.push(Walked {
            importer_page,
            row: result,
        });
    }
    walker.finish();
    (pages, continuations)
}

/// `published` without its `next.retry` lead, which reruns this same call
/// (checked here, with the output's other leads), so a page walk must not
/// follow it.
fn without_rerun(
    mut published: serde_json::Value,
    query: &super::LspSearchQuery,
) -> serde_json::Value {
    crate::tools::test_support::checks::assert_leads_valid(&published);
    if let Some(next) = published
        .pointer_mut("/results/0/data/next")
        .and_then(serde_json::Value::as_object_mut)
    {
        if let Some(retry) = next.remove("retry") {
            let rerun: super::LspSearchQuery =
                serde_json::from_value(retry["query"]["queries"][0].clone()).expect("retry query");
            assert_eq!(
                lsp_search_query::to_row(&rerun),
                lsp_search_query::to_row(query),
                "retry reruns this call"
            );
        }
        if next.is_empty() {
            published["results"][0]["data"]
                .as_object_mut()
                .expect("data")
                .remove("next");
        }
    }
    published
}

/// One id per match row on a page, grouped under `files` or listed in
/// `matches`: its file and the whole match.
fn match_rows(row: &serde_json::Value) -> Vec<String> {
    let grouped = row
        .pointer("/files")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let path = file["path"].as_str().expect("path").to_owned();
            file["matches"]
                .as_array()
                .expect("matches")
                .iter()
                .map(move |hit| format!("{path} {hit}"))
        });
    let listed = row
        .pointer("/matches")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(serde_json::Value::to_string);
    grouped.chain(listed).collect()
}

/// `(file name, line, column)` of every reference row on a page.
fn reference_rows(row: &serde_json::Value) -> Vec<(String, u64, u64)> {
    let mut rows = Vec::new();
    for file in row
        .pointer("/files")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = Path::new(file["path"].as_str().expect("path"))
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        for site in file["matches"].as_array().expect("matches") {
            rows.push((
                name.clone(),
                site["line"].as_u64().expect("line"),
                site["column"].as_u64().expect("column"),
            ));
        }
    }
    rows
}

/// Walks `operation` over a workspace whose first window imports `target`,
/// then `silent` files that spell it without binding it, then one importer.
async fn walk_across_silent_windows(operation: &str, silent: usize) -> (Fixture, Vec<Walked>) {
    let (dir, root, execution) = importer_fixture_of(&[("f", 24), ("n", silent), ("z", 1)]);
    let pool = fake_server_pool();
    let start = serde_json::json!({
        "operation": operation, "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "pageSize": 100,
        "workspaceRoot": root.to_string_lossy()
    });
    let (pages, _) = walk_pages(&pool, &root, &execution, start).await;
    (dir, pages)
}

/// A verified importer whose references request fails is a gap in the
/// answer: the row is partial with `importerRequestFailed`, the error, and
/// a retry; it never reads as complete.
#[test]
fn a_failed_importer_request_marks_the_row_partial() {
    if !require_node() {
        return;
    }
    let server = IMPORTER_SERVER.replace(
        "      if (uri === declUri) return [decl];\n      return [",
        "      if (uri === declUri) return [decl];\n      if (uri !== declUri) return { __error: 'importer references failed' };\n      return [",
    );
    assert_ne!(server, IMPORTER_SERVER, "the failing branch is injected");
    let (_dir, root, execution) = importer_fixture_serving(&[("f", 2)], &server);
    let pool = fake_server_pool();
    let query_row = serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "pageSize": 100,
        "workspaceRoot": root.to_string_lossy()
    });
    let query = serde_json::from_value(query_row.clone()).expect("query");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(super::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &query_row, &row);
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(
        row["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&serde_json::json!("importerRequestFailed"))),
        "{row}"
    );
    assert!(
        row["warnings"]
            .to_string()
            .contains("importer references failed"),
        "{row}"
    );
    assert!(row["next"].get("retry").is_some(), "{row}");
}

/// `includeDeclaration:false` drops the anchor's declaration, never an
/// importer's import binding: asked from the importer (as tsserver
/// answers), the binding is that file's declaration of its alias, and the
/// anchor's declaration is a reference.
#[test]
fn importer_recovery_without_declarations_keeps_import_bindings() {
    if !require_node() {
        return;
    }
    let honoring = "    case 'textDocument/references': {\n      const uri = p.textDocument.uri;\n      const inc = !!(p.context && p.context.includeDeclaration);\n      if (uri === declUri) return inc ? [decl] : [];\n      return inc ? [{ uri, range: at(0, 9, 6) }, { uri, range: at(1, 0, 6) }, decl] : [{ uri, range: at(1, 0, 6) }, decl];\n    }\n";
    let start = IMPORTER_SERVER
        .find("    case 'textDocument/references': {")
        .expect("references case");
    let end = start
        + IMPORTER_SERVER[start..]
            .find("    case 'textDocument/prepareCallHierarchy':")
            .expect("next case");
    let server = format!(
        "{}{honoring}{}",
        &IMPORTER_SERVER[..start],
        &IMPORTER_SERVER[end..]
    );
    let (_dir, root, execution) = importer_fixture_serving(&[("f", 2)], &server);
    let pool = fake_server_pool();
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let rows = |include_declaration: bool| {
        let query_row = serde_json::json!({
            "operation": "references", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "pageSize": 100,
            "includeDeclaration": include_declaration,
            "workspaceRoot": root.to_string_lossy()
        });
        let query = serde_json::from_value(query_row.clone()).expect("query");
        let row = runtime
            .block_on(super::execute(
                query,
                &crate::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
            ))
            .expect("row");
        assert_row_contract(ToolId::LspSearch, &query_row, &row);
        let mut rows = reference_rows(&row);
        rows.sort();
        rows
    };
    let importer_sites = |file: &str| [(file.to_owned(), 1, 10), (file.to_owned(), 2, 1)];
    let mut without = importer_sites("f000.ts").to_vec();
    without.extend(importer_sites("f001.ts"));
    assert_eq!(rows(false), without, "declaration out, bindings in");
    let mut with = vec![("a.ts".to_owned(), 1, 17)];
    with.extend(without);
    assert_eq!(rows(true), with);
}

/// A server that reaches the workspace through a symlink as well (a
/// workspace package linked under `node_modules`) names each site under both
/// spellings, and its `includeDeclaration: false` filter knows the
/// declaration under its real path only. Each site is listed once, under its
/// real path, and the declaration stays out.
#[test]
fn symlinked_twins_list_each_site_once_under_its_real_path() {
    if !require_node() {
        return;
    }
    let twins = "    case 'textDocument/references': {\n      const uri = p.textDocument.uri;\n      const inc = !!(p.context && p.context.includeDeclaration);\n      const linked = (u) => u.replace(root + '/', root + '/node_modules/pkg/');\n      const use = { uri: 'file://' + root + '/f000.ts', range: at(1, 0, 6) };\n      const twins = [use, decl].map((site) => ({ uri: linked(site.uri), range: site.range }));\n      if (uri === declUri) return (inc ? [decl, use] : [use]).concat(twins);\n      return [{ uri, range: at(0, 9, 6) }, use, decl];\n    }\n";
    let start = IMPORTER_SERVER
        .find("    case 'textDocument/references': {")
        .expect("references case");
    let end = start
        + IMPORTER_SERVER[start..]
            .find("    case 'textDocument/prepareCallHierarchy':")
            .expect("next case");
    let server = format!(
        "{}{twins}{}",
        &IMPORTER_SERVER[..start],
        &IMPORTER_SERVER[end..]
    );
    let (_dir, root, execution) = importer_fixture_serving(&[("f", 1)], &server);
    std::fs::create_dir_all(root.join("node_modules")).expect("node_modules");
    std::os::unix::fs::symlink(&root, root.join("node_modules/pkg")).expect("package link");
    let pool = fake_server_pool();
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let rows = |include_declaration: bool| {
        let query_row = serde_json::json!({
            "operation": "references", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "pageSize": 100,
            "includeDeclaration": include_declaration,
            "workspaceRoot": root.to_string_lossy()
        });
        let query = serde_json::from_value(query_row.clone()).expect("query");
        let row = runtime
            .block_on(super::execute(
                query,
                &crate::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
            ))
            .expect("row");
        assert_row_contract(ToolId::LspSearch, &query_row, &row);
        let mut sites = Vec::new();
        for file in row["files"].as_array().expect("files") {
            for site in file["matches"].as_array().expect("matches") {
                let path = Path::new(file["path"].as_str().expect("path"));
                let path = path.strip_prefix(&root).unwrap_or(path);
                sites.push((
                    path.to_string_lossy().into_owned(),
                    site["line"].as_u64().expect("line"),
                ));
            }
        }
        sites.sort();
        sites
    };
    let site = |path: &str, line: u64| (path.to_owned(), line);
    // Declarations left out first, then in, then out again: the answer does
    // not depend on what the server answered before.
    assert_eq!(
        rows(false),
        [site("f000.ts", 2)],
        "declaration out, one spelling"
    );
    assert_eq!(rows(true), [site("a.ts", 1), site("f000.ts", 2)]);
    assert_eq!(rows(false), [site("f000.ts", 2)]);
}

/// A gitignored file that spells the name is never an importer candidate,
/// and the row says so: partial, with the deciding ignore rule, and a lead
/// whose localSearch rows (ignored files included) reach that file.
#[test]
fn importer_recovery_names_gitignored_files_it_skipped() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_of(&[("f", 2)]);
    std::fs::create_dir_all(root.join(".git")).expect("repository marker");
    std::fs::write(root.join(".gitignore"), "gen/\n").expect("gitignore");
    std::fs::create_dir_all(root.join("gen")).expect("ignored dir");
    std::fs::write(
        root.join("gen/g000.ts"),
        "import { target } from '../a';\ntarget();\n",
    )
    .expect("ignored importer");
    let pool = fake_server_pool();
    let query_row = serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "pageSize": 100,
        "workspaceRoot": root.to_string_lossy()
    });
    let query = serde_json::from_value(query_row.clone()).expect("query");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(super::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &query_row, &row);
    assert!(
        row["files"]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("f001.ts")))),
        "{row}"
    );
    assert!(
        !row["files"]
            .as_array()
            .is_some_and(|files| files.iter().any(|file| file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("g000.ts")))),
        "the ignored file is no answer row: {row}"
    );
    assert_eq!(row["isPartial"], true, "{row}");
    assert!(
        row["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&serde_json::json!("importerScanIgnored"))),
        "{row}"
    );
    let warned = row["warnings"].to_string();
    assert!(
        warned.contains("1 imports it") && warned.contains(".gitignore:1 'gen/'"),
        "{row}"
    );
    let lead = &row["next"]["includeSkipped"];
    assert_eq!(lead["tool"], "localSearch", "{row}");
    let rows = lead["query"]["queries"].as_array().expect("lead rows");
    let policy = crate::tools::test_support::workspace_policy(&root);
    let found = rows
        .iter()
        .flat_map(|search| {
            assert_eq!(search["includeIgnored"], true, "{search}");
            let query: crate::tools::local_search::LocalSearchQuery =
                serde_json::from_value(search.clone()).expect("lead row is a localSearch query");
            crate::tools::local_search::execute_local_search(
                &query,
                &policy,
                &crate::security::ContentSecurity::new(),
                &crate::execution::cancel::NeverCancel,
                None,
                None,
                None,
            )
            .expect("lead runs")
            .files
            .into_iter()
            .map(|file| file.path)
        })
        .collect::<Vec<_>>();
    assert!(
        found.iter().any(|path| path.ends_with("g000.ts")),
        "{found:?}"
    );
}

/// An importer window with no verified site is a step of a continuing scan,
/// not an empty answer: the request goes on to the next window, so a later
/// page always lists sites or ends the scan, and the sites past the silent
/// windows are one `next.nextImporterPage` away.
#[test]
fn importer_windows_without_sites_are_skipped_not_returned_empty() {
    if !require_node() {
        return;
    }
    for operation in ["references", "callers"] {
        let (_dir, pages) = tokio::runtime::Runtime::new()
            .expect("rt")
            .block_on(walk_across_silent_windows(operation, 48));
        // Windows: f000-f023 | n000-n023 | n024-n047 | z000.
        assert_eq!(
            pages.len(),
            2,
            "{operation}: the silent windows cost no page"
        );
        let landed = &pages[1].row;
        assert!(landed.get("status").is_none(), "{operation}: {landed}");
        assert!(
            lists_path(landed, "z000.ts"),
            "{operation}: the importer behind the silent windows is listed: {landed}"
        );
        assert!(landed.get("isPartial").is_none(), "{operation}: {landed}");
    }
}

/// One request that verifies several importer windows opens more files than
/// the client keeps open (64): each window re-synchronizes the anchor first,
/// so the identity checks of the last window still run against the
/// anchor's program. The fake server resolves a candidate's occurrence to
/// the declaration only while `a.ts` is open, as tsserver drops the
/// program a closed anchor held.
#[test]
fn importer_windows_past_the_open_document_cap_keep_the_anchor_open() {
    if !require_node() {
        return;
    }
    let tracking = IMPORTER_SERVER
        .replace(
            "function handle(msg) {\n  if (msg.method === 'exit') process.exit(0);",
            "const open = new Set();\nfunction handle(msg) {\n  if (msg.method === 'exit') process.exit(0);\n  if (msg.method === 'textDocument/didOpen') open.add(msg.params.textDocument.uri);\n  if (msg.method === 'textDocument/didClose') open.delete(msg.params.textDocument.uri);",
        )
        .replace(
            "      return [decl];\n    }\n    case 'textDocument/references'",
            "      return open.has(declUri) ? [decl] : [];\n    }\n    case 'textDocument/references'",
        );
    assert_eq!(
        tracking.matches("open.has(declUri)").count() + tracking.matches("open.add").count(),
        2,
        "both edits applied"
    );
    // Windows: f000-f023 (page 1) | n000-n023 | n024-n047 | n048-n071 |
    // z000: page 2 opens 73 candidates in one request.
    let (_dir, root, execution) =
        importer_fixture_serving(&[("f", 24), ("n", 72), ("z", 1)], &tracking);
    let pool = fake_server_pool();
    let start = serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "pageSize": 100,
        "workspaceRoot": root.to_string_lossy()
    });
    let (pages, _) = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(walk_pages(&pool, &root, &execution, start));
    let last = &pages.last().expect("pages").row;
    assert!(
        lists_path(last, "z000.ts"),
        "the importer after 72 silent candidates is verified: {last}"
    );
    assert!(last.get("isPartial").is_none(), "{last}");
}

/// The first page is no exception: when its windows, up to the request cap,
/// held no site and the scan goes on, it is a page of a continuing scan
/// (exit 0 with the importer lead), never an `empty` result.
#[test]
fn a_first_page_of_silent_windows_with_more_to_scan_is_not_empty() {
    if !require_node() {
        return;
    }
    let silent = 24 * (super::importers::MAX_WINDOWS_PER_REQUEST + 2);
    // The operations walk separate workspaces and pools, so they run side by
    // side: each window of a silent server waits out its own settle.
    let walk = |operation: &'static str| async move {
        let (_dir, root, execution) = importer_fixture_of(&[("n", silent), ("z", 1)]);
        let pool = fake_server_pool();
        let start = serde_json::json!({
            "operation": operation, "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "pageSize": 100,
            "workspaceRoot": root.to_string_lossy()
        });
        let (pages, _) = walk_pages(&pool, &root, &execution, start).await;
        let first = &pages[0].row;
        assert_ne!(
            first.get("status").and_then(serde_json::Value::as_str),
            Some("empty"),
            "{operation}: {first}"
        );
        assert!(
            first["next"]["nextImporterPage"].is_object(),
            "{operation}: {first}"
        );
        assert!(
            pages
                .last()
                .is_some_and(|page| lists_path(&page.row, "z000.ts")),
            "{operation}: the walk reaches the importer past the silent windows"
        );
    };
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(async { tokio::join!(walk("references"), walk("callers")) });
}

/// A request skips at most [`super::importers::MAX_WINDOWS_PER_REQUEST`] windows. A
/// page that ends there with no site says so plainly: it is a successful
/// page of a scan that continues (never an `empty` result), its note names
/// the candidates verified, and `next.nextImporterPage` reaches the rest.
#[test]
fn a_capped_run_of_silent_windows_is_a_page_of_a_continuing_scan() {
    if !require_node() {
        return;
    }
    let silent = 24 * (super::importers::MAX_WINDOWS_PER_REQUEST + 2);
    // Separate workspaces and pools: the operations walk side by side.
    let walk = |operation: &'static str| async move {
        let (_dir, pages) = walk_across_silent_windows(operation, silent).await;
        assert_eq!(pages.len(), 3, "{operation}: first, capped, landing");
        let capped = &pages[1].row;
        assert!(
            capped.get("status").is_none(),
            "{operation}: a page of a continuing scan is not an empty result: {capped}"
        );
        assert!(capped["next"]["nextImporterPage"].is_object(), "{capped}");
        assert_eq!(capped["isPartial"], true, "{capped}");
        let said = capped.to_string();
        assert!(
            said.contains("held no reference to this symbol") && said.contains("scan continues"),
            "{operation}: {capped}"
        );
        // One counter: sites are paged by `pagination`, the scan is told in
        // candidates, never as a second "page" count.
        assert!(!said.contains("importer page"), "{operation}: {capped}");
        assert!(
            pages[2].row["files"]
                .as_array()
                .is_some_and(|files| files.iter().any(|file| file["path"]
                    .as_str()
                    .is_some_and(|path| path.ends_with("z000.ts")))),
            "{operation}"
        );
    };
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(async { tokio::join!(walk("references"), walk("callers")) });
}

/// References past the importer cap: walking `nextPage` then
/// `nextImporterPage` reaches every importer file in exactly one window,
/// lists every row once (a recovered row in another window's file waits for
/// that window), and only importer page 1 lists the anchor's own answer.
/// A changed candidate list stales both a window's entry and its later
/// location pages, and the restart walks from the first window again.
#[test]
fn importer_pages_reach_every_reference_exactly_once() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = fake_server_pool();
        let start = serde_json::json!({
            "operation": "references", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "pageSize": 10,
            "workspaceRoot": root.to_string_lossy()
        });
        let (pages, continuations) = walk_pages(&pool, &root, &execution, start.clone()).await;
        let windows = pages
            .iter()
            .map(|page| page.importer_page)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(windows.into_iter().collect::<Vec<_>>(), vec![1, 2, 3]);
        let mut seen = std::collections::BTreeMap::new();
        for page in &pages {
            for row in reference_rows(&page.row) {
                if row.0 == "a.ts" {
                    assert_eq!(
                        page.importer_page, 1,
                        "only importer page 1 lists the anchor"
                    );
                }
                if let Some(earlier) = seen.insert(row.clone(), page.importer_page) {
                    panic!(
                        "{row:?} listed twice (windows {earlier} and {})",
                        page.importer_page
                    );
                }
            }
        }
        let mut expected = vec![("a.ts".to_owned(), 1, 17)];
        for index in 0..IMPORTERS {
            let name = format!("f{index:03}.ts");
            expected.push((name.clone(), 1, 10));
            expected.push((name, 2, 1));
        }
        expected.sort();
        assert_eq!(
            seen.keys().cloned().collect::<Vec<_>>(),
            expected,
            "no gaps"
        );
        // Each importer file is listed by its own window: f000-f023 in 1,
        // f024-f047 in 2, f048-f059 in 3.
        for ((name, _, _), window) in &seen {
            if let Some(index) = name
                .strip_prefix('f')
                .and_then(|rest| rest.strip_suffix(".ts"))
                .and_then(|index| index.parse::<usize>().ok())
            {
                assert_eq!(*window as usize, index / 24 + 1, "{name}");
            }
        }
        // Only the last window's last page is complete; earlier windows are
        // partial, never a terminal limit.
        for page in &pages {
            assert!(page.row.get("terminalLimit").is_none(), "{}", page.row);
        }
        let last = &pages.last().expect("pages").row;
        assert!(last.get("isPartial").is_none(), "{last}");
        let first = &pages[0].row;
        assert_eq!(first["isPartial"], true, "{first}");
        assert!(
            first["partialReasons"]
                .as_array()
                .is_some_and(|reasons| reasons.contains(&"importerScanCapped".into())),
            "{first}"
        );

        // A new candidate stales the second window's entry and its later
        // location page alike; the restart has no importer cursor.
        let entry = continuations
            .iter()
            .find(|(name, _)| name == "nextImporterPage")
            .map(|(_, row)| row.clone())
            .expect("a window entry");
        let inside = continuations
            .iter()
            .find(|(name, row)| name == "nextPage" && row["importerPage"] == 2)
            .map(|(_, row)| row.clone())
            .expect("a later location page of window 2");
        write_importer(&root, "f000a.ts");
        let paths = crate::tools::test_support::workspace_policy(&root);
        for stale in [entry, inside] {
            let query = serde_json::from_value(stale).expect("query");
            let row = super::execute(
                query,
                &crate::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
            )
            .await
            .expect("stale page");
            assert_eq!(row["errorCode"], "staleSnapshot", "{row}");
            let restart = next_row(&row, "restart").expect("restart");
            for cursor in crate::response::result::CURSOR_FIELDS {
                assert!(restart.get(cursor).is_none(), "{cursor}: {restart}");
            }
        }
        // The restart walks the changed list once more: 61 importers.
        let (pages, _) = walk_pages(&pool, &root, &execution, start).await;
        let rows = pages
            .iter()
            .flat_map(|page| reference_rows(&page.row))
            .collect::<Vec<_>>();
        let unique = rows.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(rows.len(), unique.len(), "no duplicates after restart");
        assert_eq!(rows.len(), 1 + 2 * (IMPORTERS + 1));
    });
}

/// Callers past the importer cap: each importer file's reference-derived
/// caller is listed once, by its own window, and later windows leave out
/// the anchor's walk.
#[test]
fn importer_pages_reach_every_caller_exactly_once() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = fake_server_pool();
        let start = serde_json::json!({
            "operation": "callers", "path": root.join("a.ts").to_string_lossy(),
            "symbolName": "target", "line": 1, "pageSize": 10,
            "workspaceRoot": root.to_string_lossy()
        });
        let (pages, _) = walk_pages(&pool, &root, &execution, start).await;
        let mut callers = Vec::new();
        for page in &pages {
            for file in page
                .row
                .pointer("/files")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = Path::new(file["path"].as_str().expect("path"))
                    .file_name()
                    .expect("file name")
                    .to_string_lossy()
                    .into_owned();
                for _ in file["matches"].as_array().expect("matches") {
                    callers.push((name.clone(), page.importer_page));
                }
            }
        }
        callers.sort();
        let expected = (0..IMPORTERS)
            .map(|index| (format!("f{index:03}.ts"), (index / 24 + 1) as u64))
            .collect::<Vec<_>>();
        assert_eq!(
            callers, expected,
            "every importer's caller once, in its window"
        );
    });
}

/// tsserver answers when octocode's tsgo cannot start, and the answer
/// says so; a target without a fallback still fails as `serverUnavailable`.
#[test]
fn a_tsgo_that_fails_to_start_falls_back_to_tsserver_and_says_so() {
    use super::{Exit, Target, acquire_leased_client};
    use crate::execution::cancel::NeverCancel;
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    let missing = root.join("no-such-tsgo");
    let path = root.join("a.ts").to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
    let query: super::LspSearchQuery = serde_json::from_value(json!({
        "operation":"hover", "mainGoal":"test", "reasoning":"test",
        "path":uri, "symbolName":"a", "line":1
    }))
    .expect("query");
    let target = |fallback| Target {
        path: path.clone(),
        uri: uri.clone(),
        root_only: false,
        config: config(&root, &missing, None),
        fallback,
        root_note: None,
    };
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = LspClientPool::new(LspPoolOptions {
            idle_timeout_ms: 60_000,
            max_entries: 4,
            max_resident_bytes: 0,
            client: FAKE_SERVER_CLIENT,
        });
        let fallback = config(&root, &script, None);
        let leased =
            acquire_leased_client(&query, &target(Some(fallback.clone())), &pool, &NeverCancel)
                .await
                .map_err(|_| "the fallback must answer")
                .expect("leased");
        assert_eq!(leased.config.command, fallback.command);
        let reason = leased
            .fell_back_from
            .as_deref()
            .expect("fallback disclosed");
        assert!(reason.contains("no-such-tsgo"), "{reason}");
        assert!(leased.client.is_alive().await);

        // The failed key sits in its start backoff: the next request falls
        // back at once instead of retrying the broken server.
        let again = acquire_leased_client(&query, &target(Some(fallback)), &pool, &NeverCancel)
            .await
            .map_err(|_| "the fallback must answer")
            .expect("leased again");
        assert!(again.fell_back_from.is_some());

        match acquire_leased_client(&query, &target(None), &pool, &NeverCancel).await {
            Err(Exit::Row(row)) => {
                assert_eq!(row["errorCode"], "serverUnavailable", "{row}");
            }
            _ => panic!("no fallback means a serverUnavailable row"),
        }
        drop(leased);
        drop(again);
        pool.clear_all().await;
    });
}

type ImplementationTargets<'a> = (&'a str, usize, &'a [(&'a str, usize)]);

fn fallback_graph_rows(
    files: &[(&str, &str)],
    definitions: &[(&str, usize, &str, &str, usize)],
    implementations: &[ImplementationTargets<'_>],
    queries: &[(&str, &str, usize, u32)],
) -> Vec<serde_json::Value> {
    fallback_graph_pages(files, definitions, implementations, queries)
        .into_iter()
        .map(|mut pages| {
            assert_eq!(pages.len(), 1, "this fixture expects an unpaged result");
            pages.remove(0)
        })
        .collect()
}

fn fallback_graph_pages(
    files: &[(&str, &str)],
    definitions: &[(&str, usize, &str, &str, usize)],
    implementations: &[ImplementationTargets<'_>],
    queries: &[(&str, &str, usize, u32)],
) -> Vec<Vec<serde_json::Value>> {
    use super::ops::Operation;
    use super::source::{SourceCache, snippet_policy};
    use crate::execution::cancel::NeverCancel;
    use serde_json::json;
    let (_dir, root, script) = fixture();
    for (file, content) in files {
        std::fs::write(root.join(file), content).expect("source fixture");
    }
    let declaration = |file: &str, line: usize| {
        let content = files
            .iter()
            .find(|(name, _)| *name == file)
            .expect("file")
            .1;
        let text = content.lines().nth(line).expect("line");
        // A Rust trait impl is located at its self type, after `for`.
        let words = text.split_whitespace().collect::<Vec<_>>();
        let name = match words.iter().position(|word| *word == "for") {
            Some(at) if words[0].starts_with("impl") => words[at + 1],
            _ => words[2],
        };
        let start = text.find(name).expect("name position");
        json!({"uri": octocode_engine::lsp::uri::path_to_uri(&root.join(file).to_string_lossy()).expect("uri"),
            "range": {"start":{"line":line,"character":start},"end":{"line":line,"character":start+name.len()}}})
    };
    let mut graph = json!({"definitions":{},"implementations":{}});
    for (file, line, name, target, target_line) in definitions {
        let text = files
            .iter()
            .find(|(entry, _)| entry == file)
            .expect("file")
            .1
            .lines()
            .nth(*line)
            .expect("line");
        let column = text.rfind(name).expect("reference position");
        graph["definitions"]
            .as_object_mut()
            .expect("definition map")
            .entry(format!("{file}:{line}:{column}"))
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("definition targets")
            .push(declaration(target, *target_line));
    }
    for (file, line, targets) in implementations {
        let node = declaration(file, *line);
        let column = node["range"]["start"]["character"]
            .as_u64()
            .expect("column");
        graph["implementations"][format!("{file}:{line}:{column}")] = json!(
            targets
                .iter()
                .map(|(target, line)| declaration(target, *line))
                .collect::<Vec<_>>()
        );
    }
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "definitionProvider: true, implementationProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'textDocument/definition' || msg.method === 'textDocument/implementation') {
    const graph = JSON.parse(process.env.FAKE_LSP_GRAPH);
    const file = require('path').basename(new URL(msg.params.textDocument.uri).pathname);
    const p = msg.params.position;
    const key = `${file}:${p.line}:${p.character}`;
    const entries = graph[msg.method.endsWith('/definition') ? 'definitions' : 'implementations'];
    send({ jsonrpc: '2.0', id: msg.id, result: entries[key] || [] });"#,
        );
    std::fs::write(&script, server).expect("fixture server");
    let mut config = config(&root, &script, None);
    config.env = Some(
        [("FAKE_LSP_GRAPH".into(), graph.to_string())]
            .into_iter()
            .collect(),
    );
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config);
        client.start().await.expect("server starts");
        let mut rows = Vec::new();
        for (operation, file, line, depth) in queries {
            let node = declaration(file, *line);
            let name = files.iter().find(|(entry, _)| entry == file).expect("file").1.lines().nth(*line).expect("line").split_whitespace().nth(2).expect("name");
            let path = root.join(file).to_string_lossy().into_owned();
            let start = json!({"operation":operation,"path":path,"line":line+1,"symbolName":name,"graphDepth":depth});
            let mut pages = Vec::new();
            let mut walker = Walker::new(ToolId::LspSearch.as_str(), json!({"queries":[start]}));
            while let Some(call) = walker.next_query() {
                let wire = call["queries"][0].clone();
                let query: super::LspSearchQuery = serde_json::from_value(wire.clone()).expect("executable query");
                let mut sources = SourceCache::new(&paths);
                let policy = snippet_policy(&paths);
                let scope = super::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
                let row = Operation { client: &client, query: &query, sources: &mut sources, snippet_policy: &policy,
                    cancel: &NeverCancel, path: &path, scope: &scope, root_only: false, line: u32::try_from(*line).expect("line"),
                    character: u32::try_from(node["range"]["start"]["character"].as_u64().expect("character")).expect("character"),
                    language_id:Some("typescript"), budget:16_000, response_window:None }.run().await.expect("fallback hierarchy");
                let row = super::failure::with_next(&query, row);
                if let Some(next) = row.pointer("/next/nextPage/query/queries/0") {
                    let continued: super::LspSearchQuery = serde_json::from_value(next.clone()).expect("executable nextPage");
                    assert!(lsp_search_query::page(&continued) > lsp_search_query::page(&query), "page cursor must advance");
                }
                let published = crate::tools::test_support::published(ToolId::LspSearch, &wire, &row);
                walker.accept(Page { output: without_rerun(published, &query), rows: match_rows(&row) });
                pages.push(row);
            }
            walker.finish();
            rows.push(pages);
        }
        client.stop().await.expect("fixture server closed");
        rows
    })
}

#[test]
fn fallback_type_hierarchy_obeys_depth_and_records_intermediate_identity() {
    let rows = fallback_graph_rows(
        &[(
            "chain.ts",
            "export class Base {}\nexport class Middle extends Base {}\nexport class Leaf extends Middle {}\n",
        )],
        &[
            ("chain.ts", 1, "Base", "chain.ts", 0),
            ("chain.ts", 2, "Middle", "chain.ts", 1),
        ],
        &[
            ("chain.ts", 0, &[("chain.ts", 1), ("chain.ts", 2)]),
            ("chain.ts", 1, &[("chain.ts", 2)]),
            ("chain.ts", 2, &[]),
        ],
        &[
            ("supertypes", "chain.ts", 2, 1),
            ("supertypes", "chain.ts", 2, 2),
            ("supertypes", "chain.ts", 2, 3),
            ("subtypes", "chain.ts", 0, 1),
            ("subtypes", "chain.ts", 0, 2),
            ("subtypes", "chain.ts", 0, 3),
        ],
    );
    for (index, row) in rows.iter().enumerate() {
        let matches = row["matches"].as_array().expect("matches");
        assert_eq!(
            matches.len(),
            if index % 3 == 0 { 1 } else { 2 },
            "{index}: {row}"
        );
        if index % 3 != 0 {
            assert_eq!(matches[1]["via"]["symbolName"], "Middle", "{row}");
        }
        assert!(
            row.get("isPartial").is_none(),
            "fully resolved graph: {row}"
        );
    }
}

/// rust-analyzer answers `implementation` on a trait with its impl blocks,
/// located at each self type: each trait impl is a direct subtype (its
/// clause names the trait), and an impl has no subtypes of its own.
#[test]
fn fallback_rust_subtypes_are_the_trait_impls() {
    let rows = fallback_graph_rows(
        &[(
            "lib.rs",
            "pub trait Scan {}\npub struct Foo {}\nimpl Scan for Foo {}\npub struct Bar {}\nimpl<T: Scan> Scan for Bar {}\nimpl Foo {}\n",
        )],
        &[
            ("lib.rs", 2, "Scan", "lib.rs", 0),
            ("lib.rs", 4, "Scan", "lib.rs", 0),
        ],
        &[
            ("lib.rs", 0, &[("lib.rs", 2), ("lib.rs", 4)]),
            // Asked at an impl's self type, a server lists that type's impls.
            ("lib.rs", 2, &[("lib.rs", 2), ("lib.rs", 5)]),
        ],
        &[("subtypes", "lib.rs", 0, 1), ("subtypes", "lib.rs", 0, 2)],
    );
    for row in rows {
        let found = row["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|found| (found["symbolName"].as_str(), found["kind"].as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            found,
            [(Some("Foo"), Some("impl")), (Some("Bar"), Some("impl"))],
            "{row}"
        );
        assert!(row.get("isPartial").is_none(), "{row}");
    }
}

#[test]
fn fallback_type_hierarchy_resolves_aliases_and_rejects_shadow_names() {
    let rows = fallback_graph_rows(
        &[
            ("base.ts", "export class Base {}\n"),
            (
                "alias.ts",
                "import { Base as Renamed } from './base';\nexport class Child extends Renamed {}\n",
            ),
            (
                "shadow.ts",
                "export class Base {}\nexport class Unrelated extends Base {}\n",
            ),
        ],
        &[
            ("alias.ts", 1, "Renamed", "base.ts", 0),
            ("shadow.ts", 1, "Base", "shadow.ts", 0),
        ],
        &[("base.ts", 0, &[("alias.ts", 1), ("shadow.ts", 1)])],
        &[
            ("supertypes", "alias.ts", 1, 1),
            ("subtypes", "base.ts", 0, 1),
        ],
    );
    assert_eq!(rows[0]["matches"].as_array().expect("matches").len(), 1);
    assert!(
        rows[0]["matches"][0]["path"]
            .as_str()
            .expect("path")
            .ends_with("base.ts"),
        "{}",
        rows[0]
    );
    assert_eq!(rows[1]["matches"].as_array().expect("matches").len(), 1);
    assert_eq!(rows[1]["matches"][0]["symbolName"], "Child", "{}", rows[1]);
    assert!(
        rows.iter().all(|row| row.get("isPartial").is_none()),
        "{rows:?}"
    );
}

#[test]
fn fallback_type_hierarchy_resolves_implementation_aliases_and_deduplicates_declarations() {
    let rows = fallback_graph_rows(
        &[
            (
                "chain.ts",
                "export interface Base {}\nexport class Middle implements Base {}\nexport class Leaf extends Middle {}\n",
            ),
            (
                "alias.ts",
                "import { Middle as Parent } from './chain';\nexport class AliasLeaf extends Parent {}\n",
            ),
            (
                "shadow.ts",
                "export interface Base {}\nexport class Middle implements Base {}\n",
            ),
        ],
        &[
            ("chain.ts", 1, "Base", "chain.ts", 0),
            ("chain.ts", 2, "Middle", "chain.ts", 1),
            ("alias.ts", 0, "Middle", "chain.ts", 1),
            ("alias.ts", 1, "Parent", "chain.ts", 1),
            ("shadow.ts", 1, "Base", "shadow.ts", 0),
        ],
        &[
            (
                "chain.ts",
                0,
                &[
                    ("alias.ts", 0),
                    ("chain.ts", 1),
                    ("chain.ts", 1),
                    ("chain.ts", 2),
                    ("alias.ts", 1),
                    ("shadow.ts", 1),
                ],
            ),
            (
                "chain.ts",
                1,
                &[
                    ("alias.ts", 0),
                    ("chain.ts", 2),
                    ("alias.ts", 1),
                    ("chain.ts", 2),
                ],
            ),
        ],
        &[
            ("subtypes", "chain.ts", 0, 1),
            ("subtypes", "chain.ts", 0, 2),
        ],
    );
    for (index, row) in rows.iter().enumerate() {
        assert!(
            row.get("isPartial").is_none(),
            "all alias candidates resolve: {row}"
        );
        let matches = row["matches"].as_array().expect("matches");
        assert_eq!(matches.len(), if index == 0 { 1 } else { 3 }, "{row}");
        assert_eq!(matches[0]["symbolName"], "Middle", "{row}");
        assert!(
            matches[0]["path"]
                .as_str()
                .expect("path")
                .ends_with("chain.ts"),
            "{row}"
        );
        if index == 1 {
            assert_eq!(matches[1]["symbolName"], "Leaf", "{row}");
            assert_eq!(matches[2]["symbolName"], "AliasLeaf", "{row}");
            for child in &matches[1..] {
                assert_eq!(child["level"], 2, "{row}");
                assert_eq!(child["via"]["symbolName"], "Middle", "{row}");
            }
        }
    }
}

#[test]
fn fallback_type_hierarchy_keeps_known_edges_when_an_implementation_alias_is_ambiguous() {
    let rows = fallback_graph_rows(
        &[
            ("base.ts", "export interface Base {}\n"),
            ("left.ts", "export class Left implements Base {}\n"),
            ("right.ts", "export class Right implements Base {}\n"),
            ("alias.ts", "import { Either } from './left';\n"),
            ("same.ts", "import { Left as One } from './left';\n"),
        ],
        &[
            ("left.ts", 0, "Base", "base.ts", 0),
            ("right.ts", 0, "Base", "base.ts", 0),
            ("alias.ts", 0, "Either", "left.ts", 0),
            ("alias.ts", 0, "Either", "right.ts", 0),
            ("same.ts", 0, "Left", "left.ts", 0),
            ("same.ts", 0, "Left", "left.ts", 0),
        ],
        &[(
            "base.ts",
            0,
            &[("alias.ts", 0), ("same.ts", 0), ("left.ts", 0)],
        )],
        &[("subtypes", "base.ts", 0, 1)],
    );
    let row = &rows[0];
    let matches = row["matches"].as_array().expect("matches");
    assert_eq!(
        matches.len(),
        1,
        "the known declaration stays, ambiguous targets do not: {row}"
    );
    assert_eq!(matches[0]["symbolName"], "Left", "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["typeHierarchyExpansionFailed"]),
        "{row}"
    );
    assert!(
        row["warnings"]
            .to_string()
            .contains("2 distinct type declarations"),
        "{row}"
    );
    // An ambiguous alias answers the same again: no retry.
    assert!(row.pointer("/next/retry").is_none(), "{row}");
}

#[test]
fn fallback_type_hierarchy_preserves_diamond_edges_and_terminates_cycles() {
    let diamond = fallback_graph_rows(
        &[(
            "diamond.ts",
            "export interface Root {}\nexport interface Left extends Root {}\nexport interface Right extends Root {}\nexport interface Join extends Left, Right {}\nexport interface Tail extends Join {}\n",
        )],
        &[
            ("diamond.ts", 1, "Root", "diamond.ts", 0),
            ("diamond.ts", 2, "Root", "diamond.ts", 0),
            ("diamond.ts", 3, "Left", "diamond.ts", 1),
            ("diamond.ts", 3, "Right", "diamond.ts", 2),
            ("diamond.ts", 4, "Join", "diamond.ts", 3),
        ],
        &[
            (
                "diamond.ts",
                0,
                &[
                    ("diamond.ts", 1),
                    ("diamond.ts", 2),
                    ("diamond.ts", 3),
                    ("diamond.ts", 4),
                ],
            ),
            ("diamond.ts", 1, &[("diamond.ts", 3), ("diamond.ts", 4)]),
            ("diamond.ts", 2, &[("diamond.ts", 3), ("diamond.ts", 4)]),
            ("diamond.ts", 3, &[("diamond.ts", 4)]),
        ],
        &[("subtypes", "diamond.ts", 0, 3)],
    );
    let matches = diamond[0]["matches"].as_array().expect("diamond matches");
    assert_eq!(matches.len(), 5, "{diamond:?}");
    assert_eq!(
        matches
            .iter()
            .filter(|entry| entry["symbolName"] == "Tail")
            .count(),
        1,
        "{diamond:?}"
    );
    let via = matches
        .iter()
        .filter(|entry| entry["symbolName"] == "Join")
        .map(|entry| entry["via"]["symbolName"].as_str().expect("via"))
        .collect::<Vec<_>>();
    assert_eq!(via, ["Left", "Right"], "{diamond:?}");
    let cycle = fallback_graph_rows(
        &[(
            "cycle.ts",
            "export class A extends B {}\nexport class B extends A {}\n",
        )],
        &[
            ("cycle.ts", 0, "B", "cycle.ts", 1),
            ("cycle.ts", 1, "A", "cycle.ts", 0),
        ],
        &[
            ("cycle.ts", 0, &[("cycle.ts", 0), ("cycle.ts", 1)]),
            ("cycle.ts", 1, &[("cycle.ts", 0), ("cycle.ts", 1)]),
        ],
        &[
            ("supertypes", "cycle.ts", 0, 20),
            ("subtypes", "cycle.ts", 0, 20),
        ],
    );
    for row in cycle {
        let matches = row["matches"].as_array().expect("cycle matches");
        assert_eq!(matches.len(), 2, "{row}");
        assert_eq!(matches[0]["symbolName"], "B", "{row}");
        assert_eq!(matches[1]["symbolName"], "A", "{row}");
        assert_eq!(matches[1]["via"]["symbolName"], "B", "{row}");
    }
}

#[test]
fn fallback_type_hierarchy_marks_unresolved_relations_partial_without_retry() {
    let rows = fallback_graph_rows(
        &[(
            "unknown.ts",
            "export class Base {}\nexport class Child extends Unknown {}\n",
        )],
        &[],
        &[("unknown.ts", 0, &[("unknown.ts", 1)])],
        &[
            ("supertypes", "unknown.ts", 1, 2),
            ("subtypes", "unknown.ts", 0, 2),
        ],
    );
    for row in rows {
        assert_eq!(row["isPartial"], true, "{row}");
        assert_eq!(
            row["partialReasons"],
            serde_json::json!(["typeHierarchyExpansionFailed"]),
            "{row}"
        );
        // An unresolved base stays unresolved: retrying reruns the same walk.
        assert!(row.pointer("/next/retry").is_none(), "{row}");
        assert!(
            row["matches"].as_array().is_none_or(Vec::is_empty),
            "unverified edges omitted: {row}"
        );
    }
}

#[test]
fn fallback_subtypes_page_the_anchor_candidates_by_window() {
    let cap = super::walk::MAX_HIERARCHY_NODES;
    let mut content = "export class Base {}\n".to_owned();
    for index in 0..=cap {
        content.push_str(&format!("export class Child{index} extends Base {{}}\n"));
    }
    let definitions = (1..=cap + 1)
        .map(|line| ("wide.ts", line, "Base", "wide.ts", 0))
        .collect::<Vec<_>>();
    let candidates = (1..=cap + 1)
        .map(|line| ("wide.ts", line))
        .collect::<Vec<_>>();
    let mut results = fallback_graph_pages(
        &[("wide.ts", &content)],
        &definitions,
        &[("wide.ts", 0, &candidates)],
        &[("subtypes", "wide.ts", 0, 1)],
    );
    assert_eq!(results.len(), 1);
    let pages = results.remove(0);
    // The anchor's candidates page by window: the first window's rows span
    // result pages, and its last page continues to the next window.
    let first_window = pages
        .iter()
        .take_while(|row| row["partialReasons"] == serde_json::json!(["hierarchyCandidateLimit"]))
        .collect::<Vec<_>>();
    assert!(
        first_window.len() > 1,
        "the verified prefix must cross the page budget"
    );
    for (page_index, row) in first_window.iter().enumerate() {
        assert_eq!(row["pagination"]["totalItems"], cap, "{row}");
        assert_eq!(row["pagination"]["currentPage"], page_index + 1, "{row}");
        assert_eq!(row["isPartial"], true, "{row}");
        assert!(row.get("terminalLimit").is_none(), "{row}");
        assert_eq!(
            row.pointer("/next/nextCandidatePage").is_some(),
            page_index + 1 == first_window.len(),
            "only the window's last page offers the next window: {row}"
        );
        if let Some(snapshot) = row.pointer("/next/nextCandidatePage/query/queries/0/snapshot") {
            assert!(
                snapshot
                    .as_str()
                    .is_some_and(|value| value.starts_with("lsp-candidates:")),
                "candidate continuation pins the complete candidate list: {row}"
            );
        }
    }
    let mut identities = std::collections::HashSet::new();
    let mut reached = 0;
    for row in &pages {
        let shown = row["matches"].as_array().expect("matches");
        assert!(!shown.is_empty(), "each page retains complete rows: {row}");
        for matched in shown {
            assert_eq!(matched["symbolName"], format!("Child{reached}"), "{row}");
            assert_eq!(matched["displayRange"]["startLine"], reached + 2, "{row}");
            assert_eq!(matched["level"], 1, "{row}");
            assert!(
                identities.insert((matched["path"].clone(), matched["displayRange"].to_string())),
                "no repeated declaration: {matched}"
            );
            reached += 1;
        }
    }
    assert_eq!(reached, cap + 1, "every candidate is reachable");
    assert_eq!(
        identities.len(),
        cap + 1,
        "each declaration must be reached once"
    );
    let last = pages.last().expect("final page");
    assert!(last.get("isPartial").is_none(), "{last}");
    assert!(last.get("next").is_none(), "the route terminates: {last}");
}

#[test]
fn workspace_symbol_provider_coverage_is_unknown_without_losing_semantic_rows() {
    use super::ops::Operation;
    use super::source::{SourceCache, snippet_policy};
    use crate::execution::cancel::NeverCancel;
    use serde_json::{Value, json};
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "workspaceSymbolProvider: true, documentSymbolProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'workspace/symbol') {
    const names = msg.params.query === 'missing' ? [] : ['普通', 'Alias', 'Shared', 'Shared'];
    const uri = require('url').pathToFileURL(require('path').join(process.cwd(), 'a.ts')).href;
    const symbols = names.map((name, line) => ({name, kind: 12,
      containerName: line === 3 ? 'Shadow' : 'Module',
      location: {uri, range: {start: {line, character: line === 0 ? 3 : 0}, end: {line, character: 8}}}}));
    send({jsonrpc:'2.0', id:msg.id, result:symbols});
  } else if (msg.method === 'textDocument/documentSymbol') {
    send({jsonrpc:'2.0', id:msg.id, result:[{name:'ordinary',kind:12,
      range:{start:{line:0,character:0},end:{line:0,character:8}},
      selectionRange:{start:{line:0,character:0},end:{line:0,character:8}}}]});"#,
        );
    std::fs::write(&script, server).expect("fixture server");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        for (page_size, root_only) in [(2, false), (1000, false), (1000, true)] {
            let anchor = if root_only {
                root.to_string_lossy().into_owned()
            } else {
                path.clone()
            };
            let start = json!({
                "operation":"workspaceSymbol", "path":anchor, "symbolName":"Shared",
                "pageSize":page_size
            });
            let mut found = Vec::new();
            let mut page_count = 0;
            let mut walker = Walker::new(ToolId::LspSearch.as_str(), json!({"queries":[start]}));
            while let Some(call) = walker.next_query() {
                let query: super::LspSearchQuery =
                    serde_json::from_value(call["queries"][0].clone()).expect("query");
                let mut sources = SourceCache::new(&paths);
                let policy = snippet_policy(&paths);
                let scope = super::scope::Scope::new(
                    root.to_string_lossy().into_owned(),
                    vec!["*.ts".into()],
                );
                let row = Operation {
                    client: &client,
                    query: &query,
                    sources: &mut sources,
                    snippet_policy: &policy,
                    cancel: &NeverCancel,
                    path: &anchor,
                    scope: &scope,
                    root_only,
                    line: 0,
                    character: 0,
                    language_id: Some("typescript"),
                    budget: 16_000,
                    response_window: None,
                }
                .run()
                .await
                .expect("workspace symbols");
                let row = super::failure::with_next(&query, row);
                assert_eq!(row["isPartial"], true, "{row}");
                assert_eq!(
                    row["partialReasons"],
                    json!(["workspaceSymbolCoverageUnknown"])
                );
                assert!(
                    row.get("terminalLimit").is_none(),
                    "unknown is not a proven cap"
                );
                assert_eq!(row["pagination"]["totalItems"], 4);
                let published = crate::tools::test_support::published(
                    ToolId::LspSearch,
                    &lsp_search_query::to_row(&query),
                    &row,
                );
                assert!(
                    published
                        .pointer("/results/0/data/next/inspectDocumentSymbols")
                        .is_none(),
                    "optional lead is not pagination: {published}"
                );
                found.extend(row["matches"].as_array().expect("matches").iter().cloned());
                page_count += 1;
                if row.pointer("/next/nextPage").is_none() {
                    let hint = crate::tools::test_support::checks::hint(
                        &published["results"][0]["data"],
                        "inspectDocumentSymbols",
                    );
                    assert_eq!(
                        hint["query"]["queries"][0],
                        json!({"operation":"documentSymbols", "path":path})
                    );
                } else {
                    assert!(page_count < 3, "continuation must terminate");
                }
                walker.accept(Page {
                    output: published,
                    rows: match_rows(&row),
                });
            }
            walker.finish();
            assert_eq!(page_count, if page_size == 2 { 2 } else { 1 });
            assert_eq!(
                found
                    .iter()
                    .map(|v| v["symbolName"].as_str().expect("name"))
                    .collect::<Vec<_>>(),
                ["Shared", "Shared", "普通", "Alias"]
            );
            assert_eq!(found[0]["containerName"], "Module");
            assert_eq!(found[1]["containerName"], "Shadow");
            let identities = found
                .iter()
                .map(|v| (v["path"].clone(), v["displayRange"].clone()))
                .collect::<Vec<_>>();
            assert_eq!(
                identities
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                4
            );
            assert_eq!(
                found[2]["displayRange"],
                json!({"startLine":1,"startCharacter":4,"endLine":1})
            );
        }
        for (operation, name, partial) in [
            ("workspaceSymbol", Some("missing"), true),
            ("documentSymbols", None, false),
        ] {
            let mut wire = json!({"operation":operation,"path":path});
            if let Some(name) = name {
                wire["symbolName"] = json!(name);
            }
            let query: super::LspSearchQuery = serde_json::from_value(wire.clone()).expect("query");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let scope =
                super::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
            let row = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &scope,
                root_only: false,
                line: 0,
                character: 0,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await
            .expect("provider answer");
            let row = super::failure::with_next(&query, row);
            assert_eq!(
                row.get("isPartial")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                partial,
                "{row}"
            );
            assert_row_contract(ToolId::LspSearch, &wire, &row);
        }
        client.stop().await.expect("fixture server closed");
    });
}

/// tsgo answers `workspace/symbol` with at most 256 symbols; an answer of
/// exactly that many says the server capped it, one short of it does not.
/// The partial reason stays `workspaceSymbolCoverageUnknown`.
#[test]
fn workspace_symbols_at_the_server_limit_say_capped() {
    use super::ops::Operation;
    use super::source::{SourceCache, snippet_policy};
    use crate::execution::cancel::NeverCancel;
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    // Named `tsgo`, so octocode treats it as tsgo; `Cap<n>` answers n rows.
    let tsgo = root.join("tsgo");
    let server = FAKE_SERVER
        .replace("hoverProvider: true", "workspaceSymbolProvider: true")
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            r#"  } else if (msg.method === 'workspace/symbol') {
    const count = Number(msg.params.query.slice(3));
    const uri = require('url').pathToFileURL(require('path').join(process.cwd(), 'a.ts')).href;
    const symbols = Array.from({length: count}, (_, line) => ({name: msg.params.query + '_' + line, kind: 12,
      location: {uri, range: {start: {line, character: 0}, end: {line, character: 4}}}}));
    send({jsonrpc:'2.0', id:msg.id, result:symbols});"#,
        );
    std::fs::write(&tsgo, server).expect("fixture server");
    std::fs::set_permissions(
        &tsgo,
        std::fs::metadata(&script).expect("mode").permissions(),
    )
    .expect("chmod");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &tsgo, None));
        assert_eq!(client.workspace_symbol_limit(), Some(256));
        client.start().await.expect("server starts");
        for (name, capped) in [("Cap256", true), ("Cap255", false)] {
            let wire = json!({"operation":"workspaceSymbol", "path":path, "symbolName":name});
            let query: super::LspSearchQuery = serde_json::from_value(wire.clone()).expect("query");
            let mut sources = SourceCache::new(&paths);
            let policy = snippet_policy(&paths);
            let scope =
                super::scope::Scope::new(root.to_string_lossy().into_owned(), vec!["*.ts".into()]);
            let row = Operation {
                client: &client,
                query: &query,
                sources: &mut sources,
                snippet_policy: &policy,
                cancel: &NeverCancel,
                path: &path,
                scope: &scope,
                root_only: false,
                line: 0,
                character: 0,
                language_id: Some("typescript"),
                budget: 16_000,
                response_window: None,
            }
            .run()
            .await
            .expect("workspace symbols");
            let row = super::failure::with_next(&query, row);
            assert_eq!(
                row["partialReasons"],
                json!(["workspaceSymbolCoverageUnknown"]),
                "{name}"
            );
            let says_capped = row["warnings"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|warning| {
                    warning
                        .as_str()
                        .is_some_and(|text| text.contains("at most 256 workspace symbols"))
                });
            assert_eq!(says_capped, capped, "{name}: {row}");
            assert_row_contract(ToolId::LspSearch, &wire, &row);
        }
        client.stop().await.expect("fixture server closed");
    });
}
