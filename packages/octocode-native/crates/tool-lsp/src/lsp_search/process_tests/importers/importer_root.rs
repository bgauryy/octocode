//! A caller the server answers from a verified importer's own root is the
//! server's row: reference-derived callers fill in only the files no level-1
//! answer covers, so the same caller is never listed twice.

use super::*;
use octocode_test_support::json_fixture;

/// A TypeScript-like server whose project sees only the anchor file: the
/// anchor's call hierarchy answers nothing, while `b.ts` prepares its own
/// item (the import binding, a root of its own) whose `incomingCalls` names
/// `useIt` at the call. `references` from `b.ts` lists the binding and the
/// call, so a reference-derived caller would name `useIt` again.
const IMPORTER_ROOT_SERVER: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const declUri = 'file://' + root + '/a.ts';
const importerUri = 'file://' + root + '/b.ts';
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const decl = { uri: declUri, range: at(0, 16, 6) };
const binding = { name: 'target', kind: 12, uri: importerUri, range: at(0, 9, 6), selectionRange: at(0, 9, 6) };
const call = at(2, 2, 6);
const useIt = { name: 'useIt', kind: 12, uri: importerUri, range: at(1, 0, 40), selectionRange: at(1, 16, 5) };
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
      return p.textDocument.uri === importerUri
        ? [{ uri: importerUri, range: binding.selectionRange }, { uri: importerUri, range: call }]
        : [decl];
    case 'textDocument/prepareCallHierarchy':
      return p.textDocument.uri === importerUri
        ? [binding]
        : [{ name: 'target', kind: 12, uri: declUri, range: at(0, 0, 28), selectionRange: decl.range }];
    case 'callHierarchy/incomingCalls':
      return p.item.uri === importerUri ? [{ from: useIt, fromRanges: [call] }] : [];
    case 'textDocument/documentSymbol':
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

/// LSP-10: the importer's root answers `useIt` at level 1, and the same call
/// is a reference of the importer: one `useIt` row, its one site once, and
/// no reference-derived row beside it.
#[test]
fn a_caller_an_importer_root_answers_is_not_derived_again() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_serving(&[], IMPORTER_ROOT_SERVER);
    std::fs::write(
        root.join("b.ts"),
        "import { target } from './a';\nexport function useIt() {\n  target();\n}\n",
    )
    .expect("importer");
    octocode_engine::stamp::settle_written_so_far();
    let pool = fake_server_pool();
    let query = serde_json::from_value(serde_json::json!({
        "operation": "callers", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    }))
    .expect("query");
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(crate::lsp_search::execute(
            query,
            &octocode_runtime_base::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::lsp_search::test_answers(),
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &row);
    assert_eq!(
        super::call_sites::callers_of(&row),
        vec![(
            "useIt".to_owned(),
            json_fixture(r#"[{"line": 3, "column": 3}]"#)
        )],
        "{row}"
    );
    let derived = row["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| group["matches"].as_array().cloned().unwrap_or_default())
        .filter(|caller| caller.get("source").is_some())
        .count();
    assert_eq!(derived, 0, "the server's answer stands alone: {row}");
}
