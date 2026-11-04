//! Absence needs evidence: an empty answer from a server that has not
//! finished loading is no claim that a symbol has no locations. Each fake
//! server answers `[]` until its load ends; what differs is when it says so.

use super::*;
use crate::execution::cancel::NeverCancel;
use octocode_engine::lsp::client::LspClientOptions;
use octocode_engine::lsp::pool::{LspClientPool, LspPoolOptions};
use serde_json::Value;
use std::time::Duration;

/// `const scenario = ...` replaces this line.
const SCENARIO: &str = "/*SCENARIO*/";

const LOADING_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
/*SCENARIO*/
let buf = Buffer.alloc(0);
let loaded = false;
let burning = false;
let asked = 0;
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
const progress = (value) => send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'load', value } });
const begin = () => progress({ kind: 'begin', title: 'Loading' });
function finish() { loaded = true; burning = false; progress({ kind: 'end' }); }
function burn() {
  const until = Date.now() + 20;
  while (Date.now() < until);
  if (burning) setImmediate(burn);
}
function firstAnswer() {
  if (scenario === 'loadAnnouncedWithTheAnswer') { begin(); setTimeout(finish, 300); }
  if (scenario === 'loadOverBeforeTheAnswer') { begin(); finish(); }
  if (scenario === 'loadNeverEnds') { begin(); }
  if (scenario === 'loadAnnouncedAfterTheAnswer') {
    setTimeout(() => { begin(); setTimeout(finish, 100); }, 2500);
  }
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { definitionProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'initialized' && scenario === 'loadAnnouncedAfterTheAnswer') {
    burning = true;
    burn();
  } else if (msg.method === 'textDocument/definition') {
    asked += 1;
    fs.appendFileSync(__dirname + '/asked.txt', 'definition\n');
    const known = loaded;
    if (asked === 1) firstAnswer();
    if (scenario === 'everyRequestReportsProgress') { begin(); progress({ kind: 'end' }); }
    send({ jsonrpc: '2.0', id: msg.id, result: known
      ? [{ uri: msg.params.textDocument.uri, range: { start: { line: 0, character: 9 }, end: { line: 0, character: 15 } } }]
      : [] });
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

struct Loading {
    _dir: crate::tools::test_support::Fixture,
    root: std::path::PathBuf,
    execution: crate::tools::lsp_search::LspExecutionConfig,
    pool: LspClientPool,
    runtime: tokio::runtime::Runtime,
}

impl Loading {
    /// A workspace whose `.ts` files go to the `scenario` server, under
    /// `language`'s readiness budget, with every settle at most `settle`.
    fn new(scenario: &str, language: &str, settle: Duration) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = crate::tools::test_support::Fixture::new();
        let root = dir.0.canonicalize().expect("canonical root");
        let script = root.join("fake-lsp.js");
        let server = LOADING_SERVER.replace(SCENARIO, &format!("const scenario = '{scenario}';"));
        std::fs::write(&script, server).expect("script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        std::fs::write(root.join("a.ts"), "function helper() {}\nhelper();\n").expect("source");
        std::fs::write(
            root.join("lsp-servers.json"),
            json!({"languageServers": {".ts": {
                "command": script.to_string_lossy(), "args": [], "languageId": language
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
                settle_cap: Some(settle),
            },
            ..LspPoolOptions::default()
        });
        Self {
            _dir: dir,
            root,
            execution,
            pool,
            runtime: tokio::runtime::Runtime::new().expect("rt"),
        }
    }

    fn query(&self) -> LspSearchQuery {
        query(json!({
            "operation": "definition",
            "path": self.root.join("a.ts").to_string_lossy(),
            "symbolName": "helper",
            "line": 2
        }))
    }

    fn definition(&self) -> Value {
        let paths = crate::tools::test_support::workspace_policy(&self.root);
        self.runtime
            .block_on(crate::tools::lsp_search::execute(
                self.query(),
                &NeverCancel,
                &self.pool,
                &paths,
                &self.execution,
                None,
                crate::tools::lsp_search::test_answers(),
            ))
            .expect("row")
    }

    /// How many times the server was asked.
    fn asked(&self) -> usize {
        std::fs::read_to_string(self.root.join("asked.txt"))
            .unwrap_or_default()
            .lines()
            .count()
    }
}

impl Drop for Loading {
    fn drop(&mut self) {
        self.runtime.block_on(self.pool.clear_all());
    }
}

fn assert_located(row: &Value) {
    assert_ne!(row["status"], "empty", "{row}");
    assert_ne!(row["isPartial"], true, "{row}");
    assert_eq!(
        row.pointer("/files/0/matches/0/displayRange/startLine"),
        Some(&json!(1)),
        "{row}"
    );
}

/// Its load is under way when the server answers: the answer is asked again
/// once the load ends, so the row holds what the loaded server knows.
#[test]
fn an_empty_answer_during_a_load_is_asked_again_after_it() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("loadAnnouncedWithTheAnswer", "go", Duration::ZERO);
    assert_located(&server.definition());
}

/// The load started and finished after the question was asked and before
/// the wait for it began: nothing is open, yet the answer predates the end
/// of the load.
#[test]
fn an_empty_answer_older_than_a_finished_load_is_asked_again() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("loadOverBeforeTheAnswer", "go", Duration::ZERO);
    assert_located(&server.definition());
}

/// The server answers before it has said anything, as Metals does on a
/// loaded machine: every settle has run out, and only its working process
/// shows that it has not finished.
#[test]
fn an_empty_answer_before_the_first_signal_waits_while_the_server_works() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new(
        "loadAnnouncedAfterTheAnswer",
        "go",
        Duration::from_millis(300),
    );
    assert_located(&server.definition());
}

/// A load that outlasts the language's readiness budget ends the wait: the
/// row says the server is still loading and offers the same query again.
#[test]
fn a_load_that_outlasts_the_budget_is_not_read_as_absence() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("loadNeverEnds", "shellscript", Duration::ZERO);
    let row = server.definition();
    assert_eq!(row["category"], "languageServerIndexing", "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(row["partialReasons"], json!(["languageServerIndexing"]));
    let rerun = super::super::failure::same_query_rerun(&server.query());
    assert!(
        row["related"]
            .as_array()
            .expect("related")
            .iter()
            .any(|call| call["input"]["queries"][0] == rerun),
        "{row}"
    );
}

/// A server may wrap each request in its own progress, as pyright does for
/// references. That is no load: the answer is asked again at most once, then
/// stands, and a repeat is asked once.
#[test]
fn progress_around_each_request_is_not_a_load() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("everyRequestReportsProgress", "go", Duration::ZERO);
    let row = server.definition();
    assert_eq!(row["category"], "noLocations", "{row}");
    assert_ne!(row["isPartial"], true, "{row}");
    assert!(server.asked() <= 2, "asked {} times: {row}", server.asked());
    let before = server.asked();
    let row = server.definition();
    assert_eq!(row["category"], "noLocations", "{row}");
    assert_eq!(server.asked(), before + 1, "asked once: {row}");
}

/// A server that says nothing and does nothing has no load to wait for: an
/// empty answer stands, and once the server has been shown idle a repeat is
/// asked once and judged without another look.
#[test]
fn an_empty_answer_from_an_idle_silent_server_stands() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    let server = Loading::new("silent", "go", Duration::from_millis(300));
    for call in 1..=2 {
        let before = server.asked();
        let row = server.definition();
        assert_eq!(row["status"], "empty", "{row}");
        assert_eq!(row["category"], "noLocations", "{row}");
        assert_ne!(row["isPartial"], true, "{row}");
        if call == 2 {
            assert_eq!(server.asked(), before + 1, "asked once: {row}");
        }
    }
}
