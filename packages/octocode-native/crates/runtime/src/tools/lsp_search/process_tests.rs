//! Process-level fixtures: a real child language server (a Node script) that
//! crashes mid-request, and an acquire/stop loop that must leave no child
//! processes behind. Unix-only (process groups, `kill -0`).
//!
//! This module holds the shared fixture server, workspace, server config and
//! row helpers; each behavior's tests live in a child module.

use super::failure::LspFailure;
use super::tests::{FAKE_SERVER_CLIENT, fake_server_pool};
use crate::id::ToolId;
use crate::tools::lsp_search::lsp_search_query;
use crate::tools::test_support::checks::{Page, Walker, assert_absent, paths_in};
use crate::tools::test_support::{Fixture, assert_row_contract, node_available, require_node};
use octocode_contract_types::{ErrorCode, LspSearchQuery};
use octocode_engine::error::ErrorKind;
use octocode_engine::lsp::client::NativeLspClient;
use octocode_engine::lsp::pool::{LspClientPool, LspPoolOptions};
use octocode_engine::lsp::types::LspLanguageServerConfig;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod aliases;
mod definition_hops;
mod definitions;
mod document_sync;
mod fallback_graph;
mod heritage_fallback;
mod importers;
mod lifecycle;
mod receipts;
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

/// Whether a row names a file ending in `name` under a `path` key.
fn lists_path(row: &serde_json::Value, name: &str) -> bool {
    paths_in(row).iter().any(|path| path.ends_with(name))
}

/// `published` without its optional same-query related reattempt, so a page
/// walk only follows required continuations.
fn without_rerun(mut published: serde_json::Value, query: &LspSearchQuery) -> serde_json::Value {
    crate::tools::test_support::checks::assert_leads_valid(&published);
    if let Some(related) = published
        .pointer_mut("/results/0/data/related")
        .and_then(serde_json::Value::as_array_mut)
    {
        related.retain(|call| {
            let rerun = call["kind"] == "related"
                && call["tool"] == "lspSearch"
                && crate::tools::test_support::opened_input(ToolId::LspSearch, &call["input"])
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
