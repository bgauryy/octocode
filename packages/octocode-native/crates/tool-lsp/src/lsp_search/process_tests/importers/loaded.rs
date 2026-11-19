//! Answers that do not depend on what the server had loaded (TB11), and
//! identical repeats served from the kept answer (TE8).
//!
//! tsserver and tsgo answer from the projects of the documents they hold: a
//! test file its tsconfig excludes joins only once it is opened, so a first
//! call (which opens it during importer recovery) and its repeat (which
//! finds it open) used to differ.

use super::*;

/// A TS-like server whose anchor answers grow with the documents it holds:
/// `callers` and `references` of `target` list each opened `f*.ts`
/// importer, and its call hierarchy prepared anywhere names the declaration.
const LOADING_SERVER: &str = r#"#!/usr/bin/env node
const root = process.env.FAKE_ROOT || __ROOT__;
const declUri = 'file://' + root + '/a.ts';
const at = (line, character, length) => ({ start: { line, character }, end: { line, character: character + length } });
const decl = { uri: declUri, range: at(0, 16, 6) };
const opened = new Set();
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
const importers = () => [...opened].filter((uri) => /\/f\d+\.ts$/.test(uri)).sort();
function result(msg) {
  const p = msg.params || {};
  const uri = p.textDocument && p.textDocument.uri;
  switch (msg.method) {
    case 'initialize':
      return { capabilities: { textDocumentSync: 1, definitionProvider: true, referencesProvider: true, callHierarchyProvider: true, documentSymbolProvider: true } };
    case 'textDocument/definition':
      return [decl];
    case 'textDocument/references':
      if (uri === declUri) return [decl, ...importers().flatMap((file) => [{ uri: file, range: at(0, 9, 6) }, { uri: file, range: at(1, 0, 6) }])];
      return [{ uri, range: at(0, 9, 6) }, { uri, range: at(1, 0, 6) }];
    case 'textDocument/prepareCallHierarchy':
      return [{ name: 'target', kind: 12, uri: declUri, range: at(0, 0, 28), selectionRange: decl.range }];
    case 'callHierarchy/incomingCalls':
      return importers().map((file, index) => ({ from: { name: 'caller' + index, kind: 12, uri: file, range: at(1, 0, 9), selectionRange: at(1, 0, 6) }, fromRanges: [at(1, 0, 6)] }));
    case 'textDocument/documentSymbol':
      return [];
    default:
      return null;
  }
}
function handle(msg) {
  // A server with file events: the client walks the scope's fingerprint.
  if (msg.method === 'initialized') send({ jsonrpc: '2.0', id: 'watch', method: 'client/registerCapability', params: { registrations: [{ id: 'w', method: 'workspace/didChangeWatchedFiles', registerOptions: { watchers: [{ globPattern: '**/*' }] } }] } });
  if (msg.method === 'textDocument/didOpen') opened.add(msg.params.textDocument.uri);
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

/// One `operation` row on `target` (no pages follow it here).
async fn ask(
    pool: &LspClientPool,
    root: &Path,
    execution: &crate::lsp_search::LspExecutionConfig,
    operation: &str,
) -> serde_json::Value {
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
    let query = serde_json::from_value(serde_json::json!({
        "operation": operation, "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    }))
    .expect("query");
    let row = crate::lsp_search::execute(
        query,
        &octocode_runtime_base::execution::cancel::NeverCancel,
        pool,
        &paths,
        execution,
        None,
        crate::lsp_search::test_answers(),
    )
    .await
    .expect("row");
    assert_ne!(row["status"], "error", "{row}");
    row
}

/// The caller names by file, and the reference sites, of a row.
fn answered(row: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "files": row["files"],
        "matches": row["matches"],
        "coverage": row["coverage"],
        "partialReasons": row["partialReasons"],
    })
}

#[test]
fn a_first_call_answers_as_its_identical_repeat_does() {
    if !require_node() {
        return;
    }
    for operation in ["callers", "references"] {
        let (_dir, root, execution) = importer_fixture_serving(&[("f", 2)], LOADING_SERVER);
        let pool = fake_server_pool();
        let (first, repeat) = crate::lsp_search::answer_memo::computed(|| {
            tokio::runtime::Runtime::new().expect("rt").block_on(async {
                let first = ask(&pool, &root, &execution, operation).await;
                let repeat = ask(&pool, &root, &execution, operation).await;
                (first, repeat)
            })
        });
        assert_eq!(
            answered(&first),
            answered(&repeat),
            "{operation}: {first}\n{repeat}"
        );
        if operation == "callers" {
            // The server's own call hierarchy names each caller once it holds
            // the importer: no file-level row recovered from references.
            let text = first.to_string();
            assert!(
                text.contains("caller0") && text.contains("caller1"),
                "{first}"
            );
            assert!(!text.contains("recoveredFromReferences"), "{first}");
        } else {
            assert!(
                !first.to_string().contains("recoveredFromImporter"),
                "{first}"
            );
        }
    }
}

#[test]
fn a_batch_of_references_and_callers_answers_as_the_sequential_repeat() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_serving(&[("f", 2)], LOADING_SERVER);
    let pool = fake_server_pool();
    let (batched, repeat) = crate::lsp_search::answer_memo::computed(|| {
        tokio::runtime::Runtime::new().expect("rt").block_on(async {
            let (_, callers) = tokio::join!(
                ask(&pool, &root, &execution, "references"),
                ask(&pool, &root, &execution, "callers")
            );
            let repeat = ask(&pool, &root, &execution, "callers").await;
            (callers, repeat)
        })
    });
    assert_eq!(answered(&batched), answered(&repeat), "{batched}\n{repeat}");
}

/// The repeat is the kept answer; an edit to a file of the scope is not.
#[test]
fn an_identical_repeat_is_served_kept_until_a_file_changes() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_serving(&[("f", 2)], LOADING_SERVER);
    let pool = fake_server_pool();
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let before = crate::lsp_search::answer_memo::hits();
    // The server registers its file events after its first request may
    // have looked: the second call walks the fingerprint and keeps.
    let first = runtime.block_on(ask(&pool, &root, &execution, "callers"));
    let second = runtime.block_on(ask(&pool, &root, &execution, "callers"));
    let repeat = runtime.block_on(ask(&pool, &root, &execution, "callers"));
    let before = before + usize::from(crate::lsp_search::answer_memo::hits() > before + 1);
    assert_eq!(
        crate::lsp_search::answer_memo::hits(),
        before + 1,
        "{repeat}"
    );
    assert_eq!(answered(&first), answered(&repeat));
    assert_eq!(second, repeat);
    std::fs::write(
        root.join("f001.ts"),
        "import { target } from './a';\n\ntarget();\n",
    )
    .expect("edit");
    octocode_engine::stamp::settle_written_so_far();
    let edited = runtime.block_on(ask(&pool, &root, &execution, "callers"));
    assert_eq!(
        crate::lsp_search::answer_memo::hits(),
        before + 1,
        "an edit recomputes: {edited}"
    );
}
