use super::*;
use std::path::PathBuf;

#[test]
fn node_shebang_launch_uses_node_instead_of_native_host() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let script = temp_file("octocode-engine-node-shebang");
        std::fs::write(
            &script,
            "#!/usr/bin/env node\nprocess.stdout.write(JSON.stringify(process.argv.slice(2)))\n",
        )
        .unwrap();
        let mut args = vec!["--stdio".to_owned(), "argument with spaces".to_owned()];
        let program = lsp_spawn_program(&script.to_string_lossy(), &mut args)
            .await
            .unwrap();
        let output = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::process::Command::new(program)
                .args(args)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        std::fs::remove_file(script).unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            r#"["--stdio","argument with spaces"]"#
        );
    });
}

#[test]
fn location_links_select_the_symbol_and_preserve_definition_context() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let file_path = temp_file("octocode-engine-location-link");
        std::fs::write(&file_path, "// context\nfunction target() {\n  return 1;\n}\n").unwrap();
        let uri = path_to_uri(&file_path.to_string_lossy()).unwrap();
        let selection = json!({"start":{"line":1,"character":9},"end":{"line":1,"character":15}});
        let locations = snippets_from_locations(json!([
            {"targetUri":uri,"targetRange":{"start":{"line":0,"character":0},"end":{"line":4,"character":0}},"targetSelectionRange":selection},
            {"uri":uri,"range":selection}
        ])).await.unwrap();
        std::fs::remove_file(file_path).unwrap();
        assert_eq!(locations.len(), 2);
        for location in &locations {
            assert_eq!(location.range.start.line, 1);
            assert_eq!(location.range.start.character, 9);
            assert_eq!(location.range.end.character, 15);
        }
        assert_eq!(locations[0].content, "// context\nfunction target() {\n  return 1;\n}");
        assert_eq!(locations[1].content, "function target() {");
        assert_eq!(locations[0].display_range, Some(json!({"startLine":1,"endLine":4})));
        assert!(locations[1].display_range.is_none());
    });
}

// `sh` is used to spawn a real, minimal child process for the two tests
// below rather than a synthetic stand-in — the property under test
// (`wait_for_graceful_exit` actually observing/killing a real OS process) is
// not meaningfully testable without one. Gated to unix: this crate's CI runs
// `cargo test` only on its native host (macOS/Linux); win32-x64-msvc is a
// cross-compiled release target that is never executed in CI, and `sh` is
// not guaranteed on a developer's native Windows machine.
#[cfg(unix)]
#[test]
fn wait_for_graceful_exit_does_not_kill_a_promptly_exiting_process() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "exit 0"])
            .kill_on_drop(true)
            .spawn()
            .expect("spawn sh");

        let exited_on_own =
            wait_for_graceful_exit(&mut child, tokio::time::Duration::from_millis(2_000)).await;

        assert!(
            exited_on_own,
            "expected the process to exit on its own within the bound, not be killed"
        );
    });
}

#[cfg(unix)]
#[test]
fn wait_for_graceful_exit_kills_a_process_that_ignores_the_window() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "sleep 30"])
            .kill_on_drop(true)
            .spawn()
            .expect("spawn sh");

        let exited_on_own =
            wait_for_graceful_exit(&mut child, tokio::time::Duration::from_millis(50)).await;

        assert!(
            !exited_on_own,
            "expected escalation to kill for a process that outlives the graceful window"
        );
        // Confirm the process was actually terminated, not just abandoned —
        // it must be reapable promptly after the kill.
        let status = tokio::time::timeout(std::time::Duration::from_secs(2), child.wait())
            .await
            .expect("process should be reaped promptly after kill")
            .expect("wait should succeed");
        assert!(!status.success());
    });
}

#[test]
fn graceful_exit_sweeps_process_group_on_the_clean_exit_path_too() {
    // The bug: the process-group sweep ran ONLY on the timeout branch, so a
    // server that exits cleanly leaked its descendants (proc-macro-srv,
    // cargo/build scripts, clangd workers). The sweep must run on BOTH paths;
    // the hard kill only when the process outlived the window.
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        // Clean-exit path: exited == true. Sweep MUST run; hard-kill MUST NOT.
        let swept = std::cell::Cell::new(0u32);
        let hard_killed = std::cell::Cell::new(0u32);
        finish_graceful_exit(
            true,
            || swept.set(swept.get() + 1),
            || async { hard_killed.set(hard_killed.get() + 1) },
        )
        .await;
        assert_eq!(
            swept.get(),
            1,
            "process-group sweep must run on the clean-exit path"
        );
        assert_eq!(
            hard_killed.get(),
            0,
            "no hard kill when the process exited on its own"
        );

        // Timeout path: exited == false. Both sweep and hard-kill run.
        let swept = std::cell::Cell::new(0u32);
        let hard_killed = std::cell::Cell::new(0u32);
        finish_graceful_exit(
            false,
            || swept.set(swept.get() + 1),
            || async { hard_killed.set(hard_killed.get() + 1) },
        )
        .await;
        assert_eq!(swept.get(), 1, "sweep still runs on the timeout path");
        assert_eq!(
            hard_killed.get(),
            1,
            "a process that outlived the window is hard-killed"
        );
    });
}

#[test]
fn content_modified_detected_by_error_code() {
    let error = Error::new(
        Status::GenericFailure,
        "LSP error: {\"code\":-32801,\"message\":\"content modified\"}".to_owned(),
    );
    assert!(is_content_modified_error(&error));
}

#[test]
fn content_modified_detected_with_spaced_code() {
    let error = Error::new(
        Status::GenericFailure,
        "LSP error: {\"code\" : -32801 , \"message\":\"x\"}".to_owned(),
    );
    assert!(is_content_modified_error(&error));
}

#[test]
fn content_modified_not_triggered_by_phrase_in_payload() {
    // A hover/result payload that merely mentions the phrase, with a
    // different (or no) error code, must NOT be treated as ContentModified.
    let error = Error::new(
        Status::GenericFailure,
        "LSP error: {\"code\":-32603,\"message\":\"docs say: content modified by user\"}"
            .to_owned(),
    );
    assert!(!is_content_modified_error(&error));
}

#[test]
fn content_modified_not_triggered_by_substring_in_other_code() {
    // The digits -32801 appearing inside a larger number must not match.
    let error = Error::new(
        Status::GenericFailure,
        "LSP error: {\"code\":-328011,\"message\":\"x\"}".to_owned(),
    );
    assert!(!is_content_modified_error(&error));
}

#[test]
fn stderr_ring_keeps_only_recent_lines() {
    let lines = Arc::new(StdMutex::new(VecDeque::new()));

    for index in 0..(STDERR_RING_CAPACITY + 5) {
        push_stderr_line(&lines, format!("line-{index}"));
    }

    let lines = lines.lock().expect("stderr ring lock");
    assert_eq!(lines.len(), STDERR_RING_CAPACITY);
    assert_eq!(lines.front().map(String::as_str), Some("line-5"));
    assert_eq!(
        lines.back().map(String::as_str),
        Some(format!("line-{}", STDERR_RING_CAPACITY + 4).as_str())
    );
}

#[test]
fn stderr_ring_truncates_very_long_lines() {
    let lines = Arc::new(StdMutex::new(VecDeque::new()));

    push_stderr_line(&lines, "x".repeat(STDERR_LINE_MAX_CHARS + 10));

    let line = lines
        .lock()
        .expect("stderr ring lock")
        .front()
        .cloned()
        .expect("stderr line");
    assert_eq!(line.chars().count(), STDERR_LINE_MAX_CHARS + 3);
    assert!(line.ends_with("..."));
}

#[test]
fn snippet_content_cache_reuses_file_content_for_later_ranges() {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        let file_path = temp_file("octocode-engine-snippet-cache");
        std::fs::write(&file_path, "alpha\nbeta\ngamma\n").expect("write fixture");
        let file_path = file_path.to_string_lossy().into_owned();
        let mut cache = SnippetContentCache::default();

        let first = cache
            .read_range_content(&file_path, &range(0, 0))
            .await
            .expect("first range");
        std::fs::remove_file(&file_path).expect("remove fixture");
        let second = cache
            .read_range_content(&file_path, &range(1, 2))
            .await
            .expect("second range");

        assert_eq!(first, "alpha");
        assert_eq!(second, "beta\ngamma");
        assert_eq!(cache.files.len(), 1);
    });
}

/// Build a range covering whole lines `start_line..=end_line` *inclusive*.
/// LSP ranges are end-exclusive, so the end is the start of the line after
/// `end_line`.
fn range(start_line: u32, end_line: u32) -> JsRange {
    JsRange {
        start: JsExactPosition {
            line: start_line,
            character: 0,
        },
        end: JsExactPosition {
            line: end_line + 1,
            character: 0,
        },
    }
}

#[test]
fn extract_position_encoding_reads_server_choice() {
    // Server echoes the negotiated encoding.
    let result = json!({ "capabilities": { "positionEncoding": "utf-16" } });
    assert_eq!(
        extract_position_encoding(&result).as_deref(),
        Some("utf-16")
    );

    // A non-conformant server that ignored our utf-16-only advertisement.
    let result = json!({ "capabilities": { "positionEncoding": "utf-8" } });
    assert_eq!(extract_position_encoding(&result).as_deref(), Some("utf-8"));

    // Omitted ⇒ None (spec default is utf-16).
    let result = json!({ "capabilities": {} });
    assert_eq!(extract_position_encoding(&result), None);

    // No capabilities at all.
    assert_eq!(extract_position_encoding(&json!({})), None);
}

#[test]
fn parse_position_requires_numeric_line_and_character() {
    assert!(parse_position(&json!({"line": 3, "character": 7})).is_ok());
    assert!(parse_position(&json!({"line": 3})).is_err());
    assert!(parse_position(&json!({"character": 7})).is_err());
    assert!(parse_position(&json!({"line": "x", "character": 1})).is_err());
}

#[test]
fn slice_range_excludes_end_line_when_end_character_is_zero() {
    // LSP end-exclusive: {start:{0,0}, end:{2,0}} covers lines 0–1 only.
    let content = "line0\nline1\nline2\nline3\n";
    let r = JsRange {
        start: JsExactPosition {
            line: 0,
            character: 0,
        },
        end: JsExactPosition {
            line: 2,
            character: 0,
        },
    };
    assert_eq!(slice_range_content(content, &r), "line0\nline1");
}

#[test]
fn slice_range_includes_end_line_when_end_character_positive() {
    // A single-line range keeps the whole line (snippet context), not just
    // the [start.character, end.character) span.
    let content = "alpha\nbeta\n";
    let r = JsRange {
        start: JsExactPosition {
            line: 1,
            character: 2,
        },
        end: JsExactPosition {
            line: 1,
            character: 4,
        },
    };
    assert_eq!(slice_range_content(content, &r), "beta");
}

#[test]
fn graph_server_receipt_is_stable_without_exposing_session_handles() {
    let client = NativeLspClient::new(JsLanguageServerConfig {
        command: "/opt/bin/rust-analyzer".to_owned(),
        args: Some(vec!["--stdio".to_owned()]),
        workspace_root: "/workspace".to_owned(),
        language_id: Some("rust".to_owned()),
        initialization_options: Some(json!({"cargo":{"features":"all"}})),
        env: None,
        max_memory_mb: None,
    });
    let first = client.graph_server_receipt();
    let second = client.graph_server_receipt();
    assert_eq!(first.family, "rust-analyzer");
    assert_eq!(first.configuration_digest, second.configuration_digest);
    assert!(!first.configuration_digest.is_empty());
    assert!(first.capabilities.is_empty());
    assert_eq!(client.document_version("/workspace/src/lib.rs"), None);
}

#[test]
fn open_documents_evicts_least_recently_used_when_over_cap() {
    // A long session must not grow `open_docs` (and the server-side document
    // set) without bound. Over the cap, the least-recently-synced document is
    // evicted and its URI is returned so the caller can emit a `didClose`.
    let mut docs = OpenDocuments::new(2);

    let (version_a, evicted) = docs.reserve("file:///a");
    assert_eq!(version_a, 1);
    assert!(evicted.is_none());

    let (_, evicted) = docs.reserve("file:///b");
    assert!(evicted.is_none());
    assert_eq!(docs.len(), 2);

    // Touch `a` so `b` becomes the least-recently-used document.
    let (version_a2, evicted) = docs.reserve("file:///a");
    assert_eq!(version_a2, 2, "a re-open bumps the document version");
    assert!(evicted.is_none());

    // Opening a third distinct document exceeds the cap: `b` (LRU) is evicted
    // and returned as the didClose target; the map stays bounded.
    let (version_c, evicted) = docs.reserve("file:///c");
    assert_eq!(version_c, 1);
    assert_eq!(
        evicted.as_deref(),
        Some("file:///b"),
        "the least-recently-used document is the eviction/didClose target"
    );
    assert_eq!(docs.len(), 2, "the open-document map is bounded by the cap");
    assert_eq!(docs.version("file:///b"), None);
    assert_eq!(docs.version("file:///a"), Some(2));
    assert_eq!(docs.version("file:///c"), Some(1));

    // The currently-syncing document is never chosen as its own eviction victim.
    let (_, evicted) = docs.reserve("file:///c");
    assert_ne!(evicted.as_deref(), Some("file:///c"));
}

#[test]
fn open_documents_rollback_restores_prior_version_state() {
    let mut docs = OpenDocuments::new(4);
    // A failed first sync (didOpen) rolls the URI back out entirely.
    let (v1, _) = docs.reserve("file:///a");
    docs.rollback("file:///a", v1);
    assert_eq!(docs.version("file:///a"), None);

    // A failed later sync (didChange) rolls the version back by one.
    docs.reserve("file:///b");
    let (v2, _) = docs.reserve("file:///b");
    assert_eq!(v2, 2);
    docs.rollback("file:///b", v2);
    assert_eq!(docs.version("file:///b"), Some(1));
}

fn temp_file(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
}

/// Minimal stdio LSP server (node) that, like tsserver, emits NO progress on
/// `initialized` but starts a project-load `$/progress` wave ~100 ms AFTER the
/// first `didOpen`. `references` answers empty until that load finishes.
#[cfg(unix)]
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

#[cfg(unix)]
#[test]
fn first_open_waits_for_the_project_load_the_open_triggers() {
    use std::os::unix::fs::PermissionsExt;
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let root = temp_file("octocode-engine-deferred-load");
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

        let client = NativeLspClient::new(JsLanguageServerConfig {
            command: script.to_string_lossy().into_owned(),
            args: Some(Vec::new()),
            workspace_root: root_path.to_string_lossy().into_owned(),
            language_id: Some("typescript".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
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
                "function foo() {}\nfoo();\n".into(),
                Some(400),
                Some(5_000),
            )
            .await
            .expect("document syncs");
        assert_eq!(readiness.as_deref(), Some("progressIdle"));
        let references = client
            .get_references(source_path.clone(), 0, 9, Some(true))
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
                "function foo() {}\nfoo();\n".into(),
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

#[test]
fn initialize_advertises_push_diagnostics_so_servers_publish_them() {
    let config = JsLanguageServerConfig {
        command: "typescript-language-server".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: std::env::temp_dir().to_string_lossy().into_owned(),
        language_id: Some("typescript".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
    };
    let params = initialize_params(&config).expect("initialize params");
    assert_eq!(
        params.pointer("/capabilities/textDocument/publishDiagnostics/versionSupport"),
        Some(&json!(true)),
        "{params}"
    );
}
