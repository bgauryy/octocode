//! Reference-derived callers keep only TypeScript call sites (#480): an
//! implementing method's own declaration is never its caller.

use super::*;

/// A TypeScript-like server whose project holds only `port.ts` (the
/// anchor): from `ledger.ts`, `references` lists the implementing
/// declaration `read(at: number)` (3:3), the call `p.read(2)` (6:12) and the
/// interface member; its call hierarchy answers nothing.
const LEDGER_SERVER: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const declUri = 'file://' + root + '/port.ts';
const ledgerUri = 'file://' + root + '/ledger.ts';
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const decl = { uri: declUri, range: at(0, 24, 4) };
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
    case 'textDocument/definition':
      return [decl];
    case 'textDocument/references':
      if (p.textDocument.uri === declUri) return [decl];
      return [{ uri: ledgerUri, range: at(2, 2, 4) }, { uri: ledgerUri, range: at(5, 11, 4) }, decl];
    case 'textDocument/prepareCallHierarchy':
      if (p.textDocument.uri !== declUri) return [];
      return [{ name: 'read', kind: 6, uri: declUri, range: at(0, 24, 25), selectionRange: decl.range }];
    case 'textDocument/documentSymbol':
      if (p.textDocument.uri !== ledgerUri) return [];
      return [
        { name: 'read', kind: 6, range: at(2, 2, 40), selectionRange: at(2, 2, 4) },
        { name: 'useLedger', kind: 12, range: { start: { line: 4, character: 0 }, end: { line: 6, character: 1 } }, selectionRange: at(4, 16, 9) },
      ];
    case 'callHierarchy/incomingCalls':
      return [];
    default:
      return null;
  }
}
function handle(msg) {
  if (msg.method === 'exit') process.exit(0);
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

/// The callers of `Port.read` as `(caller, sites)`, with `ledger.ts`
/// holding `ledger`.
fn ledger_callers(ledger: &str) -> Vec<(String, serde_json::Value)> {
    let (_dir, root, execution) = importer_fixture_serving(&[], LEDGER_SERVER);
    std::fs::write(
        root.join("port.ts"),
        "export interface Port { read(at: number): string; }\n",
    )
    .expect("port");
    std::fs::write(root.join("ledger.ts"), ledger).expect("ledger");
    let pool = fake_server_pool();
    let query = serde_json::from_value(serde_json::json!({
        "operation": "callers", "path": root.join("port.ts").to_string_lossy(),
        "symbolName": "read", "line": 1, "workspaceRoot": root.to_string_lossy()
    }))
    .expect("query");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::tools::lsp_search::test_answers(),
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &row);
    row["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| group["matches"].as_array().cloned().unwrap_or_default())
        .map(|caller| {
            (
                caller["symbolName"].as_str().unwrap_or_default().to_owned(),
                caller["sites"].clone(),
            )
        })
        .collect()
}

const LEDGER: &str = "import { Port } from \"./port\";\nexport class Ledger implements Port {\n  read(at: number): string { return String(at); }\n}\nexport function useLedger(p: Port): string {\n  return p.read(2);\n}\n";

fn only_the_call() -> Vec<(String, serde_json::Value)> {
    vec![(
        "useLedger".to_owned(),
        serde_json::json!([{"line": 6, "column": 12}]),
    )]
}

#[test]
fn an_implementing_declaration_is_not_a_caller() {
    if !require_node() {
        return;
    }
    assert_eq!(ledger_callers(LEDGER), only_the_call());
}

/// oxc rejects the whole file (a syntax error on the last line), so the
/// text rule decides every site: the declaration `read(` must still go.
#[test]
fn an_implementing_declaration_is_not_a_caller_in_a_file_oxc_rejects() {
    if !require_node() {
        return;
    }
    let broken = format!("{LEDGER}const broken = ;\n");
    assert_eq!(ledger_callers(&broken), only_the_call());
}
