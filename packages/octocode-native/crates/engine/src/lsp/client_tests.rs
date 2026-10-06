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
        ]), &SnippetReadPolicy::default()).await.unwrap();
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

fn rpc_error(error: Value) -> Error {
    Error::rpc(crate::error::RpcError::from_value(error))
}

#[test]
fn content_modified_detected_by_typed_error_code() {
    assert!(is_retryable_error(&rpc_error(
        json!({"code": -32801, "message": "content modified"})
    )));
}

#[test]
fn content_modified_not_triggered_by_phrase_in_payload() {
    // A payload that merely mentions the phrase or the number, with a
    // different code, must NOT be treated as ContentModified.
    assert!(!is_retryable_error(&rpc_error(json!({
        "code": -32603,
        "message": "docs say: content modified by user (-32801)"
    }))));
}

#[test]
fn content_modified_not_triggered_by_other_codes_or_untyped_text() {
    assert!(!is_retryable_error(&rpc_error(
        json!({"code": -328011, "message": "x"})
    )));
    // Rendered text is never parsed back into a code.
    assert!(!is_retryable_error(&Error::new(
        "LSP error: {\"code\":-32801,\"message\":\"content modified\"}",
    )));
}

#[test]
fn server_cancelled_is_retried_only_when_the_server_asks_for_a_retrigger() {
    assert!(is_retryable_error(&rpc_error(json!({
        "code": -32802, "message": "cancelled", "data": {"retriggerRequest": true}
    }))));
    assert!(!is_retryable_error(&rpc_error(
        json!({"code": -32802, "message": "cancelled"})
    )));
    assert!(!is_retryable_error(&rpc_error(json!({
        "code": -32802, "message": "cancelled", "data": {"retriggerRequest": false}
    }))));
    assert!(!is_retryable_error(&rpc_error(
        json!({"code": -32800, "message": "request cancelled"})
    )));
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
        let mut cache = SnippetContentCache::new(SnippetReadPolicy::default());

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
fn parse_position_rejects_values_beyond_u32_instead_of_wrapping() {
    let too_big = u64::from(u32::MAX) + 1;
    let error = parse_position(&json!({"line": too_big, "character": 0}))
        .expect_err("line overflow must not wrap to 0");
    assert!(error.reason.contains("line"), "{}", error.reason);
    assert!(parse_position(&json!({"line": 0, "character": too_big})).is_err());
    let max =
        parse_position(&json!({"line": u32::MAX, "character": u32::MAX})).expect("u32::MAX fits");
    assert_eq!((max.line, max.character), (u32::MAX, u32::MAX));
}

fn cached(content: &str) -> CachedSource {
    CachedSource {
        content: content.to_owned(),
        lines: LineIndex::new(content),
    }
}

#[test]
fn slice_range_breaks_lines_on_crlf_and_lone_cr_like_the_server() {
    for content in ["a0\r\na1\r\na2\r\n", "a0\ra1\ra2\r", "a0\na1\r\na2\r"] {
        let source = cached(content);
        assert_eq!(
            slice_range_content(&source, &range(1, 1)),
            "a1",
            "{content:?}"
        );
        assert_eq!(
            slice_range_content(&source, &range(0, 2)),
            "a0\na1\na2",
            "{content:?}"
        );
        // Past the last content line: nothing, and no trailing empty line.
        assert_eq!(
            slice_range_content(&source, &range(3, 3)),
            "",
            "{content:?}"
        );
        assert_eq!(
            slice_range_content(&source, &range(2, 9)),
            "a2",
            "{content:?}"
        );
    }
    let source = cached("x\r😀 é\ry");
    let emoji_line = JsRange {
        start: JsExactPosition {
            line: 1,
            character: 3,
        },
        end: JsExactPosition {
            line: 1,
            character: 4,
        },
    };
    assert_eq!(slice_range_content(&source, &emoji_line), "😀 é");
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
    assert_eq!(slice_range_content(&cached(content), &r), "line0\nline1");
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
    assert_eq!(slice_range_content(&cached(content), &r), "beta");
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
            .get_locations(
                LocationRequest::References {
                    include_declaration: true,
                },
                source_path.clone(),
                0,
                9,
                &SnippetReadPolicy::default(),
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

fn ts_config() -> JsLanguageServerConfig {
    JsLanguageServerConfig {
        command: "typescript-language-server".into(),
        args: Some(vec!["--stdio".into()]),
        workspace_root: std::env::temp_dir().to_string_lossy().into_owned(),
        language_id: Some("typescript".into()),
        initialization_options: None,
        env: None,
        max_memory_mb: None,
    }
}

#[test]
fn initialize_declares_only_what_the_client_handles() {
    let params = initialize_params(&ts_config()).expect("initialize params");
    let stale = params
        .pointer("/capabilities/general/staleRequestSupport")
        .expect("staleRequestSupport");
    assert_eq!(stale["cancel"], json!(true));
    let retried: Vec<&str> = stale["retryOnContentModified"]
        .as_array()
        .expect("method list")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for method in [
        "textDocument/definition",
        "textDocument/references",
        "textDocument/hover",
        "callHierarchy/incomingCalls",
        "workspace/symbol",
        "textDocument/diagnostic",
    ] {
        assert!(
            retried.contains(&method),
            "{method} is retried: {retried:?}"
        );
    }
    // We never send didSave, so we must not claim it.
    assert_eq!(
        params.pointer("/capabilities/textDocument/synchronization/didSave"),
        None
    );
    // serverStatus is rust-analyzer's extension only.
    assert_eq!(params.pointer("/capabilities/experimental"), None);
    assert_eq!(
        params.pointer("/capabilities/general/positionEncodings"),
        Some(&json!(["utf-16"]))
    );

    let mut rust = ts_config();
    rust.command = "/home/u/.cargo/bin/rust-analyzer".into();
    let params = initialize_params(&rust).expect("initialize params");
    assert_eq!(
        params.pointer("/capabilities/experimental/serverStatusNotification"),
        Some(&json!(true))
    );
}

#[test]
fn rust_analyzer_client_starts_headless_unless_the_caller_overrides() {
    let mut config = ts_config();
    config.command = "rust-analyzer".into();
    let client = NativeLspClient::new(config.clone());
    let options = client
        .inner
        .config
        .initialization_options
        .clone()
        .expect("headless defaults");
    assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));
    assert_eq!(
        options.pointer("/cargo/buildScripts/enable"),
        Some(&json!(false))
    );
    assert_eq!(options.pointer("/checkOnSave"), Some(&json!(false)));
    assert_eq!(options.pointer("/cachePriming/enable"), Some(&json!(false)));

    config.initialization_options = Some(json!({"cargo": {"buildScripts": {"enable": true}}}));
    let client = NativeLspClient::new(config);
    let options = client
        .inner
        .config
        .initialization_options
        .clone()
        .expect("options");
    assert_eq!(
        options.pointer("/cargo/buildScripts/enable"),
        Some(&json!(true))
    );
    assert_eq!(options.pointer("/procMacro/enable"), Some(&json!(false)));

    // Other servers are untouched.
    assert!(
        NativeLspClient::new(ts_config())
            .inner
            .config
            .initialization_options
            .is_none()
    );
}

#[test]
fn stderr_drain_bounds_a_giant_line_and_keeps_reading() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut input = vec![b'x'; 4 * 1024 * 1024];
        input.extend_from_slice(b"\r\nnext line\n");
        let lines = Arc::new(StdMutex::new(VecDeque::new()));
        drain_stderr(input.as_slice(), Arc::clone(&lines)).await;
        let lines = lines.lock().unwrap();
        assert_eq!(
            lines.len(),
            2,
            "{:?}",
            lines.iter().map(String::len).collect::<Vec<_>>()
        );
        assert!(lines[0].len() <= STDERR_LINE_MAX_CHARS + 3);
        assert!(lines[0].ends_with("..."));
        assert_eq!(lines[1], "next line");
    });
}

#[test]
fn stderr_drain_survives_invalid_utf8() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let input: &[u8] = b"bad \xff\xfe bytes\nstill draining\nlast without newline";
        let lines = Arc::new(StdMutex::new(VecDeque::new()));
        drain_stderr(input, Arc::clone(&lines)).await;
        let lines = lines.lock().unwrap();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("bad ") && lines[0].contains('\u{FFFD}'));
        assert_eq!(lines[1], "still draining");
        assert_eq!(lines[2], "last without newline");
    });
}

#[test]
fn capped_line_reader_discards_the_overflow_without_buffering_it() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let input = [vec![b'a'; 100], b"\nb\n".to_vec()].concat();
        let mut reader = BufReader::with_capacity(16, input.as_slice());
        let mut line = Vec::new();
        let (consumed, truncated) = read_capped_line(&mut reader, &mut line, 10).await.unwrap();
        assert_eq!((consumed, truncated, line.len()), (101, true, 10));
        line.clear();
        let (consumed, truncated) = read_capped_line(&mut reader, &mut line, 10).await.unwrap();
        assert_eq!(
            (consumed, truncated, line.as_slice()),
            (2, false, &b"b"[..])
        );
        line.clear();
        assert_eq!(
            read_capped_line(&mut reader, &mut line, 10)
                .await
                .unwrap()
                .0,
            0
        );
    });
}

#[cfg(unix)]
fn make_fifo(path: &std::path::Path) {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    // SAFETY: `c_path` is a valid NUL-terminated path for the call's duration.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) }, 0, "mkfifo");
}

#[cfg(unix)]
#[test]
fn snippet_reads_reject_fifos_and_devices_without_blocking() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let fifo = temp_file("octocode-engine-snippet-fifo");
        make_fifo(&fifo);
        for path in [fifo.to_string_lossy().into_owned(), "/dev/zero".to_owned()] {
            let mut cache = SnippetContentCache::new(SnippetReadPolicy::default());
            let result = tokio::time::timeout(
                Duration::from_secs(10),
                cache.read_range_content(&path, &range(0, 0)),
            )
            .await
            .expect("a non-regular file must be refused, not read or opened");
            let error = result.expect_err("non-regular file rejected");
            assert!(
                error.reason.contains("not a regular file"),
                "{path}: {}",
                error.reason
            );
        }
        let _ = std::fs::remove_file(fifo);
    });
}

#[test]
fn snippet_reads_reject_oversized_files_before_reading() {
    let path = temp_file("octocode-engine-snippet-oversized");
    let file = std::fs::File::create(&path).unwrap();
    // Sparse: the size check must refuse it from metadata alone.
    file.set_len(MAX_SNIPPET_SOURCE_BYTES + 1).unwrap();
    drop(file);
    let error = read_bounded_regular_file(&path, MAX_SNIPPET_SOURCE_BYTES)
        .expect_err("oversized file rejected");
    assert!(error.reason.contains("too large"), "{}", error.reason);
    // The same bound applies through `take`, not only through metadata.
    std::fs::write(&path, "abcdef").unwrap();
    assert!(read_bounded_regular_file(&path, 5).is_err());
    assert_eq!(read_bounded_regular_file(&path, 6).unwrap(), "abcdef");
    std::fs::remove_file(path).unwrap();
}

#[cfg(unix)]
#[test]
fn unauthorized_snippet_paths_are_never_touched() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        // A FIFO would block any open or read: reaching it proves the policy
        // was consulted first.
        let fifo = temp_file("octocode-engine-snippet-unauthorized");
        make_fifo(&fifo);
        let allowed = temp_file("octocode-engine-snippet-authorized");
        std::fs::write(&allowed, "allowed line\n").unwrap();
        let consulted = Arc::new(AtomicUsize::new(0));
        let policy = {
            let consulted = Arc::clone(&consulted);
            let allowed = allowed.clone();
            SnippetReadPolicy::with_authorizer(move |path| {
                consulted.fetch_add(1, Ordering::SeqCst);
                (path == allowed).then(|| path.to_path_buf())
            })
        };
        let fifo_uri = path_to_uri(&fifo.to_string_lossy()).unwrap();
        let allowed_uri = path_to_uri(&allowed.to_string_lossy()).unwrap();
        let at = json!({"start":{"line":0,"character":0},"end":{"line":0,"character":3}});
        let locations = tokio::time::timeout(
            Duration::from_secs(10),
            snippets_from_locations(
                json!([
                    {"uri": fifo_uri, "range": at},
                    {"uri": fifo_uri, "range": at},
                    {"uri": allowed_uri, "range": at}
                ]),
                &policy,
            ),
        )
        .await
        .expect("an unauthorized path is never opened")
        .expect("locations");
        assert_eq!(
            locations.len(),
            3,
            "refused locations are kept, without content"
        );
        assert_eq!(locations[0].content, SNIPPET_CONTENT_WITHHELD);
        assert_eq!(locations[1].content, SNIPPET_CONTENT_WITHHELD);
        assert_eq!(locations[2].content, "allowed line");
        assert_eq!(consulted.load(Ordering::SeqCst), 2, "one decision per file");
        let _ = std::fs::remove_file(fifo);
        let _ = std::fs::remove_file(allowed);
    });
}

#[test]
fn snippet_policy_reads_the_path_the_authorizer_returns() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let real = temp_file("octocode-engine-snippet-canonical");
        std::fs::write(&real, "canonical content\n").unwrap();
        let target = real.clone();
        let policy = SnippetReadPolicy::with_authorizer(move |_| Some(target.clone()));
        let mut cache = SnippetContentCache::new(policy);
        let content = cache
            .read_range_content("/server/said/elsewhere.rs", &range(0, 0))
            .await
            .unwrap();
        assert_eq!(content, "canonical content");
        std::fs::remove_file(real).unwrap();
    });
}

#[test]
fn lease_marks_the_client_busy_until_every_holder_releases() {
    let client = NativeLspClient::new(ts_config());
    assert!(!client.is_busy());
    let first = client.lease();
    let clone = client.clone();
    let second = clone.lease();
    assert!(client.is_busy() && clone.is_busy());
    drop(first);
    assert!(client.is_busy(), "one lease still held");
    // Owned + 'static: a lease can move into another task.
    let moved = std::thread::spawn(move || drop(second));
    moved.join().unwrap();
    assert!(!client.is_busy());
}

/// A client wired to an in-memory server instead of a spawned process.
async fn client_with_fake_server(
    client_to_server: usize,
) -> (
    NativeLspClient,
    BufReader<tokio::io::DuplexStream>,
    tokio::io::DuplexStream,
) {
    let (client_w, server_r) = tokio::io::duplex(client_to_server);
    let (server_w, client_r) = tokio::io::duplex(64 * 1024);
    let client = NativeLspClient::new(ts_config());
    let connection = JsonRpcConnection::new(
        client_r,
        client_w,
        ClientRequestContext {
            configuration: json!({}),
            section_root: None,
            workspace_folders: json!([]),
        },
        Arc::clone(&client.inner.progress),
    );
    *client.inner.connection.lock().await = Some(Arc::new(connection));
    (client, BufReader::new(server_r), server_w)
}

async fn read_client_frame(reader: &mut BufReader<tokio::io::DuplexStream>) -> Value {
    use tokio::io::AsyncReadExt;
    let mut length = None;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).await.expect("header");
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length:") {
            length = Some(value.trim().parse::<usize>().expect("length"));
        }
    }
    let mut body = vec![0; length.expect("Content-Length")];
    reader.read_exact(&mut body).await.expect("body");
    serde_json::from_slice(&body).expect("json")
}

#[test]
fn concurrent_syncs_of_one_document_reach_the_server_in_version_order() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        // Large bodies widen the window between version reservation and the
        // write (JSON encoding happens in between), so without the per-client
        // sync lock the workers reliably interleave out of order.
        const SYNCS: usize = 24;
        const BODY: usize = 512 * 1024;
        // Several rounds (one fresh document each) so an unordered
        // implementation is caught with high probability.
        const ROUNDS: usize = 4;
        let (client, mut server, _server_w) = client_with_fake_server(64 * 1024).await;
        for round in 0..ROUNDS {
            let file = std::env::temp_dir().join(format!("octocode-sync-order-{round}.ts"));
            let file = file.to_string_lossy().into_owned();
            let reader = tokio::spawn(async move {
                let mut frames = Vec::new();
                for _ in 0..SYNCS {
                    frames.push(read_client_frame(&mut server).await);
                }
                (frames, server)
            });
            let tasks: Vec<_> = (0..SYNCS)
                .map(|index| {
                    let client = client.clone();
                    let file = file.clone();
                    tokio::spawn(async move {
                        let mut body = format!("// {index}\n");
                        body.push_str(&"x".repeat(BODY));
                        client.open_document(file, body).await
                    })
                })
                .collect();
            for task in tasks {
                task.await.unwrap().expect("sync");
            }
            let (frames, returned) = tokio::time::timeout(Duration::from_secs(30), reader)
                .await
                .expect("frames")
                .unwrap();
            server = returned;
            assert_eq!(frames[0]["method"], "textDocument/didOpen", "round {round}");
            assert_eq!(frames[0]["params"]["textDocument"]["version"], 1);
            for (index, frame) in frames.iter().enumerate().skip(1) {
                assert_eq!(
                    frame["method"], "textDocument/didChange",
                    "round {round} frame {index}"
                );
                assert_eq!(
                    frame["params"]["textDocument"]["version"],
                    json!(index + 1),
                    "round {round}: versions must arrive in order"
                );
            }
        }
        assert!(!client.is_busy());
    });
}

#[test]
fn a_blocked_sync_keeps_the_client_busy_and_cancelling_it_releases() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        // A 1-byte pipe the server never reads: the didOpen write stalls.
        let (client, _server, _server_w) = client_with_fake_server(1).await;
        let syncing = {
            let client = client.clone();
            tokio::spawn(async move {
                client
                    .open_document("/tmp/octocode-busy.ts".into(), "x".repeat(4096))
                    .await
            })
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !client.is_busy() {
            assert!(
                std::time::Instant::now() < deadline,
                "sync never counted as busy"
            );
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(client.is_busy(), "a stalled sync is activity, not idleness");
        syncing.abort();
        let _ = syncing.await;
        assert!(!client.is_busy(), "cancelling the sync releases its lease");
    });
}

/// Minimal stdio LSP server (node). `FAKE_INIT_DELAY_MS` delays the
/// `initialize` reply; `FAKE_ALLOC_MB` allocates (and touches) that much
/// memory after `initialized`; `FAKE_PID_FILE` records the server pid.
#[cfg(unix)]
const CONFIGURABLE_SERVER: &str = r#"#!/usr/bin/env node
const fs = require('fs');
if (process.env.FAKE_PID_FILE) fs.writeFileSync(process.env.FAKE_PID_FILE, String(process.pid));
const hold = [];
let buf = Buffer.alloc(0);
function send(m) {
  const s = JSON.stringify(m);
  process.stdout.write('Content-Length: ' + Buffer.byteLength(s) + '\r\n\r\n' + s);
}
function handle(msg) {
  if (msg.method === 'initialize') {
    const reply = () => send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });
    setTimeout(reply, Number(process.env.FAKE_INIT_DELAY_MS || 0));
  } else if (msg.method === 'initialized') {
    const mb = Number(process.env.FAKE_ALLOC_MB || 0);
    for (let i = 0; i < mb; i++) hold.push(Buffer.alloc(1 << 20, 1));
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
fn node_available() -> bool {
    std::process::Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// A temp dir holding [`CONFIGURABLE_SERVER`] and a client config for it.
#[cfg(unix)]
fn configurable_server(
    tag: &str,
    env: &[(&str, String)],
    max_memory_mb: Option<u32>,
) -> (PathBuf, JsLanguageServerConfig) {
    use std::os::unix::fs::PermissionsExt;
    let root = temp_file(tag);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let script = root.join("fake-lsp.js");
    std::fs::write(&script, CONFIGURABLE_SERVER).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = JsLanguageServerConfig {
        command: script.to_string_lossy().into_owned(),
        args: Some(Vec::new()),
        workspace_root: root.to_string_lossy().into_owned(),
        language_id: Some("plaintext".into()),
        initialization_options: None,
        env: Some(
            env.iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        ),
        max_memory_mb,
    };
    (root, config)
}

#[cfg(unix)]
fn wait_until_gone(pid: i32) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    // SAFETY: signal 0 only checks whether the pid exists.
    while unsafe { libc::kill(pid, 0) } == 0 {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    true
}

/// `stop` overlapping a slow `start` neither deadlocks nor leaves the
/// server `start` publishes running with a live connection: it waits for the
/// start (same lock order) and then tears it down.
#[cfg(unix)]
#[test]
fn stop_overlapping_a_slow_start_waits_then_tears_it_down() {
    if !node_available() {
        return;
    }
    let pid_file = temp_file("octocode-engine-start-stop-pid");
    let (root, config) = configurable_server(
        "octocode-engine-start-stop",
        &[
            ("FAKE_INIT_DELAY_MS", "600".into()),
            ("FAKE_PID_FILE", pid_file.to_string_lossy().into_owned()),
        ],
        None,
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let client = NativeLspClient::new(config);
        let starter = client.clone();
        let start = tokio::spawn(async move { starter.start().await });
        tokio::time::sleep(Duration::from_millis(150)).await;
        timeout(Duration::from_secs(20), client.stop())
            .await
            .expect("stop must not deadlock against an in-flight start")
            .expect("stop");
        timeout(Duration::from_secs(20), start)
            .await
            .expect("start finishes")
            .expect("start task")
            .expect("start succeeds");
        assert!(
            client.connection_handle().await.is_err(),
            "stop must tear down the connection the overlapping start published"
        );
        assert!(client.inner.child.lock().await.is_none(), "child reaped");
        assert!(!client.is_alive().await);
    });
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .expect("server pid")
        .trim()
        .parse()
        .expect("numeric pid");
    assert!(wait_until_gone(pid), "the started server was left running");
    let _ = std::fs::remove_file(pid_file);
    let _ = std::fs::remove_dir_all(root);
}

/// On macOS (no `RLIMIT_AS`) a server whose resident memory passes
/// `maxMemoryMb` is killed by the RSS watchdog and its requests fail with a
/// clear "exceeded memory cap" error.
#[cfg(target_os = "macos")]
#[test]
fn server_over_its_memory_cap_is_killed_with_a_clear_error() {
    if !node_available() {
        return;
    }
    let (root, config) = configurable_server(
        "octocode-engine-memory-cap",
        &[("FAKE_ALLOC_MB", "400".into())],
        Some(128),
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let client = NativeLspClient::new(config);
        client.start().await.expect("fake server starts");
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while client.is_alive().await {
            assert!(
                std::time::Instant::now() < deadline,
                "the watchdog never fired"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let error = client
            .get_hover(root.join("a.ts").to_string_lossy().into_owned(), 0, 0)
            .await
            .expect_err("the connection was failed");
        assert!(error.to_string().contains("exceeded memory cap"), "{error}");
        let _ = client.stop().await;
    });
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn open_documents_remember_the_synced_content() {
    let mut docs = OpenDocuments::new(2);
    let (version, _) = docs.reserve("file:///a");
    assert_eq!(docs.unchanged("file:///a", 7), None, "nothing synced yet");
    docs.record("file:///a", version, 7);
    assert_eq!(
        docs.unchanged("file:///a", 7),
        Some(1),
        "same content: no resync"
    );
    assert_eq!(
        docs.unchanged("file:///a", 8),
        None,
        "edited content resyncs"
    );
    // A new sync forgets the old content until it is recorded.
    let (version, _) = docs.reserve("file:///a");
    assert_eq!(docs.unchanged("file:///a", 7), None);
    docs.record("file:///a", version, 8);
    assert_eq!(docs.unchanged("file:///a", 8), Some(2));
    // A failed sync or an eviction forgets the content too.
    let (version, _) = docs.reserve("file:///a");
    docs.rollback("file:///a", version);
    assert_eq!(docs.unchanged("file:///a", 8), None);
    docs.record("file:///a", 2, 8);
    docs.reserve("file:///b");
    docs.reserve("file:///c");
    assert_eq!(docs.unchanged("file:///a", 8), None, "evicted");
}

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
        let root = temp_file("octocode-engine-response-reuse");
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("fake-lsp.js");
        std::fs::write(&script, COUNTING_SERVER).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let source = root.join("a.ts");
        std::fs::write(&source, "function foo() {}\nfoo();\n".repeat(10)).unwrap();
        let source_path = source.canonicalize().unwrap().to_string_lossy().into_owned();
        let client = NativeLspClient::new(JsLanguageServerConfig {
            command: script.to_string_lossy().into_owned(),
            args: Some(Vec::new()),
            workspace_root: root.canonicalize().unwrap().to_string_lossy().into_owned(),
            language_id: Some("typescript".into()),
            initialization_options: None,
            env: None,
            max_memory_mb: None,
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
        })
        .await;
        assert_ne!(fresh, edited);
        assert_eq!(line(reuse("g1")).await, fresh);
        client.stop().await.unwrap();
        let _ = std::fs::remove_dir_all(root);
    });
}
