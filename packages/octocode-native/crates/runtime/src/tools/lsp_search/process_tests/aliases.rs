//! Alias recovery over three server behaviors: a server that follows the
//! rename (no extra request), one that answers references at the alias
//! (recovered from that answer), and one that answers the alias with the
//! imported symbol's references (recovered by verifying the local name).

use super::*;

/// `x` is declared in `a.ts`; `b.ts` imports it as `y` and calls `y` twice.
/// `__MODE__` picks what `references` answers; every request is logged.
const ALIAS_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
const root = __ROOT__;
const log = __LOG__;
const mode = __MODE__;
const a = 'file://' + root + '/a.ts';
const b = 'file://' + root + '/b.ts';
const at = (uri, line, character) => ({ uri, range: { start: { line, character }, end: { line, character: character + 1 } } });
const decl = at(a, 0, 16);
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
    case 'textDocument/definition':
      return [decl];
    case 'textDocument/references': {
      const uri = p.textDocument.uri;
      if (uri === a) {
        if (mode === 'follows') return [decl, at(b, 0, 9), at(b, 0, 14), at(b, 1, 0), at(b, 2, 0)];
        return [decl, at(b, 0, 9)];
      }
      if (mode === 'alias') return [at(b, 0, 14), at(b, 1, 0), at(b, 2, 0)];
      return [decl, at(b, 0, 9)];
    }
    default:
      return null;
  }
}
function handle(msg) {
  if (msg.method === 'exit') process.exit(0);
  if (msg.id === undefined || !msg.method) return;
  const p = msg.params || {};
  if (p.textDocument) fs.appendFileSync(log, msg.method + ' ' + p.textDocument.uri.split('/').pop() + '\n');
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

/// A listed site: file name, one-based line and column, and whether it is
/// labelled as recovered through an alias.
type Site = (String, u64, u64, bool);

/// The requests the server logged answering `mode`'s references, and every
/// site the row lists.
fn references_under(mode: &str) -> (Vec<String>, Vec<Site>) {
    use std::os::unix::fs::PermissionsExt;
    let dir = Fixture::new();
    let outer = dir.0.canonicalize().expect("canonical fixture");
    let root = outer.join("ws");
    std::fs::create_dir_all(&root).expect("workspace");
    let log = outer.join("requests.log");
    let literal = |value: &Path| serde_json::to_string(&value.to_string_lossy()).expect("literal");
    let script = outer.join("fake-alias-lsp.js");
    let source = ALIAS_SERVER
        .replace("__ROOT__", &literal(&root))
        .replace("__LOG__", &literal(&log))
        .replace("__MODE__", &serde_json::to_string(mode).expect("mode"));
    std::fs::write(&script, source).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::fs::write(root.join("a.ts"), "export function x() {}\n").expect("a.ts");
    std::fs::write(
        root.join("b.ts"),
        "import { x as y } from './a';\ny();\ny();\n",
    )
    .expect("b.ts");
    // A language outside the TypeScript family: no importer recovery, so
    // the log holds the alias requests alone.
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": "aliased"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    };
    let query = serde_json::from_value(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "x", "line": 1, "workspaceRoot": root.to_string_lossy()
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
    assert!(row.get("isPartial").is_none(), "{row}");
    let requests = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.starts_with("textDocument/did"))
        .map(str::to_owned)
        .collect();
    // Every listed site is classified: the import line's names are
    // imports, `y()` is a call, and the declaration is a declaration.
    for site in row["matches"].as_array().into_iter().flatten() {
        let file = site["path"].as_str().expect("path");
        let line = site["displayRange"]["startLine"].as_u64().expect("line");
        let expected = match (file.ends_with("b.ts"), line) {
            (true, 1) => "import",
            (true, _) => "call",
            (false, _) => "declaration",
        };
        assert_eq!(site["role"], expected, "{site}");
    }
    let mut sites = row["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|site| {
            (
                Path::new(site["path"].as_str().expect("path"))
                    .file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned(),
                site["displayRange"]["startLine"].as_u64().expect("line"),
                site["displayRange"]["startCharacter"]
                    .as_u64()
                    .expect("column"),
                site["source"] == "recoveredAlias",
            )
        })
        .collect::<Vec<_>>();
    sites.sort();
    (requests, sites)
}

/// Every site of `x` and of its alias `y`; `recovered` marks the alias's.
fn every_site(recovered: bool) -> Vec<Site> {
    vec![
        ("a.ts".into(), 1, 17, false),
        ("b.ts".into(), 1, 10, false),
        ("b.ts".into(), 1, 15, recovered),
        ("b.ts".into(), 2, 1, recovered),
        ("b.ts".into(), 3, 1, recovered),
    ]
}

/// A server whose answer lists the alias binding followed the rename:
/// recovery sends no request of its own.
#[test]
fn a_server_that_follows_the_rename_costs_no_recovery_request() {
    if !require_node() {
        return;
    }
    let (requests, sites) = references_under("follows");
    assert_eq!(sites, every_site(false));
    assert_eq!(requests, ["textDocument/references a.ts"], "{requests:?}");
}

/// A server that lists only the import's own name: references at the alias
/// binding recover its uses, with no identity request.
#[test]
fn references_at_the_alias_recover_its_uses() {
    if !require_node() {
        return;
    }
    let (requests, sites) = references_under("alias");
    assert_eq!(sites, every_site(true));
    assert_eq!(
        requests,
        [
            "textDocument/references a.ts",
            "textDocument/references b.ts"
        ],
        "{requests:?}"
    );
}

/// A server that answers the alias with the imported symbol's references
/// (rust-analyzer): each occurrence of the local name is verified by its
/// definition and listed.
#[test]
fn a_server_that_answers_the_alias_as_the_symbol_recovers_by_verification() {
    if !require_node() {
        return;
    }
    let (requests, sites) = references_under("symbol");
    assert_eq!(sites, every_site(true));
    assert!(
        requests
            .iter()
            .any(|request| request == "textDocument/definition b.ts"),
        "{requests:?}"
    );
}
