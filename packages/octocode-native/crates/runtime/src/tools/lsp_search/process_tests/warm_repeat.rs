//! A warm repeat of an incoming operation asks the server nothing and reads
//! each file once.

use super::*;

/// A TS-like server that logs every request it answers: references from
/// the declaration miss `c.ts` (importer recovery finds it), and every
/// definition resolves to the declaration in `a.ts`.
const REUSE_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
const log = process.env.FAKE_LSP_LOG;
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function at(uri, line, start, end) {
  return { uri, range: { start: { line, character: start }, end: { line, character: end } } };
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { referencesProvider: true, definitionProvider: true, textDocumentSync: 1 } } });
    return;
  }
  if (msg.method === 'exit') process.exit(0);
  if (msg.id === undefined || !msg.method) return;
  fs.appendFileSync(log, msg.method + '\n');
  const uri = msg.params && msg.params.textDocument && msg.params.textDocument.uri;
  const dir = uri ? uri.slice(0, uri.lastIndexOf('/') + 1) : '';
  const decl = at(dir + 'a.ts', 0, 16, 19);
  if (msg.method === 'textDocument/definition') {
    send({ jsonrpc: '2.0', id: msg.id, result: [decl] });
  } else if (msg.method === 'textDocument/references') {
    const found = [decl, at(dir + 'b.ts', 1, 0, 3), at(dir + 'b.ts', 2, 0, 3)];
    if (uri.endsWith('/c.ts')) found.push(at(dir + 'c.ts', 1, 0, 3));
    send({ jsonrpc: '2.0', id: msg.id, result: found });
  } else {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
  }
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

/// A warm repeat of an incoming TS operation under an unchanged response
/// generation asks the server nothing, authorizes and reads each snippet
/// file once per request (however many responses name it), and answers
/// exactly what the cold call did.
#[test]
fn warm_references_repeat_sends_no_requests_and_reads_each_file_once() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::ops::Operation;
    use crate::tools::lsp_search::source::SourceCache;
    use octocode_engine::lsp::client::{RESPONSE_SCOPE, ResponseScope, SnippetReadPolicy};
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    std::fs::write(&script, REUSE_SERVER).expect("server");
    std::fs::write(root.join("a.ts"), "export function foo() {}\n").expect("a");
    std::fs::write(
        root.join("b.ts"),
        "import { foo } from './a';\nfoo();\nfoo();\n",
    )
    .expect("b");
    std::fs::write(root.join("c.ts"), "import { foo } from './a';\nfoo();\n").expect("c");
    let log = root.join("requests.log");
    let mut server = config(&root, &script, None);
    server.language_id = Some("typescript".into());
    server.env = Some(
        [(
            "FAKE_LSP_LOG".to_owned(),
            log.to_string_lossy().into_owned(),
        )]
        .into_iter()
        .collect(),
    );
    let paths = crate::tools::test_support::workspace_policy(&root);
    let path = root.join("a.ts").to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
    let query = serde_json::from_value(json!({
        "operation":"references", "mainGoal":"test", "reasoning":"test",
        "path":uri, "symbolName":"foo", "line":1
    }))
    .expect("query");
    let requests = || {
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .count()
    };
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(server);
        client.start().await.expect("server starts");
        let authorized = Arc::new(AtomicUsize::new(0));
        // One request: its own snippet policy, canonical memo, and scope;
        // the response generation (anchor + scope fingerprint) is unchanged.
        let request = || {
            let (client, paths, query, path, root) = (&client, &paths, &query, &path, &root);
            let authorized = Arc::clone(&authorized);
            let generation = ResponseScope {
                reuse: true,
                generation: "anchor+fingerprint".into(),
            };
            RESPONSE_SCOPE.scope(
                std::cell::RefCell::new(generation),
                crate::tools::lsp_search::scope::with_canonical_memo(async move {
                    let policy = {
                        let paths = paths.clone();
                        SnippetReadPolicy::with_authorizer(move |file| {
                            authorized.fetch_add(1, Ordering::SeqCst);
                            paths.validate_read(file).ok().map(|valid| valid.canonical)
                        })
                        .shared_reads()
                    };
                    let scope = crate::tools::lsp_search::scope::Scope::new(
                        root.to_string_lossy().into_owned(),
                        vec!["*.ts".into()],
                    );
                    scope.set_fingerprint(format!("warm-reuse-{}", std::process::id()));
                    let mut sources = SourceCache::new(paths);
                    Operation {
                        client,
                        query,
                        sources: &mut sources,
                        snippet_policy: &policy,
                        cancel: &NeverCancel,
                        path,
                        scope: &scope,
                        root_only: false,
                        line: 0,
                        character: 16,
                        language_id: Some("typescript"),
                        budget: 16_000,
                        response_window: None,
                    }
                    .run()
                    .await
                    .expect("references row")
                }),
            )
        };
        let cold = request().await;
        let cold_requests = requests();
        let cold_reads = authorized.swap(0, Ordering::SeqCst);
        assert!(lists_path(&cold, "c.ts"), "importer recovered: {cold}");
        assert!(cold_requests > 0);
        // a.ts, b.ts, c.ts: once each, though several responses name them.
        assert_eq!(cold_reads, 3, "{cold}");
        let warm = request().await;
        assert_eq!(warm, cold);
        assert_eq!(
            requests(),
            cold_requests,
            "the warm repeat asked the server"
        );
        assert_eq!(authorized.load(Ordering::SeqCst), 3);
        client.stop().await.expect("fixture server closed");
    });
}
