//! Importer recovery fixtures and page walkers: a TS-like server whose project
//! sees only the anchor file, and the importer-window scan over it.

use super::*;

mod call_sites;
mod identity;
mod pages;
mod per_binding;
mod recovery;
mod windows;

/// A TypeScript-like server whose project sees only the anchor file (an
/// inferred project): `references` from the anchor lists the declaration
/// alone; from an importer it lists the importer's own two sites plus a
/// shared site in `f000.ts`. Every position resolves to the declaration.
const IMPORTER_SERVER: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const declUri = 'file://' + root + '/a.ts';
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const decl = { uri: declUri, range: at(0, 16, 6) };
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
      // `n*.ts` files mention the name but bind another declaration.
      const uri = p.textDocument.uri;
      const other = 'file://' + root + '/other.ts';
      if (/\/n\d+\.ts$/.test(uri)) return [{ uri: other, range: at(0, 16, 5) }];
      if (uri === other) return [{ uri, range: at(0, 16, 5) }];
      return [decl];
    }
    case 'textDocument/references': {
      const uri = p.textDocument.uri;
      if (uri === declUri) return [decl];
      return [{ uri, range: at(0, 9, 6) }, { uri, range: at(1, 0, 6) }, { uri: 'file://' + root + '/f000.ts', range: at(1, 0, 6) }];
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
  const answer = result(msg);
  // `{ __error }` answers the request with an internal error.
  if (answer && answer.__error) send({ jsonrpc: '2.0', id: msg.id, error: { code: -32603, message: answer.__error } });
  else send({ jsonrpc: '2.0', id: msg.id, result: answer });
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

/// Importer candidates in the fixture: 24 + 24 + 12 (three windows).
const IMPORTERS: usize = 60;

/// A workspace with `a.ts` declaring `target`, [`IMPORTERS`] files that
/// import and call it, the fake server, and an explicit server config.
fn importer_fixture() -> (
    Fixture,
    PathBuf,
    crate::tools::lsp_search::LspExecutionConfig,
) {
    importer_fixture_of(&[("f", IMPORTERS)])
}

/// Like [`importer_fixture`], with `layout` listing `(prefix, count)` runs of
/// files that spell `target`; candidates sort by name, so runs fill importer
/// windows in order. An `n` file spells the name but binds another
/// declaration (no reference); any other prefix imports and calls it.
fn importer_fixture_of(
    layout: &[(&str, usize)],
) -> (
    Fixture,
    PathBuf,
    crate::tools::lsp_search::LspExecutionConfig,
) {
    importer_fixture_serving(layout, IMPORTER_SERVER)
}

/// [`importer_fixture_of`] with its own fake server script.
fn importer_fixture_serving(
    layout: &[(&str, usize)],
    server: &str,
) -> (
    Fixture,
    PathBuf,
    crate::tools::lsp_search::LspExecutionConfig,
) {
    use std::os::unix::fs::PermissionsExt;
    let dir = Fixture::new();
    let outer = dir.0.canonicalize().expect("canonical fixture");
    let root = outer.join("ws");
    std::fs::create_dir_all(&root).expect("workspace");
    // The server script lives beside the workspace: it spells the symbol,
    // and a candidate scan must not count it.
    let script = outer.join("fake-ts-lsp.js");
    let source = server.replace(
        "__ROOT__",
        &serde_json::to_string(&root.to_string_lossy()).expect("root literal"),
    );
    std::fs::write(&script, source).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::fs::write(root.join("tsconfig.json"), "{}\n").expect("tsconfig");
    std::fs::write(root.join("a.ts"), "export function target() {}\n").expect("anchor");
    std::fs::write(root.join("other.ts"), "export function other() {}\n").expect("other");
    for (prefix, count) in layout {
        for index in 0..*count {
            write_importer(&root, &format!("{prefix}{index:03}.ts"));
        }
    }
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": "typescript"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    };
    (dir, root, execution)
}

fn write_importer(root: &Path, name: &str) {
    std::fs::write(
        root.join(name),
        "import { target } from './a';\ntarget();\n",
    )
    .expect("importer");
}

/// The single query row of a continuation.
fn next_row(row: &serde_json::Value, name: &str) -> Option<serde_json::Value> {
    row.pointer(&format!("/next/{name}/input/queries/0"))
        .cloned()
}

/// One walked page: the row and the importer page it belongs to.
struct Walked {
    importer_page: u64,
    row: serde_json::Value,
}

/// Run `start` and every page lead its pages offer (`checks::Walker`: each
/// call once, each match row once, every output and lead within the
/// contract). Every page of a window with windows left offers
/// `next.nextImporterPage`, the same continuation on each: a published
/// cursor names its content, so a reader follows it once (the test
/// publisher numbers its cursors, so the walk drops a repeated one here).
/// Also returns every continuation offered, by name.
async fn walk_pages(
    pool: &LspClientPool,
    root: &Path,
    execution: &crate::tools::lsp_search::LspExecutionConfig,
    start: serde_json::Value,
    budget: Option<usize>,
) -> (Vec<Walked>, Vec<(String, serde_json::Value)>) {
    let paths = crate::tools::test_support::workspace_policy(root);
    let mut pages = Vec::new();
    let mut continuations = Vec::new();
    let mut followed = Vec::new();
    let mut walker = Walker::new(
        ToolId::LspSearch.as_str(),
        serde_json::json!({ "queries": [start] }),
    );
    while let Some(call) = walker.next_query() {
        let row =
            crate::tools::test_support::opened_input(crate::id::ToolId::LspSearch, &call)["queries"][0].clone();
        let importer_page = row
            .get("importerPage")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1);
        let query = serde_json::from_value(row.clone()).expect("continuation is a valid query");
        let result = crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            pool,
            &paths,
            execution,
            budget,
        )
        .await
        .expect("page");
        assert_ne!(result["status"], "error", "{result}");
        let page = next_row(&result, "nextPage");
        let importer = next_row(&result, "nextImporterPage");
        let mut offered = result.clone();
        if let Some(call) = &importer
            && followed.contains(call)
            && let Some(next) = offered
                .get_mut("next")
                .and_then(serde_json::Value::as_object_mut)
        {
            next.remove("nextImporterPage");
        }
        followed.extend(importer.clone().filter(|call| !followed.contains(call)));
        for (name, call) in [("nextPage", page), ("nextImporterPage", importer)] {
            if let Some(call) = call {
                continuations.push((name.to_owned(), call));
            }
        }
        walker.accept(Page {
            output: crate::tools::test_support::published(ToolId::LspSearch, &offered),
            rows: match_rows(&result),
        });
        pages.push(Walked {
            importer_page,
            row: result,
        });
    }
    walker.finish();
    (pages, continuations)
}

/// `(file name, line, column)` of every reference row on a page.
fn reference_rows(row: &serde_json::Value) -> Vec<(String, u64, u64)> {
    row["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|site| {
            let name = Path::new(site["path"].as_str().expect("path"))
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned();
            (
                name,
                site["displayRange"]["startLine"].as_u64().expect("line"),
                site["displayRange"]["startCharacter"]
                    .as_u64()
                    .expect("column"),
            )
        })
        .collect()
}

/// Walks `operation` over a workspace whose first window imports `target`,
/// then `silent` files that spell it without binding it, then one importer.
async fn walk_across_silent_windows(operation: &str, silent: usize) -> (Fixture, Vec<Walked>) {
    let (dir, root, execution) = importer_fixture_of(&[("f", 24), ("n", silent), ("z", 1)]);
    let pool = fake_server_pool();
    let start = serde_json::json!({
        "operation": operation, "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    });
    let (pages, _) = walk_pages(&pool, &root, &execution, start, None).await;
    (dir, pages)
}
