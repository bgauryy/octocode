//! A language with no coverage signal of its own: an answer naming no use is
//! partial only when the server cannot place a file's spelling elsewhere.

use super::*;

/// A Go-like server: `references` lists the declaration alone, `definition` at
/// a site names whatever `__DEFINITIONS__` (a JS object from file name to the
/// answer) holds for that file.
const SERVER: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const uri = (name) => 'file://' + root + '/' + name;
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const decl = { uri: uri('a.go'), range: at(0, 5, 6) };
const definitions = __DEFINITIONS__;
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function result(msg) {
  const p = msg.params || {};
  switch (msg.method) {
    case 'initialize':
      return { capabilities: { textDocumentSync: 1, definitionProvider: true, referencesProvider: true } };
    case 'textDocument/references':
      return [decl];
    case 'textDocument/definition': {
      const name = p.textDocument.uri.split('/').pop();
      return name === 'a.go' ? [decl] : definitions[name] || [];
    }
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

/// `references` of `Helper` in `a.go`, with `b.go` calling a `Helper` and the
/// server answering `definition` there with `definitions`.
fn row_with(definitions: &str, b_go: &str) -> serde_json::Value {
    let dir = Fixture::new();
    let outer = dir.0.canonicalize().expect("real");
    let root = outer.join("ws");
    std::fs::create_dir_all(root.join(".git")).expect("root");
    let script = outer.join("fake-gopls");
    let source = SERVER
        .replace(
            "__ROOT__",
            &serde_json::to_string(&root.to_string_lossy()).expect("root"),
        )
        .replace("__DEFINITIONS__", definitions);
    crate::tools::lsp_search::tests::write_executable(&script, source.as_bytes());
    std::fs::write(root.join("go.mod"), "module m\n").expect("module");
    std::fs::write(root.join("a.go"), "func Helper() {}\n").expect("anchor");
    std::fs::write(root.join("b.go"), b_go).expect("other");
    std::fs::write(
        root.join("lsp-servers.json"),
        serde_json::json!({"languageServers": {".go": {
            "command": script.to_string_lossy(), "args": [], "languageId": "go"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        octocode_home: Some(root.clone()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    };
    let query = serde_json::from_value(serde_json::json!({
        "operation": "references", "path": root.join("a.go").to_string_lossy(),
        "symbolName": "Helper", "line": 1, "workspaceRoot": root.to_string_lossy()
    }))
    .expect("query");
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &fake_server_pool(),
            &paths,
            &execution,
            None,
            crate::tools::lsp_search::test_answers(),
        ))
        .expect("row")
}

const CALL: &str = "func f() { Helper() }\n";

/// The file's `Helper` resolves to its own declaration: another symbol, so a
/// truly unused name stays clean however common the name is.
#[test]
fn a_name_the_server_places_elsewhere_is_no_gap() {
    let row = row_with(
        "{ 'b.go': [{ uri: uri('b.go'), range: at(0, 11, 6) }] }",
        CALL,
    );
    assert_ne!(row["isPartial"], true, "{row}");
}

/// A server with no project behind the file answers nothing: partial.
#[test]
fn a_file_the_server_cannot_place_makes_the_answer_partial() {
    let row = row_with("{}", CALL);
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["serverUnanswered"]),
        "{row}"
    );
}

/// A site that resolves to the anchor is a use the server's answer missed.
#[test]
fn a_site_resolving_to_the_anchor_makes_the_answer_partial() {
    let row = row_with("{ 'b.go': [decl] }", CALL);
    assert_eq!(row["isPartial"], true, "{row}");
}
