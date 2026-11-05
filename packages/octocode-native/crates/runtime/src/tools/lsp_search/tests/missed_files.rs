//! An answer that lists some uses can still miss a file: a sibling module the
//! server did not load, a crate outside its workspace. A file that spells the name
//! in code and is not in the answer makes the row partial, unless the server places
//! that spelling on another symbol.

use super::*;
use crate::execution::cancel::NeverCancel;
use octocode_engine::lsp::client::LspClientOptions;
use octocode_engine::lsp::pool::{LspClientPool, LspPoolOptions};
use serde_json::Value;
use std::time::Duration;

/// `const elsewhere = ...` replaces this line.
const BEHAVIOUR: &str = "/*BEHAVIOUR*/";

/// Answers `references` with the anchor file alone. `definition` in `b.*` names
/// `b.*` itself when `elsewhere` holds (another symbol of the name), and nothing
/// otherwise (a file it did not load).
const SERVER: &str = r#"#!/usr/bin/env node
/*BEHAVIOUR*/
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
const site = (uri, line, character) => ({ uri, range: { start: { line, character }, end: { line, character: character + 6 } } });
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { definitionProvider: true, referencesProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'textDocument/references') {
    const uri = msg.params.textDocument.uri;
    send({ jsonrpc: '2.0', id: msg.id, result: [site(uri, 0, 9), site(uri, 1, 0)] });
  } else if (msg.method === 'textDocument/definition') {
    const uri = msg.params.textDocument.uri;
    const known = !uri.includes('/b.') || elsewhere;
    const line = uri.includes('/b.') ? msg.params.position.line : 0;
    const character = uri.includes('/b.') ? msg.params.position.character : 9;
    send({ jsonrpc: '2.0', id: msg.id, result: known ? [site(uri, line, character)] : [] });
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

/// The row of `references helper` from `a.{extension}` with a `b.{extension}`
/// that calls `helper`, in a repository.
fn references(language: &str, extension: &str, elsewhere: bool) -> Value {
    use std::os::unix::fs::PermissionsExt;
    let dir = crate::tools::test_support::Fixture::new();
    let root = dir.0.canonicalize().expect("canonical root");
    std::fs::create_dir_all(root.join(".git")).expect("repository");
    let script = root.join("fake-lsp.js");
    let server = SERVER.replace(BEHAVIOUR, &format!("const elsewhere = {elsewhere};"));
    std::fs::write(&script, server).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let anchor = root.join(format!("a.{extension}"));
    std::fs::write(&anchor, "function helper() {}\nhelper();\n").expect("anchor");
    std::fs::write(root.join(format!("b.{extension}")), "helper();\n").expect("other file");
    std::fs::write(
        root.join("lsp-servers.json"),
        json!({"languageServers": {format!(".{extension}"): {
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
            settle_cap: Some(Duration::ZERO),
        },
        ..LspPoolOptions::default()
    });
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let row = runtime
        .block_on(crate::tools::lsp_search::execute(
            query(json!({
                "operation": "references",
                "path": anchor.to_string_lossy(),
                "symbolName": "helper",
                "line": 1
            })),
            &NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::tools::lsp_search::test_answers(),
        ))
        .expect("row");
    runtime.block_on(pool.clear_all());
    row
}

/// The languages with no coverage signal of their own, and Rust.
const CHECKED: [(&str, &str); 4] = [
    ("go", "go"),
    ("java", "java"),
    ("scala", "scala"),
    ("rust", "rs"),
];

#[test]
fn an_answer_listing_uses_is_partial_while_another_file_may_hold_one() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    for (language, extension) in CHECKED {
        let row = references(language, extension, false);
        assert_eq!(row["matchCount"], 2, "{language}: {row}");
        assert_eq!(row["isPartial"], true, "{language}: {row}");
        assert_eq!(
            row["partialReasons"],
            json!(["serverUnanswered"]),
            "{language}: {row}"
        );
        assert_ne!(
            row.pointer("/coverage/state"),
            Some(&json!("complete")),
            "{language}: {row}"
        );
    }
}

#[test]
fn an_answer_stays_complete_when_the_server_places_the_spelling_elsewhere() {
    if !crate::tools::test_support::require_node() {
        return;
    }
    for (language, extension) in CHECKED {
        let row = references(language, extension, true);
        assert_eq!(row["matchCount"], 2, "{language}: {row}");
        assert_ne!(row["isPartial"], true, "{language}: {row}");
    }
}
