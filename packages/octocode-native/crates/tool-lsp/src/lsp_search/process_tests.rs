//! Process-level fixtures: a real child language server (a Node script) that
//! crashes mid-request, and an acquire/stop loop that must leave no child
//! processes behind. Unix-only (process groups, `kill -0`).
//!
//! This module holds the shared fixture server, workspace, server config and
//! row helpers; each behavior's tests live in a child module.

use super::failure::LspFailure;
use super::tests::{FAKE_SERVER_CLIENT, fake_server_pool};
use crate::lsp_search::lsp_search_query;
use octocode_contract_types::{ErrorCode, LspSearchQuery};
use octocode_engine::error::ErrorKind;
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::pool::{LspClientPool, LspPoolOptions};
use octocode_engine::lsp::types::LspLanguageServerConfig;
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::test_support::checks::{
    Page, Walker, assert_absent, location_rows, paths_in,
};
use octocode_runtime_base::test_support::{
    Fixture, assert_row_contract, node_available, require_node,
};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod aliases;
mod cold_start;
mod definition_hops;
mod definitions;
mod document_sync;
mod fallback_graph;
mod first_diagnostics;
mod heritage_fallback;
mod importers;
mod legacy_text;
mod lifecycle;
mod receipts;
mod server_text;
mod warm_repeat;
mod workspace_symbols;

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
    let dir = Fixture::new();
    let root = dir.0.canonicalize().expect("canonical root");
    let script = root.join("fake-lsp.js");
    crate::lsp_search::tests::write_executable(&script, FAKE_SERVER.as_bytes());
    std::fs::write(root.join("a.ts"), "const a = 1;\n").expect("source");
    (dir, root, script)
}

fn config(root: &Path, script: &Path, pids: Option<&Path>) -> LspLanguageServerConfig {
    LspLanguageServerConfig {
        command: script.to_string_lossy().into_owned(),
        args: Some(Vec::new()),
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("plaintext".into()),
        document_language_ids: Default::default(),
        initialization_options: None,
        // The server runs under what it is given: `PATH` for `node`.
        env: Some(
            std::env::var("PATH")
                .into_iter()
                .map(|path| ("PATH".to_owned(), path))
                .chain(pids.map(|pids| {
                    (
                        "FAKE_LSP_PIDS".to_owned(),
                        pids.to_string_lossy().into_owned(),
                    )
                }))
                .collect(),
        ),
        max_memory_mb: None,
        source: Default::default(),
    }
}

/// Whether a row names a file ending in `name` under a `path` key.
fn lists_path(row: &serde_json::Value, name: &str) -> bool {
    paths_in(row).iter().any(|path| path.ends_with(name))
}

/// `published` without its optional same-query related reattempt, so a page
/// walk only follows required continuations.
fn without_rerun(mut published: serde_json::Value, query: &LspSearchQuery) -> serde_json::Value {
    octocode_runtime_base::test_support::checks::assert_leads_valid(&published);
    if let Some(related) = published
        .pointer_mut("/results/0/data/related")
        .and_then(serde_json::Value::as_array_mut)
    {
        related.retain(|call| {
            let rerun = call["kind"] == "related"
                && call["tool"] == "lspSearch"
                && octocode_runtime_base::test_support::opened_input(
                    ToolId::LspSearch,
                    &call["input"],
                )
                .get("queries")
                .and_then(serde_json::Value::as_array)
                .and_then(|queries| queries.first())
                .is_some_and(|rerun| rerun == &super::failure::same_query_rerun(query));
            !rerun
        });
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

/// A stand-in server for a repository fixture at `root`: `answers` is the
/// body of `function result(msg)` (with `p = msg.params`, `at(line,
/// character, length)` and `file(name)`, the URI of `root/name`), and every
/// `didOpen` is recorded in `opened.txt` beside the script.
fn recording_server(root: &Path, answers: &str) -> String {
    const FRAME: &str = r#"#!/usr/bin/env node
const fs = require('fs');
const root = __ROOT__;
const file = (name) => 'file://' + root + '/' + name;
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function result(msg) {
  const p = msg.params || {};
  __ANSWERS__
}
function handle(msg) {
  if (msg.method === 'exit') process.exit(0);
  if (msg.method === 'textDocument/didOpen') fs.appendFileSync(__dirname + '/opened.txt', msg.params.textDocument.uri + '\n');
  if (msg.id === undefined || !msg.method) return;
  send({ jsonrpc: '2.0', id: msg.id, result: result(msg) });
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
    FRAME
        .replace(
            "__ROOT__",
            &serde_json::to_string(&root.to_string_lossy()).expect("root literal"),
        )
        .replace("__ANSWERS__", answers)
}

/// A repository (`.git`) fixture: `files` (path, text) under `repo/`, and
/// `answers` (see [`recording_server`]) serving every extension `extensions`
/// names as `language_id`. Returns the repository root, the file listing
/// what the server opened, and the execution config.
fn repository_fixture(
    files: &[(&str, &str)],
    answers: &str,
    (extensions, language_id): (&[&str], &str),
) -> (
    Fixture,
    PathBuf,
    PathBuf,
    crate::lsp_search::LspExecutionConfig,
) {
    let dir = Fixture::new();
    let outer = dir.0.canonicalize().expect("canonical fixture");
    let root = outer.join("repo");
    std::fs::create_dir_all(root.join(".git")).expect("repository");
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(&path, text).expect("write");
    }
    let script = outer.join("fake-lsp.js");
    crate::lsp_search::tests::write_executable(&script, recording_server(&root, answers));
    let server = serde_json::json!({
        "command": script.to_string_lossy(), "args": [], "languageId": language_id
    });
    let servers = extensions
        .iter()
        .map(|extension| ((*extension).to_owned(), server.clone()))
        .collect::<serde_json::Map<_, _>>();
    std::fs::write(
        root.join("lsp-servers.json"),
        serde_json::json!({ "languageServers": servers }).to_string(),
    )
    .expect("config");
    let execution = crate::lsp_search::LspExecutionConfig {
        octocode_home: Some(root.to_path_buf()),
        ..crate::lsp_search::LspExecutionConfig::with_path()
    };
    octocode_engine::stamp::settle_written_so_far();
    (dir, root, outer.join("opened.txt"), execution)
}

/// One `query` row run to its answer on a fresh pool, checked against the
/// output contract.
fn run_once(
    root: &Path,
    execution: &crate::lsp_search::LspExecutionConfig,
    query: serde_json::Value,
) -> serde_json::Value {
    let query = serde_json::from_value(query).expect("query");
    let pool = fake_server_pool();
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let row = runtime
        .block_on(crate::lsp_search::execute(
            query,
            &octocode_runtime_base::execution::cancel::NeverCancel,
            &pool,
            &paths,
            execution,
            None,
            crate::lsp_search::test_answers(),
        ))
        .expect("row");
    runtime.block_on(pool.clear_all());
    assert_row_contract(ToolId::LspSearch, &row);
    row
}
