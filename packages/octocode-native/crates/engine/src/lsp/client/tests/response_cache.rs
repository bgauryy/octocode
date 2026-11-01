//! Response reuse and open documents following their files.

use super::*;
use serde_json::json;

/// Counts `textDocument/references` requests and answers with the count as
/// the reference line, so a reused (cached) answer is visible.
#[cfg(unix)]
const COUNTING_SERVER: &str = r#"#!/usr/bin/env node
let buf = Buffer.alloc(0);
let count = 0;
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { referencesProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'textDocument/references') {
    count += 1;
    const uri = msg.params.textDocument.uri;
    send({ jsonrpc: '2.0', id: msg.id, result: [{ uri, range: { start: { line: count, character: 0 }, end: { line: count, character: 1 } } }] });
  } else if (msg.method === 'exit') {
    process.exit(0);
  } else if (msg.id !== undefined && msg.method) {
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

#[cfg(unix)]
#[test]
fn first_page_reuses_responses_when_generation_matches() {
    use std::os::unix::fs::PermissionsExt;
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let root = tmp.0.join("octocode-engine-response-reuse");
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("fake-lsp.js");
        std::fs::write(&script, COUNTING_SERVER).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let source = root.join("a.ts");
        std::fs::write(&source, "function foo() {}\nfoo();\n".repeat(10)).unwrap();
        let source_path = source
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let client = NativeLspClient::new(LspLanguageServerConfig {
            command: script.to_string_lossy().into_owned(),
            args: Some(Vec::new()),
            workspace_root: root.canonicalize().unwrap().to_string_lossy().into_owned(),
            language_id: Some("typescript".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
            source: Default::default(),
        });
        client.start().await.expect("fake server starts");
        let line = |scope: ResponseScope| {
            let client = &client;
            let source_path = source_path.clone();
            RESPONSE_SCOPE.scope(std::cell::RefCell::new(scope), async move {
                client
                    .get_locations(
                        LocationRequest::References {
                            include_declaration: true,
                        },
                        source_path,
                        0,
                        9,
                        &SnippetReadPolicy::default(),
                        None,
                    )
                    .await
                    .expect("references")[0]
                    .range
                    .start
                    .line
            })
        };
        let reuse = |generation: &str| ResponseScope {
            reuse: true,
            generation: generation.to_owned(),
            call: None,
        };
        let first = line(reuse("g1")).await;
        // Same generation: served from the cache, no second request.
        assert_eq!(line(reuse("g1")).await, first);
        // Another generation (an edit anywhere in the fingerprint): asks.
        let edited = line(reuse("g2")).await;
        assert_ne!(edited, first);
        // Without reuse a request always asks, and refreshes the cache.
        let fresh = line(ResponseScope {
            reuse: false,
            generation: "g1".into(),
            call: None,
        })
        .await;
        assert_ne!(fresh, edited);
        assert_eq!(line(reuse("g1")).await, fresh);
        client.stop().await.unwrap();
        let _ = std::fs::remove_dir_all(root);
    });
}

/// Answers `textDocument/references` from the text it holds: the line of
/// `MARK` in every open document, plus the anchor at the request count, so
/// both a stale open document and a reused answer are visible.
#[cfg(unix)]
const DOCUMENT_SERVER: &str = r#"#!/usr/bin/env node
let buf = Buffer.alloc(0);
let count = 0;
const docs = new Map();
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function at(uri, line) {
  return { uri, range: { start: { line, character: 0 }, end: { line, character: 1 } } };
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { referencesProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'textDocument/didOpen') {
    docs.set(msg.params.textDocument.uri, msg.params.textDocument.text);
  } else if (msg.method === 'textDocument/didChange') {
    docs.set(msg.params.textDocument.uri, msg.params.contentChanges[0].text);
  } else if (msg.method === 'textDocument/didClose') {
    docs.delete(msg.params.textDocument.uri);
  } else if (msg.method === 'textDocument/references') {
    count += 1;
    const result = [at(msg.params.textDocument.uri, count)];
    for (const [uri, text] of docs) {
      const line = text.split('\n').indexOf('MARK');
      if (line >= 0) result.push(at(uri, line));
    }
    send({ jsonrpc: '2.0', id: msg.id, result });
  } else if (msg.method === 'exit') {
    process.exit(0);
  } else if (msg.id !== undefined && msg.method) {
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

/// A document opened earlier and then edited on disk answers with its new
/// text, from a request anchored in another file, and no answer given
/// before the edit is reused after it; a deleted one is closed.
#[cfg(unix)]
#[test]
fn open_documents_follow_their_files_and_never_replay_older_answers() {
    use std::os::unix::fs::PermissionsExt;
    if !node_available() {
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let root = tmp.0.join("octocode-engine-open-documents");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let script = root.join("fake-lsp.js");
        std::fs::write(&script, DOCUMENT_SERVER).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let anchor = root.join("a.ts");
        std::fs::write(&anchor, "use();\n".repeat(10)).unwrap();
        let other = root.join("b.ts");
        std::fs::write(&other, "MARK\n").unwrap();
        let (anchor, other) = (
            anchor.to_string_lossy().into_owned(),
            other.to_string_lossy().into_owned(),
        );
        let client = NativeLspClient::new(LspLanguageServerConfig {
            command: script.to_string_lossy().into_owned(),
            args: Some(Vec::new()),
            workspace_root: root.to_string_lossy().into_owned(),
            language_id: Some("typescript".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
            source: Default::default(),
        });
        client.start().await.expect("fake server starts");
        client
            .open_document(other.clone(), "MARK\n")
            .await
            .expect("open b");
        // (anchor line = request count, b's MARK line when b is open)
        let ask = || {
            let client = &client;
            let anchor = anchor.clone();
            let scope = ResponseScope {
                reuse: true,
                generation: "g".into(),
                call: None,
            };
            RESPONSE_SCOPE.scope(std::cell::RefCell::new(scope), async move {
                let found = client
                    .get_locations(
                        LocationRequest::References {
                            include_declaration: true,
                        },
                        anchor.clone(),
                        0,
                        0,
                        &SnippetReadPolicy::default(),
                        None,
                    )
                    .await
                    .expect("references");
                let line = |name: &str| {
                    found
                        .iter()
                        .find(|snippet| snippet.uri.ends_with(name))
                        .map(|snippet| snippet.range.start.line)
                };
                (line("/a.ts"), line("/b.ts"))
            })
        };
        let first = ask().await;
        assert_eq!(first.1, Some(0), "b's synced text: {first:?}");
        assert_eq!(ask().await, first, "nothing changed: the answer is reused");

        std::fs::write(&other, "x\nx\nx\nMARK\n").unwrap();
        let edited = ask().await;
        assert_eq!(edited.1, Some(3), "b is answered from its new text");
        assert_ne!(edited.0, first.0, "the older answer is not replayed");
        assert_eq!(ask().await, edited, "the new answer is reused");

        std::fs::remove_file(&other).unwrap();
        let removed = ask().await;
        assert_eq!(removed.1, None, "a deleted file's document is closed");
        assert_ne!(removed.0, edited.0);
        client.stop().await.unwrap();
    });
}

#[tokio::test(start_paused = true)]
async fn settled_requests_wait_only_when_they_reach_the_server() {
    use super::document_sync::{
        client_with_fake_server_using, read_client_frame, send_server_frame,
    };
    let tmp = TmpDir::new();
    let root = tmp.0.join("octocode-engine-settled-reuse");
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("a.ts");
    std::fs::write(&source, "function foo() {}\nfoo();\n".repeat(10)).unwrap();
    let source_path = source
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let (client, mut reader, mut writer) =
        client_with_fake_server_using(NativeLspClient::new(ts_config()), 64 * 1024).await;
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&requests);
    let server =
        tokio::spawn(async move {
            loop {
                let request = read_client_frame(&mut reader).await;
                assert_eq!(request["method"], "textDocument/references");
                let line = seen.fetch_add(1, Ordering::SeqCst) + 1;
                send_server_frame(&mut writer, json!({"jsonrpc":"2.0","id":request["id"],"result":[{
                    "uri":request["params"]["textDocument"]["uri"],
                    "range":{"start":{"line":line,"character":0},"end":{"line":line,"character":1}}
                }]})).await;
            }
        });
    let settle = Duration::from_millis(200);
    let ask = |settled: Option<Duration>| {
        let client = &client;
        let source_path = source_path.clone();
        let scope = ResponseScope {
            reuse: true,
            generation: "g1".to_owned(),
            call: None,
        };
        RESPONSE_SCOPE.scope(std::cell::RefCell::new(scope), async move {
            let started = tokio::time::Instant::now();
            let line = client
                .get_locations(
                    LocationRequest::References {
                        include_declaration: true,
                    },
                    source_path,
                    0,
                    9,
                    &SnippetReadPolicy::default(),
                    settled,
                )
                .await
                .expect("references")[0]
                .range
                .start
                .line;
            (line, started.elapsed())
        })
    };
    let (first, _) = ask(None).await;
    // A settled request never takes the earlier (pre-settle) answer: it
    // waits, then asks the server.
    let (settled, waited) = ask(Some(settle)).await;
    assert_ne!(settled, first);
    assert_eq!(waited, settle);
    // Repeated under the same generation, both answers come from the
    // cache: no wait, no request (the server would answer a new line).
    let (again, waited) = ask(Some(settle)).await;
    assert_eq!(again, settled);
    assert_eq!(waited, Duration::ZERO);
    assert_eq!(ask(None).await.0, first);
    assert_eq!(
        requests.load(Ordering::SeqCst),
        2,
        "both cache hits avoid the provider"
    );
    server.abort();
    assert!(
        server
            .await
            .expect_err("mock task cancelled")
            .is_cancelled()
    );
}

/// The response cache weighs an entry by its JSON length, measured
/// once without building the text.
#[test]
fn cached_response_weight_is_the_json_length() {
    let value = json!({"uri": "file:///a.rs", "range": [1, 2, 3], "text": "q\"uote\u{e9}"});
    let entry = CachedResponse::new(Arc::new(value.clone()), None);
    assert_eq!(entry.json_bytes, value.to_string().len());
    assert_eq!(
        CachedResponse::new(Arc::new(Value::Null), None).json_bytes,
        4
    );
}
