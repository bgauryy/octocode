//! A server that watches files hears which scope files changed before a request
//! that reads other files. An outline reads only the anchor, which its own open
//! synchronizes, so it walks nothing.

use super::*;
use crate::execution::cancel::NeverCancel;
use octocode_engine::lsp::client::LspClientOptions;
use octocode_engine::lsp::pool::{LspClientPool, LspPoolOptions};
use std::time::Duration;

/// Registers for file events when the first search arrives and logs each one it is sent.
const SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
let buf = Buffer.alloc(0);
// A search is answered once the client has replied to the registration, which
// it does after it recorded it: the answer then proves the client watches.
let registered = false;
let held = [];
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function answerReferences(msg) {
  const uri = msg.params.textDocument.uri;
  send({ jsonrpc: '2.0', id: msg.id, result: [{ uri, range: { start: { line: 0, character: 9 }, end: { line: 0, character: 15 } } }] });
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { documentSymbolProvider: true, referencesProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'workspace/didChangeWatchedFiles') {
    fs.appendFileSync(__dirname + '/events.txt', JSON.stringify(msg.params) + '\n');
  } else if (msg.method === 'textDocument/documentSymbol') {
    send({ jsonrpc: '2.0', id: msg.id, result: [] });
  } else if (msg.id === 'watch' && !msg.method) {
    registered = true;
    held.splice(0).forEach(answerReferences);
  } else if (msg.method === 'textDocument/references') {
    // Registers only now, after the client's first request has already
    // looked for a watching server: the registration arrives late.
    if (registered) { answerReferences(msg); return; }
    held.push(msg);
    if (held.length === 1) send({ jsonrpc: '2.0', id: 'watch', method: 'client/registerCapability', params: { registrations: [{ id: 'w', method: 'workspace/didChangeWatchedFiles', registerOptions: { watchers: [{ globPattern: '**/*' }] } }] } });
  } else if (msg.method === 'exit') {
    process.exit(0);
  } else if (msg.id !== undefined && msg.method) {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
  }
}
process.stdin.on('end', () => process.exit(0));
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

/// A started watching server in a fresh repository, and a runner for rows.
struct Watching {
    root: std::path::PathBuf,
    path: String,
    pool: LspClientPool,
    runtime: tokio::runtime::Runtime,
    execution: crate::tools::lsp_search::LspExecutionConfig,
    paths: crate::policy::path::PathPolicy,
    _dir: crate::tools::test_support::Fixture,
}

impl Watching {
    fn new() -> Self {
        let dir = crate::tools::test_support::Fixture::new();
        let root = dir.0.canonicalize().expect("canonical root");
        std::fs::create_dir_all(root.join(".git")).expect("repository");
        let script = root.join("fake-lsp.js");
        crate::tools::lsp_search::tests::write_executable(&script, SERVER.as_bytes());
        let anchor = root.join("a.go");
        std::fs::write(&anchor, "func helper() {}\nhelper()\n").expect("anchor");
        std::fs::write(
            root.join("lsp-servers.json"),
            json!({"languageServers": {".go": {
                "command": script.to_string_lossy(), "args": [], "languageId": "go"
            }}})
            .to_string(),
        )
        .expect("config");
        let execution = crate::tools::lsp_search::LspExecutionConfig {
            octocode_home: Some(root.clone()),
            ..crate::tools::lsp_search::LspExecutionConfig::default()
        };
        let pool = LspClientPool::new(LspPoolOptions {
            client: LspClientOptions {
                settle_cap: Some(Duration::ZERO),
            },
            ..LspPoolOptions::default()
        });
        let paths = crate::tools::test_support::workspace_policy(&root);
        Self {
            path: anchor.to_string_lossy().into_owned(),
            root,
            pool,
            runtime: tokio::runtime::Runtime::new().expect("rt"),
            execution,
            paths,
            _dir: dir,
        }
    }

    fn run(&self, row: Value) -> Value {
        self.runtime
            .block_on(crate::tools::lsp_search::execute(
                query(row),
                &NeverCancel,
                &self.pool,
                &self.paths,
                &self.execution,
                None,
                crate::tools::lsp_search::test_answers(),
            ))
            .expect("row")
    }

    fn references(&self) -> Value {
        self.run(json!({"operation": "references", "path": self.path, "symbolName": "helper", "line": 1}))
    }

    fn events(&self) -> String {
        std::fs::read_to_string(self.root.join("events.txt")).unwrap_or_default()
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.runtime.block_on(self.pool.clear_all());
    }
}

#[test]
fn a_change_between_the_first_and_second_request_reaches_the_server() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let watching = Watching::new();
    // The server answers only once the client has recorded its registration,
    // which is after the first request's own sync looked: the first request
    // must still leave a snapshot for the second to compare with.
    watching.references();
    std::fs::write(watching.root.join("b.go"), "helper()\n").expect("new file");
    octocode_engine::stamp::settle_written_so_far();
    watching.references();
    assert!(
        watching.events().contains("b.go"),
        "the second request reports the change: {}",
        watching.events()
    );
}

#[test]
fn an_outline_walks_no_scope_and_a_search_still_reports_changes() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let watching = Watching::new();
    watching.references();
    watching.references();
    std::fs::write(watching.root.join("b.go"), "helper()\n").expect("new file");
    octocode_engine::stamp::settle_written_so_far();
    watching.run(json!({"operation": "documentSymbols", "path": watching.path}));
    assert_eq!(watching.events(), "", "an outline walks no scope");
    watching.references();
    assert!(
        watching.events().contains("b.go"),
        "a search still reports the change: {}",
        watching.events()
    );
}
