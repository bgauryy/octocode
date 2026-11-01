//! The definition chain's verification hop: a first answer that already
//! starts a declaration ends the chain without another request, and an
//! answer on a re-export still hops to the declaration.

use super::*;

/// `x` is declared in `a.ts`; `b.ts` calls it through an import, `e.ts`
/// through the barrel `c.ts`. Every document message is logged.
const HOP_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
const root = __ROOT__;
const log = __LOG__;
const uri = (name) => 'file://' + root + '/' + name;
const at = (name, line, character) => ({ uri: uri(name), range: { start: { line, character }, end: { line, character: character + 1 } } });
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function result(msg) {
  const p = msg.params || {};
  if (msg.method === 'initialize') return { capabilities: { textDocumentSync: 1, definitionProvider: true } };
  if (msg.method !== 'textDocument/definition') return null;
  const file = p.textDocument.uri.split('/').pop();
  return file === 'e.ts' ? [at('c.ts', 0, 9)] : [at('a.ts', 0, 16)];
}
function handle(msg) {
  if (msg.method === 'exit') process.exit(0);
  const p = msg.params || {};
  if (p.textDocument) fs.appendFileSync(log, msg.method + ' ' + p.textDocument.uri.split('/').pop() + '\n');
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

/// The definition of `symbol` on one-based `line` of `file`: the documents
/// the server was sent (`didOpen` and requests) and the files listed.
fn definition_of(file: &str, symbol: &str, line: u32) -> (Vec<String>, Vec<String>) {
    use std::os::unix::fs::PermissionsExt;
    let dir = Fixture::new();
    let outer = dir.0.canonicalize().expect("canonical fixture");
    let root = outer.join("ws");
    std::fs::create_dir_all(&root).expect("workspace");
    let log = outer.join("requests.log");
    let literal = |value: &Path| serde_json::to_string(&value.to_string_lossy()).expect("literal");
    let script = outer.join("fake-hop-lsp.js");
    let source = HOP_SERVER
        .replace("__ROOT__", &literal(&root))
        .replace("__LOG__", &literal(&log));
    std::fs::write(&script, source).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    for (name, text) in [
        ("a.ts", "export function x() {}\n"),
        ("b.ts", "import { x as y } from './a';\ny();\n"),
        ("c.ts", "export { x } from './a';\n"),
        ("e.ts", "import { x } from './c';\nx();\n"),
    ] {
        std::fs::write(root.join(name), text).expect("source");
    }
    // A language outside the TypeScript family: no importer recovery.
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": "hops"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    };
    let query = serde_json::from_value(serde_json::json!({
        "operation": "definition", "path": root.join(file).to_string_lossy(),
        "symbolName": symbol, "line": line, "workspaceRoot": root.to_string_lossy()
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
        ))
        .expect("row");
    assert_row_contract(ToolId::LspSearch, &row);
    let sent = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.starts_with("textDocument/didClose"))
        .map(str::to_owned)
        .collect();
    let listed = paths_in(&row)
        .into_iter()
        .filter_map(|path| {
            Path::new(&path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    (sent, listed)
}

/// A first answer on a declaration is the declaration: one `definition`
/// request and one document sync, the anchor's.
#[test]
fn a_first_answer_on_a_declaration_sends_no_verification_hop() {
    if !require_node() {
        return;
    }
    let (sent, listed) = definition_of("b.ts", "y", 2);
    assert!(listed.iter().any(|name| name == "a.ts"), "{listed:?}");
    assert_eq!(
        sent,
        ["textDocument/didOpen b.ts", "textDocument/definition b.ts"],
        "{sent:?}"
    );
}

/// A first answer on a barrel's re-export is a binding: the chain hops to
/// the declaration it names.
#[test]
fn a_first_answer_on_a_re_export_still_hops() {
    if !require_node() {
        return;
    }
    let (sent, listed) = definition_of("e.ts", "x", 2);
    assert!(listed.iter().any(|name| name == "a.ts"), "{listed:?}");
    let definitions = sent
        .iter()
        .filter(|line| line.starts_with("textDocument/definition"))
        .collect::<Vec<_>>();
    assert_eq!(
        definitions,
        [
            "textDocument/definition e.ts",
            "textDocument/definition c.ts"
        ],
        "{sent:?}"
    );
}
