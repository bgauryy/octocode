//! Importers of a workspace package imported by name (TB1): the host's
//! server binds the name to the package's emitted `dist/*.d.ts`, a
//! declaration of its own, so its call is found only through the tsconfig's
//! `outDir`/`rootDir` mapping back to the anchor's source.

use super::*;

/// A TypeScript-like server over two packages. `packages/core/src/a.ts`
/// declares `target`; its program sees nothing of `packages/host`. The host's
/// program binds `target` to `packages/core/dist/__DIST__.d.ts`, where its
/// call hierarchy finds `main` calling it at `use.ts:3`.
const PACKAGE_SERVER: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const core = 'file://' + root + '/packages/core';
const srcUri = core + '/src/a.ts';
const distUri = core + '/dist/__DIST__.d.ts';
const hostUri = 'file://' + root + '/packages/host/src/use.ts';
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const src = { uri: srcUri, range: at(0, 16, 6) };
const dist = { uri: distUri, range: at(0, 24, 6) };
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function result(msg) {
  const p = msg.params || {};
  const uri = p.textDocument && p.textDocument.uri;
  switch (msg.method) {
    case 'initialize':
      return { capabilities: { textDocumentSync: 1, definitionProvider: true, referencesProvider: true, callHierarchyProvider: true, documentSymbolProvider: true } };
    case 'textDocument/definition':
      if (uri.includes('/packages/host/') || uri === distUri) return [dist];
      return [src];
    case 'textDocument/references':
      if (uri.includes('/packages/host/')) return [{ uri: hostUri, range: at(0, 9, 6) }, { uri: hostUri, range: at(2, 2, 6) }, dist];
      return [src];
    case 'textDocument/prepareCallHierarchy':
      if (uri.includes('/packages/host/')) return [{ name: 'target', kind: 12, uri: distUri, range: at(0, 0, 40), selectionRange: dist.range }];
      return [{ name: 'target', kind: 12, uri: srcUri, range: at(0, 0, 28), selectionRange: src.range }];
    case 'callHierarchy/incomingCalls':
      if (p.item.uri === distUri) return [{ from: { name: 'main', kind: 12, uri: hostUri, range: at(1, 0, 40), selectionRange: at(1, 16, 4) }, fromRanges: [at(2, 2, 6)] }];
      return [];
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

/// The two-package workspace, the host bound to `dist/<dist>.d.ts`. The
/// build output is not ignored, so the text scan lists it too.
fn packages(dist: &str) -> (Fixture, PathBuf, crate::lsp_search::LspExecutionConfig) {
    let server = PACKAGE_SERVER.replace("__DIST__", dist);
    let (dir, root, execution) = importer_fixture_serving(&[], &server);
    for (name, text) in [
        (
            "package.json",
            r#"{"private":true,"workspaces":["packages/*"]}"#,
        ),
        (
            "packages/core/package.json",
            r#"{"name":"@x/core","types":"dist/a.d.ts"}"#,
        ),
        (
            "packages/core/tsconfig.json",
            "{\n  // emitted declarations\n  \"compilerOptions\": { \"outDir\": \"./dist\", \"rootDir\": \"src\", \"declaration\": true },\n  \"include\": [\"src/**/*\"]\n}\n",
        ),
        ("packages/core/src/a.ts", "export function target() {}\n"),
        (
            "packages/core/dist/a.d.ts",
            "export declare function target(): void;\n",
        ),
        // A stale declaration whose source is gone.
        (
            "packages/core/dist/gone.d.ts",
            "export declare function target(): void;\n",
        ),
        (
            "packages/host/package.json",
            r#"{"name":"@x/host","dependencies":{"@x/core":"*"}}"#,
        ),
        ("packages/host/tsconfig.json", "{}\n"),
        (
            "packages/host/src/use.ts",
            "import { target } from '@x/core';\nexport function main() {\n  target();\n}\n",
        ),
    ] {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("file");
    }
    octocode_engine::stamp::settle_written_so_far();
    (dir, root, execution)
}

/// Every page of `operation` on `target` at the core source, and the last
/// page's row.
fn answer(dist: &str, operation: &str) -> (Vec<Walked>, serde_json::Value) {
    let (_dir, root, execution) = packages(dist);
    let pool = fake_server_pool();
    let start = serde_json::json!({
        "operation": operation,
        "path": root.join("packages/core/src/a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    });
    let (pages, _) = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(walk_pages(&pool, &root, &execution, start, None));
    let last = pages.last().expect("a page").row.clone();
    (pages, last)
}

#[test]
fn a_caller_through_the_package_build_output_is_listed() {
    if !require_node() {
        return;
    }
    let (_, row) = answer("a", "callers");
    let files = row["files"].as_array().expect("caller files");
    let host = files
        .iter()
        .find(|file| {
            file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with("packages/host/src/use.ts"))
        })
        .unwrap_or_else(|| panic!("the host caller is listed: {row}"));
    assert_eq!(host["matches"][0]["symbolName"], "main", "{row}");
    assert!(
        files.iter().all(|file| !file["path"]
            .as_str()
            .is_some_and(|path| path.contains("/dist/"))),
        "no build output is a caller: {row}"
    );
    assert!(row.pointer("/coverage/packageImporters").is_none(), "{row}");
    assert_ne!(row["isPartial"], true, "{row}");
}

#[test]
fn references_through_the_package_build_output_list_the_host_not_the_mirror() {
    if !require_node() {
        return;
    }
    let (pages, row) = answer("a", "references");
    let rows = pages
        .iter()
        .flat_map(|page| reference_rows(&page.row))
        .collect::<Vec<_>>();
    assert!(
        rows.contains(&("use.ts".to_owned(), 1, 10)) && rows.contains(&("use.ts".to_owned(), 3, 3)),
        "{rows:?}"
    );
    assert!(
        !rows.iter().any(|(file, _, _)| file.ends_with(".d.ts")),
        "the emitted declaration mirrors the source: {rows:?}"
    );
    assert!(row.pointer("/coverage/packageImporters").is_none(), "{row}");
}

/// A host bound to a declaration of the build output that no source
/// mirrors (a stale build): no use is claimed, and the row says what the
/// scan could not verify instead of `complete`.
#[test]
fn an_unmapped_build_output_binding_is_partial_with_a_count_and_a_lead() {
    if !require_node() {
        return;
    }
    let (_, row) = answer("gone", "callers");
    assert_eq!(row["coverage"]["packageImporters"], 1, "{row}");
    assert_eq!(row["coverage"]["state"], "partial", "{row}");
    assert!(
        row["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.iter().any(|reason| reason == "importUnresolved")),
        "{row}"
    );
    let lead = row["related"]
        .as_array()
        .and_then(|related| related.iter().find(|call| call["tool"] == "localSearch"))
        .unwrap_or_else(|| panic!("a text lead: {row}"));
    assert!(lead.to_string().contains("target"), "{lead}");
}

/// A definition that lands in the build output offers the source it was
/// compiled from (`next.sourceDefinition`, a read of the declaration).
#[test]
fn a_definition_in_the_build_output_offers_its_source() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = packages("a");
    let pool = fake_server_pool();
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let query = serde_json::from_value(serde_json::json!({
        "operation": "definition",
        "path": root.join("packages/host/src/use.ts").to_string_lossy(),
        "symbolName": "target", "line": 3
    }))
    .expect("query");
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
    assert!(
        row.to_string().contains("dist/a.d.ts"),
        "the server's own answer stays: {row}"
    );
    let read = &row["next"]["sourceDefinition"];
    assert_eq!(read["tool"], "localFetch", "{row}");
    let input = &read["input"]["queries"][0];
    assert!(
        input["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("packages/core/src/a.ts")),
        "{row}"
    );
    assert_eq!(input["ranges"][0]["startLine"], 1, "{row}");
}
