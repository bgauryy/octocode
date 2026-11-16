//! Source text the server is sent when the file is not UTF-8: the decoded
//! text of the one engine rule, stated by a warning, and a binary file as
//! its own refusal.

use super::*;

/// Logs the text of each `didOpen` (as JSON) and answers `documentSymbol`
/// with one function symbol per `fn` line of that text, at its decoded column.
const OUTLINE_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
const log = __LOG__;
let buf = Buffer.alloc(0);
let opened = '';
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function symbols() {
  return opened.split('\n').flatMap((text, line) => {
    const at = text.indexOf('fn ');
    if (at < 0) return [];
    const name = /fn ([^(]+)/.exec(text)[1];
    const range = { start: { line, character: at }, end: { line, character: text.length } };
    const selection = { start: { line, character: at + 3 }, end: { line, character: at + 3 + name.length } };
    return [{ name, kind: 12, range, selectionRange: selection }];
  });
}
function handle(msg) {
  if (msg.method === 'exit') process.exit(0);
  const p = msg.params || {};
  if (msg.method === 'textDocument/didOpen') {
    opened = p.textDocument.text;
    fs.appendFileSync(log, JSON.stringify(opened) + '\n');
  }
  if (msg.id === undefined || !msg.method) return;
  const result = msg.method === 'initialize'
    ? { capabilities: { textDocumentSync: 1, documentSymbolProvider: true } }
    : msg.method === 'textDocument/documentSymbol' ? symbols() : null;
  send({ jsonrpc: '2.0', id: msg.id, result });
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

/// The `documentSymbols` row of a file holding `bytes`, and the texts the
/// server was sent in `didOpen`.
fn outline_of(bytes: &[u8]) -> (serde_json::Value, Vec<String>) {
    let dir = Fixture::new();
    let outer = dir.0.canonicalize().expect("canonical fixture");
    let root = outer.join("ws");
    std::fs::create_dir_all(&root).expect("workspace");
    let log = outer.join("opened.log");
    let literal = |value: &Path| serde_json::to_string(&value.to_string_lossy()).expect("literal");
    let script = outer.join("fake-outline-lsp.js");
    crate::tools::lsp_search::tests::write_executable(
        &script,
        OUTLINE_SERVER.replace("__LOG__", &literal(&log)).as_bytes(),
    );
    let file = root.join("a.ts");
    std::fs::write(&file, bytes).expect("source");
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": "outline"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        octocode_home: Some(root.to_path_buf()),
        ..crate::tools::lsp_search::LspExecutionConfig::with_path()
    };
    let query = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "path": file.to_string_lossy(),
        "workspaceRoot": root.to_string_lossy()
    }))
    .expect("query");
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &fake_server_pool(),
            &crate::tools::test_support::workspace_policy(&root),
            &execution,
            None,
            crate::tools::lsp_search::test_answers(),
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &row);
    let opened = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("logged text"))
        .collect();
    (row, opened)
}

/// Latin-1 text is sent decoded, one char per byte, with the engine's
/// warning; the symbol lines and columns count the decoded text, which keeps
/// the raw file's lines and counts one column per raw byte.
#[test]
fn a_latin1_file_is_synced_decoded_with_the_warning() {
    if !require_node() {
        return;
    }
    let raw = b"fn caf\xe9() {}\nfn z() {}\n";
    let (row, opened) = outline_of(raw);
    assert_eq!(opened, ["fn caf\u{e9}() {}\nfn z() {}\n"], "{row}");
    assert!(
        row["warnings"].as_array().is_some_and(|warnings| warnings
            .iter()
            .any(|w| w == octocode_engine::text::LATIN1_WARNING)),
        "{row}"
    );
    let symbols = row["symbols"].as_array().expect("symbols");
    let lines: Vec<_> = symbols
        .iter()
        .map(|symbol| symbol["line"].as_u64())
        .collect();
    // Symbol lines are the raw file's lines: `z` is on line 2 of the bytes.
    assert_eq!(lines, [Some(1), Some(2)], "{row}");
    assert_eq!(
        raw.split(|byte| *byte == b'\n').nth(1),
        Some(&b"fn z() {}"[..])
    );
}

/// UTF-8 text with stray bytes is sent with U+FFFD at the stray bytes and the
/// same lossy warning a read gives.
#[test]
fn a_file_with_stray_bytes_is_synced_lossy_with_the_warning() {
    if !require_node() {
        return;
    }
    let (row, opened) = outline_of(b"// \xc3\xa9 \xff\nfn z() {}\n");
    assert_eq!(opened, ["// \u{e9} \u{fffd}\nfn z() {}\n"], "{row}");
    assert!(
        row["warnings"].as_array().is_some_and(|warnings| warnings
            .iter()
            .any(|w| w == octocode_engine::text::LOSSY_WARNING)),
        "{row}"
    );
}

/// A binary file is not source text: the row says so under the code every
/// reader gives it, with no read to retry and no text-encoding advice.
#[test]
fn a_binary_file_is_refused_as_binary_not_as_a_failed_read() {
    if !require_node() {
        return;
    }
    let (row, opened) = outline_of(b"fn a() {}\0\x01\x02");
    assert!(opened.is_empty(), "no document is synced: {opened:?}");
    assert_eq!(row["errorCode"], "binaryFileUnsupported", "{row}");
    assert!(row.get("related").is_none(), "{row}");
    assert!(!row.to_string().contains("text encoding"), "{row}");
}
