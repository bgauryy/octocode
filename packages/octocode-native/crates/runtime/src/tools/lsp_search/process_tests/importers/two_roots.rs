//! Two call-hierarchy roots of one symbol (clangd prepares the declaration
//! and the definition) that both answer the same caller: the caller is one
//! level-1 row holding the union of both answers' sites.

use super::*;

/// A server whose `prepareCallHierarchy` at the anchor answers two items of
/// `target`; `incomingCalls` of each lists `useIt` in `b.ts`, with one site
/// of its own and one site both share.
const TWO_ROOTS_SERVER: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const declUri = 'file://' + root + '/a.ts';
const callerUri = 'file://' + root + '/b.ts';
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const first = { name: 'target', kind: 12, uri: declUri, range: at(0, 0, 28), selectionRange: at(0, 16, 6) };
const second = { name: 'target', kind: 12, uri: declUri, range: at(1, 0, 28), selectionRange: at(1, 16, 6) };
const caller = { name: 'useIt', kind: 12, uri: callerUri, range: at(1, 0, 60), selectionRange: at(1, 16, 5) };
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
      return [{ uri: declUri, range: first.selectionRange }];
    case 'textDocument/references':
      return [{ uri: declUri, range: first.selectionRange }];
    case 'textDocument/prepareCallHierarchy':
      return p.textDocument.uri === declUri ? [first, second] : [];
    case 'callHierarchy/incomingCalls': {
      const own = p.item.range.start.line === 0 ? at(2, 2, 6) : at(3, 2, 6);
      return [{ from: caller, fromRanges: [own, at(4, 2, 6)] }];
    }
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

#[test]
fn one_caller_of_two_roots_is_one_row_with_the_union_of_sites() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_serving(&[], TWO_ROOTS_SERVER);
    std::fs::write(
        root.join("b.ts"),
        "import { target } from './a';\nexport function useIt() {\n  target();\n  target();\n  target();\n}\n",
    )
    .expect("caller");
    let pool = fake_server_pool();
    let query = serde_json::from_value(serde_json::json!({
        "operation": "callers", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
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
    assert_eq!(
        super::call_sites::callers_of(&row),
        vec![(
            "useIt".to_owned(),
            serde_json::json!([
                {"line": 3, "column": 3},
                {"line": 4, "column": 3},
                {"line": 5, "column": 3}
            ])
        )],
        "{row}"
    );
}
