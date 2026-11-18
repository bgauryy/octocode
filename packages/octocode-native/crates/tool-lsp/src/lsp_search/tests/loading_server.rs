//! Absence needs evidence: an empty answer from a server that has not
//! finished loading is no claim that a symbol has no locations. Each fake
//! server answers `[]` until its load ends; what differs is when it says so.

use super::*;
use octocode_engine::lsp::client::LspClientOptions;
use octocode_engine::lsp::pool::{LspClientPool, LspPoolOptions};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_test_support::json_fixture;
use serde_json::Value;
use std::time::Duration;

/// `const scenario = ...` replaces this line.
const SCENARIO: &str = "/*SCENARIO*/";

const LOADING_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
/*SCENARIO*/
let buf = Buffer.alloc(0);
let loaded = scenario === 'loadAfterTheFirstAnswerUntilReleased';
let workers = [];
let asked = 0;
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
const progress = (value) => send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'load', value } });
const begin = () => progress({ kind: 'begin', title: 'Loading' });
const status = (health, message) => send({ jsonrpc: '2.0', method: 'experimental/serverStatus', params: { health, quiescent: true, message } });
function finish() { loaded = true; workers.forEach((w) => w.terminate()); workers = []; progress({ kind: 'end' }); }
// Busy threads: a loaded machine gives each a slice, so the process tree's CPU
// time shows work however starved the scheduler leaves any one thread.
function burn() {
  const { Worker } = require('worker_threads');
  for (let i = 0; i < 4; i++) workers.push(new Worker('for (;;) {}', { eval: true }));
}
function firstAnswer() {
  if (scenario === 'loadAnnouncedWithTheAnswer') { begin(); setTimeout(finish, 300); }
  if (scenario === 'loadOverBeforeTheAnswer') { begin(); finish(); }
  if (scenario === 'loadNeverEnds') { begin(); }
  if (scenario === 'loadAnnouncedAfterTheAnswer') {
    setTimeout(() => { begin(); setTimeout(finish, 100); }, 2500);
  }
  // Once the first answer is out, a load begins that lasts until the test releases it.
  if (scenario === 'loadAfterTheFirstAnswerUntilReleased') {
    setImmediate(() => { loaded = false; reportingLoadUntilReleased(); });
  }
}
// A load that reports its work every 200 ms and ends after `ms`.
function reportingLoad(ms) {
  begin();
  const ticks = setInterval(() => progress({ kind: 'report', message: 'indexing' }), 200);
  setTimeout(() => { clearInterval(ticks); finish(); }, ms);
}
// A load that reports its work every 200 ms until the test writes `released`.
function reportingLoadUntilReleased() {
  begin();
  const ticks = setInterval(() => {
    if (fs.existsSync(__dirname + '/released')) { clearInterval(ticks); finish(); return; }
    progress({ kind: 'report', message: 'indexing' });
  }, 200);
}
function handle(msg) {
  if (msg.method === 'initialize') {
    fs.appendFileSync(__dirname + '/started.txt', 'started\n');
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { definitionProvider: true, referencesProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'initialized' && scenario === 'reportingLoadOf3s') {
    reportingLoad(3000);
  } else if (msg.method === 'initialized' && scenario === 'loadAnnouncedAfterTheAnswer') {
    burn();
  } else if (msg.method === 'initialized' && scenario === 'healthError') {
    status('error', 'Failed to load workspaces: unparsable Cargo.toml');
  } else if (msg.method === 'initialized' && scenario === 'healthWarning') {
    status('warning', 'Failed to run build scripts of some packages.');
  } else if (msg.method === 'initialized' && scenario === 'healthUnexplained') {
    status('error');
  } else if (msg.method === 'initialized' && scenario === 'healthRecovers') {
    status('error', 'Failed to load workspaces: unparsable Cargo.toml');
  } else if (msg.method === 'textDocument/definition') {
    asked += 1;
    if (scenario === 'healthRecovers' && asked === 2) status('ok');
    fs.appendFileSync(__dirname + '/asked.txt', 'definition\n');
    const known = loaded;
    if (asked === 1) firstAnswer();
    if (scenario === 'everyRequestReportsProgress') { begin(); progress({ kind: 'end' }); }
    send({ jsonrpc: '2.0', id: msg.id, result: known
      ? [{ uri: msg.params.textDocument.uri, range: { start: { line: 0, character: 9 }, end: { line: 0, character: 15 } } }]
      : [] });
  } else if (msg.method === 'textDocument/references') {
    asked += 1;
    fs.appendFileSync(__dirname + '/asked.txt', 'references\n');
    const known = loaded;
    if (asked === 1) firstAnswer();
    const site = (line, character) => ({ uri: msg.params.textDocument.uri, range: { start: { line, character }, end: { line, character: character + 6 } } });
    send({ jsonrpc: '2.0', id: msg.id, result: known ? [site(0, 9), site(1, 0)] : [site(0, 9)] });
  } else if (msg.method === 'exit') {
    process.exit(0);
  } else if (msg.id !== undefined && msg.method) {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
  }
}
// The client closing the pipe ends the server, and with it the burn threads.
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

mod server_health;

struct Loading {
    _dir: octocode_runtime_base::test_support::Fixture,
    root: std::path::PathBuf,
    execution: crate::lsp_search::LspExecutionConfig,
    pool: LspClientPool,
    runtime: tokio::runtime::Runtime,
}

impl Loading {
    /// A workspace whose `.ts` files go to the `scenario` server, under
    /// `language`'s readiness budget, with every settle at most `settle`.
    fn new(scenario: &str, language: &str, settle: Duration) -> Self {
        let dir = octocode_runtime_base::test_support::Fixture::new();
        let root = dir.0.canonicalize().expect("canonical root");
        // A repository bounds the project: these tests judge the server's answer.
        std::fs::create_dir_all(root.join(".git")).expect("repository");
        let script = root.join("fake-lsp.js");
        let server = LOADING_SERVER.replace(SCENARIO, &format!("const scenario = '{scenario}';"));
        crate::lsp_search::tests::write_executable(&script, server.as_bytes());
        std::fs::write(root.join("a.ts"), "function helper() {}\nhelper();\n").expect("source");
        std::fs::write(
            root.join("lsp-servers.json"),
            json!({"languageServers": {".ts": {
                "command": script.to_string_lossy(), "args": [], "languageId": language
            }}})
            .to_string(),
        )
        .expect("config");
        let execution = crate::lsp_search::LspExecutionConfig {
            octocode_home: Some(root.clone()),
            ..crate::lsp_search::LspExecutionConfig::with_path()
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
        self.definition_within(None)
    }

    /// [`Self::definition`] for a caller that gives up `deadline` from now.
    fn definition_within(&self, deadline: Option<Duration>) -> Value {
        let paths = octocode_runtime_base::test_support::workspace_policy(&self.root);
        self.runtime
            .block_on(async {
                let deadline = deadline.map(|after| tokio::time::Instant::now() + after);
                octocode_engine::lsp::with_caller_deadline(
                    deadline,
                    crate::lsp_search::execute(
                        self.query(),
                        &NeverCancel,
                        &self.pool,
                        &paths,
                        &self.execution,
                        None,
                        crate::lsp_search::test_answers(),
                    ),
                )
                .await
            })
            .expect("row")
    }

    /// How many times a server was started.
    fn started(&self) -> usize {
        std::fs::read_to_string(self.root.join("started.txt"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    /// How many times the server was asked for `method` (`references`).
    fn asked_for(&self, method: &str) -> usize {
        std::fs::read_to_string(self.root.join("asked.txt"))
            .unwrap_or_default()
            .lines()
            .filter(|asked| *asked == method)
            .count()
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
        row.pointer("/files/0/matches/0/line"),
        Some(&json!(1)),
        "{row}"
    );
}

/// Its load is under way when the server answers: the answer is asked again
/// once the load ends, so the row holds what the loaded server knows.
#[test]
fn an_empty_answer_during_a_load_is_asked_again_after_it() {
    if !octocode_runtime_base::test_support::require_node() {
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
    if !octocode_runtime_base::test_support::require_node() {
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
    if !octocode_runtime_base::test_support::require_node() {
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
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let server = Loading::new("loadNeverEnds", "shellscript", Duration::ZERO);
    let row = server.definition();
    assert_eq!(row["category"], "languageServerIndexing", "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["languageServerIndexing"]"#)
    );
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

/// A load that outlasts the language's readiness budget while it keeps
/// reporting its work is waited for within the call: the first answer is
/// the loaded server's, with no fixed wait cutting it short.
#[test]
fn a_load_that_keeps_working_is_waited_for_within_the_call() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let server = Loading::new("reportingLoadOf3s", "shellscript", Duration::ZERO);
    assert_located(&server.definition());
    assert_eq!(server.started(), 1);
}

/// The caller's deadline comes while the load still works: the row says how
/// long the call waited and offers the same query again, and that retry
/// reaches the same server, still loading, which answers once it is done.
/// The server is started and answering before the call with the deadline,
/// and its load lasts until the test releases it: the deadline is spent
/// waiting for the load, never on the server's start.
#[test]
fn a_load_past_the_deadline_is_retried_on_the_same_server() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let server = Loading::new(
        "loadAfterTheFirstAnswerUntilReleased",
        "shellscript",
        Duration::ZERO,
    );
    assert_located(&server.definition());
    let row = server.definition_within(Some(Duration::from_secs(5)));
    assert_eq!(row["category"], "languageServerIndexing", "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["languageServerIndexing"]"#),
        "{row}"
    );
    let reason = row["reason"].as_str().unwrap_or_default();
    assert!(reason.contains(" s;"), "the row states the wait: {row}");
    let rerun = super::super::failure::same_query_rerun(&server.query());
    assert!(
        row["related"]
            .as_array()
            .expect("related")
            .iter()
            .any(|call| call["input"]["queries"][0] == rerun),
        "{row}"
    );
    std::fs::write(server.root.join("released"), "").expect("release the load");
    assert_located(&server.definition());
    assert_eq!(server.started(), 1, "the retry reuses the loading server");
}

/// A server may wrap each request in its own progress, as pyright does for
/// references. That is no load: the answer is asked again at most once, then
/// stands, and a repeat is asked once.
#[test]
fn progress_around_each_request_is_not_a_load() {
    if !octocode_runtime_base::test_support::require_node() {
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
    if !octocode_runtime_base::test_support::require_node() {
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

impl Loading {
    fn references(&self) -> Value {
        let paths = octocode_runtime_base::test_support::workspace_policy(&self.root);
        let query = query(json!({
            "operation": "references",
            "path": self.root.join("a.ts").to_string_lossy(),
            "symbolName": "helper",
            "line": 1
        }));
        self.runtime
            .block_on(crate::lsp_search::execute(
                query,
                &NeverCancel,
                &self.pool,
                &paths,
                &self.execution,
                None,
                crate::lsp_search::test_answers(),
            ))
            .expect("row")
    }
}

/// A server still loading lists the declaration it can see and no use: that
/// reads as "nothing calls this" as an empty answer does, so it is judged the
/// same way and asked again once the load ends.
#[test]
fn a_declaration_only_answer_during_a_load_is_asked_again_after_it() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let server = Loading::new("loadAnnouncedWithTheAnswer", "go", Duration::ZERO);
    let row = server.references();
    assert_eq!(row["matchCount"], 2, "the use is listed: {row}");
    assert_ne!(row["isPartial"], true, "{row}");
    assert_eq!(server.asked(), 2, "{row}");
}

/// A load that outlasts the budget leaves the declaration on the row, partial:
/// the evidence stays, and it is no claim that nothing uses the name.
#[test]
fn a_declaration_only_answer_past_the_budget_stays_partial_with_its_evidence() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let server = Loading::new("loadNeverEnds", "shellscript", Duration::ZERO);
    let row = server.references();
    assert_eq!(row["matchCount"], 1, "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    // The spelling on line 2 the server never lists is named too. A server
    // that has loaded nothing cannot say what it names, so it is unverified
    // and adds no reason of its own.
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["languageServerIndexing"]"#)
    );
    let warnings = row["warnings"].to_string();
    assert!(
        warnings.contains("a.ts:2") && warnings.contains("unverified"),
        "{row}"
    );
}

/// An idle server's declaration-only answer stands: it is asked once, not
/// judged a load. The spelling it does not list in the file it answers
/// (`helper();` on line 2) is named. This server resolves no definition, so
/// nothing says the line names the anchor: it is unverified, and the row
/// leads to the text of the file.
#[test]
fn a_declaration_only_answer_from_an_idle_server_stands() {
    if !octocode_runtime_base::test_support::require_node() {
        return;
    }
    let server = Loading::new("silent", "go", Duration::from_millis(300));
    let row = server.references();
    assert_eq!(row["matchCount"], 1, "{row}");
    assert!(row.get("partialReasons").is_none(), "{row}");
    let warnings = row["warnings"].to_string();
    assert!(
        warnings.contains("a.ts:2") && warnings.contains("unverified"),
        "{row}"
    );
    octocode_runtime_base::test_support::checks::related_call(&row, "localSearch", |_| true);
    assert_eq!(server.asked_for("references"), 1, "{row}");
}
