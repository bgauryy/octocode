//! Reference-derived callers keep only TypeScript call sites (#480): an
//! implementing method's own declaration is never its caller.

use super::*;

/// A TypeScript-like server whose project holds only `port.ts` (the
/// anchor): from `ledger.ts`, `references` lists the implementing
/// declaration `read(at: number)` (3:3), the call `p.read(2)` (6:12) and the
/// interface member; its call hierarchy answers nothing.
///
/// `__DECL_SITE__` is the declaration's reference range and `__SYMBOLS__` the
/// `documentSymbol` answer (a JS expression); `{__error: true}` makes that
/// request fail.
const LEDGER_SERVER: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const declUri = 'file://' + root + '/port.ts';
const ledgerUri = 'file://' + root + '/ledger.ts';
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const decl = { uri: declUri, range: at(0, 24, 4) };
const declSite = __DECL_SITE__;
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
      return [{ uri: ledgerUri, range: declSite }, { uri: ledgerUri, range: at(5, 11, 4) }, decl];
    case 'textDocument/prepareCallHierarchy':
      if (p.textDocument.uri !== declUri) return [];
      return [{ name: 'read', kind: 6, uri: declUri, range: at(0, 24, 25), selectionRange: decl.range }];
    case 'textDocument/documentSymbol':
      if (p.textDocument.uri !== ledgerUri) return [];
      return __SYMBOLS__;
    case 'callHierarchy/incomingCalls':
      return [];
    default:
      return null;
  }
}
function handle(msg) {
  if (msg.method === 'exit') process.exit(0);
  if (msg.id === undefined || !msg.method) return;
  const answer = result(msg);
  if (answer && answer.__error) {
    send({ jsonrpc: '2.0', id: msg.id, error: { code: -32603, message: 'outline failed' } });
    return;
  }
  send({ jsonrpc: '2.0', id: msg.id, result: answer });
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

/// The outline of `ledger.ts` the server answers by default: a nested
/// `DocumentSymbol` list whose `read` selection is the declaration site.
const NESTED_OUTLINE: &str = "[\
  { name: 'read', kind: 6, range: at(2, 2, 40), selectionRange: declSite },\
  { name: 'useLedger', kind: 12, range: { start: { line: 4, character: 0 }, end: { line: 6, character: 1 } }, selectionRange: at(4, 16, 9) },\
]";

/// A flat `SymbolInformation` outline: no `selectionRange`, only each
/// symbol's `location.range`.
const FLAT_OUTLINE: &str = "[\
  { name: 'read', kind: 6, location: { uri: ledgerUri, range: declSite } },\
  { name: 'useLedger', kind: 12, location: { uri: ledgerUri, range: { start: { line: 4, character: 0 }, end: { line: 6, character: 1 } } } },\
]";

/// The row of `callers` for `Port.read`, with `ledger.ts` holding `ledger`,
/// the server's reference to the declaration at `site` (a JS expression)
/// and its outline `symbols`.
fn ledger_row(ledger: &str, site: &str, symbols: &str) -> serde_json::Value {
    let server = LEDGER_SERVER
        .replace("__DECL_SITE__", site)
        .replace("__SYMBOLS__", symbols);
    let (_dir, root, execution) = importer_fixture_serving(&[], &server);
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
    row
}

/// The callers in `row` as `(caller, sites)`.
fn callers_of(row: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
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

/// The callers of `Port.read` as `(caller, sites)` with the default server.
fn ledger_callers(ledger: &str) -> Vec<(String, serde_json::Value)> {
    callers_of(&ledger_row(ledger, "at(2, 2, 4)", NESTED_OUTLINE))
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

/// oxc parses the file but does not decide the declaration site (it points
/// into the middle of the name): the text rule and the outline drop the
/// declaration name and keep the real call.
#[test]
fn a_site_oxc_does_not_decide_falls_back_to_the_text_rule_and_outline() {
    if !require_node() {
        return;
    }
    let row = ledger_row(LEDGER, "at(2, 3, 3)", NESTED_OUTLINE);
    assert_eq!(callers_of(&row), only_the_call());
}

/// A flat `SymbolInformation` outline selects each symbol's `location.range`:
/// the declaration name matches it and is dropped, and the call is
/// attributed to the flat symbol around it.
#[test]
fn a_flat_symbol_information_outline_drops_the_declaration_name() {
    if !require_node() {
        return;
    }
    let broken = format!("{LEDGER}const broken = ;\n");
    let row = ledger_row(&broken, "at(2, 2, 4)", FLAT_OUTLINE);
    assert_eq!(callers_of(&row), only_the_call());
}

/// The outline request fails: the text rule decides alone, so the
/// declaration stays with the call (under the file), and the failure is
/// reported because calls remain.
#[test]
fn a_failed_outline_leaves_the_text_rule_alone_and_is_reported() {
    if !require_node() {
        return;
    }
    let broken = format!("{LEDGER}const broken = ;\n");
    let row = ledger_row(&broken, "at(2, 2, 4)", "({__error: true})");
    let sites = callers_of(&row)
        .into_iter()
        .flat_map(|(_, sites)| sites.as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(sites.len(), 2, "{row}");
    assert!(row.to_string().contains("outline failed"), "{row}");
}

/// The same failure is not reported when no call remains to be affected.
#[test]
fn a_failed_outline_is_not_reported_when_no_call_remains() {
    if !require_node() {
        return;
    }
    let no_calls = "a\nb\n  read = 1;\nc\nd\n           read = 2;\nconst broken = ;\n";
    let row = ledger_row(no_calls, "at(2, 2, 4)", "({__error: true})");
    assert_eq!(callers_of(&row), vec![], "{row}");
    assert!(!row.to_string().contains("outline failed"), "{row}");
}
