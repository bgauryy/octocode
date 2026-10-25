//! Readiness waits after startup and after a first open.

use super::*;

/// Minimal stdio LSP server (node) that, like tsserver, emits NO progress on
/// `initialized` but starts a project-load `$/progress` wave ~100 ms AFTER the
/// first `didOpen`. `references` answers empty until that load finishes.
const DEFERRED_PROJECT_LOAD_SERVER: &str = r#"#!/usr/bin/env node
let buf = Buffer.alloc(0);
let loaded = false;
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function handle(msg) {
  if (msg.method === 'initialize') {
    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { referencesProvider: true, textDocumentSync: 1 } } });
  } else if (msg.method === 'textDocument/didOpen') {
    setTimeout(() => {
      send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'load', value: { kind: 'begin', title: 'Loading project' } } });
      setTimeout(() => {
        loaded = true;
        send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'load', value: { kind: 'end' } } });
      }, 400);
    }, 100);
  } else if (msg.method === 'textDocument/references') {
    const uri = msg.params.textDocument.uri;
    send({ jsonrpc: '2.0', id: msg.id, result: loaded
      ? [{ uri, range: { start: { line: 0, character: 9 }, end: { line: 0, character: 12 } } }]
      : [] });
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

#[test]
#[ignore = "opt-in process integration: Node server and wall-clock indexing"]
fn first_open_waits_for_the_project_load_the_open_triggers() {
    use std::os::unix::fs::PermissionsExt;
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let tmp = TmpDir::new();
        let root = tmp.0.join("octocode-engine-deferred-load");
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("fake-lsp.js");
        std::fs::write(&script, DEFERRED_PROJECT_LOAD_SERVER).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let source = root.join("a.ts");
        std::fs::write(&source, "function foo() {}\nfoo();\n").unwrap();
        let root_path = root.canonicalize().unwrap();
        let source_path = source
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();

        let client = NativeLspClient::new(LspLanguageServerConfig {
            command: script.to_string_lossy().into_owned(),
            args: Some(Vec::new()),
            workspace_root: root_path.to_string_lossy().into_owned(),
            language_id: Some("typescript".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
            source: Default::default(),
        });
        client.start().await.expect("fake server starts");
        // No progress on initialized: the initial readiness wait only settles.
        assert_eq!(
            client.wait_for_ready(Some(300)).await.unwrap(),
            "settledWithoutProgress"
        );

        let readiness = client
            .open_document_and_wait(
                source_path.clone(),
                "function foo() {}\nfoo();\n",
                Some(400),
                Some(5_000),
            )
            .await
            .expect("document syncs");
        assert_eq!(readiness.as_deref(), Some("progressIdle"));
        let references = client
            .get_locations(
                LocationRequest::References {
                    include_declaration: true,
                },
                source_path.clone(),
                0,
                9,
                &SnippetReadPolicy::default(),
                None,
            )
            .await
            .expect("references");
        assert_eq!(
            references.len(),
            1,
            "references must not race the load the didOpen triggered"
        );

        // A re-sync of an already-open document does not wait again.
        let again = client
            .open_document_and_wait(
                source_path,
                "function foo() {}\nfoo();\n",
                Some(400),
                Some(5_000),
            )
            .await
            .expect("document re-syncs");
        assert_eq!(again, None);
        client.stop().await.unwrap();
        let _ = std::fs::remove_dir_all(root);
    });
}

/// A readiness wait that times out while the server still indexes
/// does not leave a client that fails every later call. The next acquire
/// reuses the same process (no respawn) and waits again; it sees indexing
/// finish and the client answers. The re-wait does not pay the 2 s
/// silent-server settle on top of the elapsed timeout.
#[test]
#[ignore = "opt-in process integration: Node server and wall-clock indexing"]
fn readiness_timeout_client_waits_again_on_the_next_acquire() {
    use crate::lsp::pool::{LspClientPool, LspPoolOptions};
    if !node_available() {
        return;
    }
    let tmp = TmpDir::new();
    let pid_file = tmp.0.join("octocode-engine-readiness-timeout-pid");
    let (_tmp, root, mut config) = configurable_server(
        "octocode-engine-readiness-timeout",
        &[
            // Longer than the 2 s shellscript readiness budget, shorter
            // than two of them.
            ("FAKE_INDEX_MS", "3000".into()),
            ("FAKE_PID_FILE", pid_file.to_string_lossy().into_owned()),
        ],
        None,
    );
    config.language_id = Some("shellscript".into());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let pool = LspClientPool::new(LspPoolOptions::default());
        let first = pool
            .acquire(config.clone())
            .await
            .expect("start")
            .expect("client");
        assert_eq!(
            first.readiness().as_deref(),
            Some("timeout"),
            "indexing outlasts the first readiness budget"
        );
        let first_pid = std::fs::read_to_string(&pid_file).expect("server pid");

        let started = std::time::Instant::now();
        let second = pool
            .acquire(config.clone())
            .await
            .expect("acquire")
            .expect("client");
        let waited = started.elapsed();
        assert!(second.same_client(&first), "same pooled server, no respawn");
        assert_eq!(
            std::fs::read_to_string(&pid_file).expect("server pid"),
            first_pid,
            "no second server process"
        );
        assert_eq!(
            second.readiness().as_deref(),
            Some("progressIdle"),
            "the next acquire waits again instead of keeping the stored timeout"
        );
        assert!(
            waited < Duration::from_millis(2_000),
            "the re-wait ends with the indexing, without a second settle: {waited:?}"
        );
        second
            .get_hover(root.join("a.sh").to_string_lossy().into_owned(), 0, 0)
            .await
            .expect("the resumed client answers");

        // A ready client is handed out again without any wait.
        let started = std::time::Instant::now();
        let third = pool
            .acquire(config)
            .await
            .expect("acquire")
            .expect("client");
        assert!(third.same_client(&first));
        assert!(started.elapsed() < Duration::from_millis(500));
        pool.clear_all().await;
    });
    let _ = std::fs::remove_file(pid_file);
    let _ = std::fs::remove_dir_all(root);
}

/// Once the startup readiness wait saw the project load finish
/// (`progressIdle`), a first `didOpen` does not stack the 400 ms settle on
/// it. A server that is silent at startup keeps the settle (see
/// `first_open_waits_for_the_project_load_the_open_triggers`).
#[test]
#[ignore = "opt-in process integration: Node server and wall-clock indexing"]
fn first_open_after_a_confirmed_project_load_skips_the_settle() {
    if !node_available() {
        return;
    }
    let (_tmp, root, config) = configurable_server(
        "octocode-engine-open-after-index",
        &[("FAKE_INDEX_MS", "50".into())],
        None,
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = NativeLspClient::new(config);
        client.start().await.expect("fake server starts");
        assert_eq!(
            client.wait_for_ready(Some(5_000)).await.unwrap(),
            "progressIdle"
        );
        let file = root.join("a.sh").to_string_lossy().into_owned();
        let started = std::time::Instant::now();
        let readiness = client
            .open_document_and_wait(file, "echo hi\n", Some(400), Some(5_000))
            .await
            .expect("document syncs");
        let waited = started.elapsed();
        assert_eq!(readiness.as_deref(), Some("settledWithoutProgress"));
        assert!(
            waited < Duration::from_millis(400),
            "no settle stacked on a confirmed project load: {waited:?}"
        );
        client.stop().await.unwrap();
    });
    let _ = std::fs::remove_dir_all(root);
}

/// A client whose options cap the settle waits never waits longer than the
/// cap for a server that reports no progress: the startup settle, the
/// first-open settle and a settled request's delay all end at the cap. A
/// harness of fake servers that answer at once sets it to zero.
#[test]
#[ignore = "opt-in process integration: Node server and wall-clock indexing"]
fn a_settle_cap_bounds_every_silent_server_wait() {
    if !node_available() {
        return;
    }
    let (_tmp, root, config) = configurable_server("octocode-engine-settle-cap", &[], None);
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = NativeLspClient::with_options(
            config,
            LspClientOptions {
                settle_cap: Some(Duration::ZERO),
            },
        );
        client.start().await.expect("fake server starts");
        let started = std::time::Instant::now();
        assert_eq!(
            client.wait_for_ready(Some(5_000)).await.unwrap(),
            "settledWithoutProgress"
        );
        let file = root.join("a.sh").to_string_lossy().into_owned();
        let readiness = client
            .open_document_and_wait(file.clone(), "echo hi\n", Some(400), Some(5_000))
            .await
            .expect("document syncs");
        assert_eq!(readiness.as_deref(), Some("settledWithoutProgress"));
        client
            .get_locations(
                LocationRequest::Definition,
                file,
                0,
                0,
                &SnippetReadPolicy::default(),
                Some(Duration::from_millis(400)),
            )
            .await
            .expect("definition");
        let waited = started.elapsed();
        assert!(
            waited < Duration::from_millis(400),
            "every settle ends at the zero cap: {waited:?}"
        );
        client.stop().await.unwrap();
    });
    let _ = std::fs::remove_dir_all(root);
}
