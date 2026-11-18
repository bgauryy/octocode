//! Importers of a renamed re-export: a relay exports the anchor's symbol
//! under another name, and its consumers never spell the anchor's.

use super::*;

/// A TypeScript-like server whose project sees only the anchor file. The name
/// at a position binds `target` in `a.ts`, except in a file under `u/`, whose
/// names bind `u/other.ts`. `references` from an importer lists every site of
/// the asked name in that file.
const RENAMING_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
const root = process.env.FAKE_ROOT || __ROOT__;
const declUri = 'file://' + root + '/a.ts';
const otherUri = 'file://' + root + '/u/other.ts';
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const decl = { uri: declUri, range: at(0, 16, 6) };
const word = /[A-Za-z_$][A-Za-z0-9_$]*/g;
function lines(uri) {
  return fs.readFileSync(uri.slice('file://'.length), 'utf8').split('\n');
}
function nameAt(uri, position) {
  for (const found of (lines(uri)[position.line] || '').matchAll(word)) {
    if (found.index <= position.character && position.character < found.index + found[0].length) return found[0];
  }
  return null;
}
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
    case 'textDocument/definition': {
      const uri = p.textDocument.uri;
      if (uri.includes('/u/')) return [{ uri: otherUri, range: at(0, 16, 4) }];
      return [decl];
    }
    case 'textDocument/references': {
      const uri = p.textDocument.uri;
      if (uri === declUri) return [decl];
      const name = nameAt(uri, p.position);
      const sites = [];
      lines(uri).forEach((text, line) => {
        for (const found of text.matchAll(word)) {
          if (found[0] === name) sites.push({ uri, range: at(line, found.index, name.length) });
        }
      });
      return sites;
    }
    case 'textDocument/prepareCallHierarchy':
      return [{ name: 'target', kind: 12, uri: declUri, range: at(0, 0, 28), selectionRange: decl.range }];
    case 'callHierarchy/incomingCalls':
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

/// The reference rows of `references` on `target` in a workspace holding
/// `files` beside `a.ts`, every page walked, and the last page's row.
fn references_over(files: &[(&str, &str)]) -> (Vec<(String, u64, u64)>, serde_json::Value) {
    let (_dir, root, execution) = importer_fixture_serving(&[], RENAMING_SERVER);
    for (name, text) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("file");
    }
    octocode_engine::stamp::settle_written_so_far();
    let pool = fake_server_pool();
    let start = serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    });
    let (pages, _) = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(walk_pages(&pool, &root, &execution, start, None));
    let mut rows = pages
        .iter()
        .flat_map(|page| reference_rows(&page.row))
        .collect::<Vec<_>>();
    rows.sort();
    let last = pages.last().expect("a page").row.clone();
    (rows, last)
}

fn files_of(rows: &[(String, u64, u64)]) -> Vec<&str> {
    let mut files = rows
        .iter()
        .map(|(file, _, _)| file.as_str())
        .collect::<Vec<_>>();
    files.dedup();
    files
}

/// `export { target as viaC } from './a'`: the consumer of `viaC` spells no
/// `target`, and is an importer all the same.
#[test]
fn a_consumer_of_a_renamed_re_export_is_an_importer() {
    if !require_node() {
        return;
    }
    let (rows, last) = references_over(&[
        ("c.ts", "export { target as viaC } from './a';\n"),
        (
            "d.ts",
            "import { viaC } from './c';\nexport const d1 = viaC(8);\n",
        ),
    ]);
    assert_eq!(files_of(&rows), ["a.ts", "c.ts", "d.ts"], "{rows:?}");
    assert!(
        rows.contains(&("d.ts".to_owned(), 1, 10)) && rows.contains(&("d.ts".to_owned(), 2, 19)),
        "{rows:?}"
    );
    assert_ne!(last["coverage"]["importerScan"], "capped", "{last}");
    assert!(
        last.pointer("/coverage/importerScan").is_none(),
        "a complete scan is not stated: {last}"
    );
}

/// The rename is followed through every relay: a second rename of the first
/// alias, and a relay that renames what it imported under a local name.
#[test]
fn renames_are_followed_transitively_and_through_a_local_binding() {
    if !require_node() {
        return;
    }
    let (rows, _) = references_over(&[
        ("c.ts", "export { target as viaC } from './a';\n"),
        ("e.ts", "export { viaC as viaE } from './c';\n"),
        ("f.ts", "import { viaE } from './e';\nviaE(1);\n"),
        (
            "g.ts",
            "import { target as inner } from './a';\nexport { inner as viaG };\n",
        ),
        ("h.ts", "import { viaG } from './g';\nviaG();\n"),
        // A barrel of the alias relays it under the same name.
        ("i.ts", "export { viaE } from './e';\n"),
        ("j.ts", "import { viaE } from './i';\nviaE(2);\n"),
    ]);
    assert_eq!(
        files_of(&rows),
        [
            "a.ts", "c.ts", "e.ts", "f.ts", "g.ts", "h.ts", "i.ts", "j.ts"
        ],
        "{rows:?}"
    );
}

/// Two relays that rename each other's export end the walk: each `(relay,
/// name)` is followed once.
#[test]
fn a_cycle_of_renames_ends() {
    if !require_node() {
        return;
    }
    let (rows, last) = references_over(&[
        ("c.ts", "export { target as viaC } from './a';\n"),
        (
            "k.ts",
            "export { viaC as viaK } from './c';\nexport { viaC as viaE } from './m';\n",
        ),
        ("m.ts", "export { viaE as viaC } from './k';\n"),
        ("d.ts", "import { viaK } from './k';\nviaK();\n"),
    ]);
    assert!(files_of(&rows).contains(&"d.ts"), "{rows:?}");
    assert_ne!(last["coverage"]["importerScan"], "capped", "{last}");
    assert!(
        last.pointer("/coverage/importerScan").is_none(),
        "a complete scan is not stated: {last}"
    );
}

/// A same-named export of another module is no alias of the anchor's: its
/// consumer reaches no relay, and is neither listed nor left unverified.
#[test]
fn a_same_named_export_of_another_module_is_not_an_importer() {
    if !require_node() {
        return;
    }
    let (rows, last) = references_over(&[
        ("c.ts", "export { target as viaC } from './a';\n"),
        ("d.ts", "import { viaC } from './c';\nviaC(8);\n"),
        ("u/other.ts", "export function viaC() {}\n"),
        ("u/user.ts", "import { viaC } from './other';\nviaC();\n"),
    ]);
    assert_eq!(files_of(&rows), ["a.ts", "c.ts", "d.ts"], "{rows:?}");
    assert_ne!(last["coverage"]["importerScan"], "capped", "{last}");
    assert!(
        last.pointer("/coverage/importerScan").is_none(),
        "a complete scan is not stated: {last}"
    );
    assert_ne!(last["isPartial"], true, "{last}");
}
