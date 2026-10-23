//! Server lifecycle against a real child process: startup indexing, a crash
//! mid-request, acquire/stop cleanup, prewarm, memory policy, tsgo→tsserver fallback.

use super::*;

fn execution_config(
    root: &Path,
    script: &Path,
    language_id: &str,
) -> crate::tools::lsp_search::LspExecutionConfig {
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": language_id
        }}})
        .to_string(),
    )
    .expect("config");
    crate::tools::lsp_search::LspExecutionConfig {
        config_path: Some(config.to_string_lossy().into_owned()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    }
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Announced startup indexing is a live, incomplete answer with an exact
/// retry. The retry reuses the same server after its progress ends; a request
/// during the first response would crash this fixture instead.
#[test]
fn initial_indexing_returns_partial_then_retries_on_the_same_server() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let server = FAKE_SERVER
        .replace(
            "const pids = process.env.FAKE_LSP_PIDS;",
            "const pids = __dirname + '/pids.txt';\nconst gate = __dirname + '/indexing-ready';",
        )
        .replace(
            "let buf = Buffer.alloc(0);",
            "let indexingDone = false;\nlet buf = Buffer.alloc(0);",
        )
        .replace(
            "    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });",
            "    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });\n    send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'index', value: { kind: 'begin', title: 'indexing' } } });\n    const waitForGate = setInterval(() => {\n      if (!fs.existsSync(gate)) return;\n      clearInterval(waitForGate);\n      indexingDone = true;\n      send({ jsonrpc: '2.0', method: '$/progress', params: { token: 'index', value: { kind: 'end', message: 'ready' } } });\n    }, 10);",
        )
        .replace(
            "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
            "  } else if (msg.method === 'textDocument/hover') {\n    if (!indexingDone) process.exit(3);\n    fs.appendFileSync(pids, 'hover\\n');\n    send({ jsonrpc: '2.0', id: msg.id, result: { contents: 'ready' } });",
        );
    assert_ne!(server, FAKE_SERVER, "the indexing fixture is injected");
    std::fs::write(&script, server).expect("indexing server");

    let execution = execution_config(&root, &script, "shellscript");
    // The startup settle is the window in which the client reads the
    // server's `begin` progress; a zero cap races the hover past it.
    let pool = LspClientPool::new(LspPoolOptions {
        client: octocode_engine::lsp::client::LspClientOptions {
            settle_cap: Some(std::time::Duration::from_secs(1)),
        },
        ..LspPoolOptions::default()
    });
    let query_row = serde_json::json!({
        "operation": "hover", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "a", "line": 1
    });
    let query: crate::tools::lsp_search::LspSearchQuery =
        serde_json::from_value(query_row.clone()).expect("query");
    let expected_retry = lsp_search_query::to_row(&query);
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let first = runtime
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
        ))
        .expect("indexing row");
    assert_row_contract(ToolId::LspSearch, &query_row, &first);
    assert_eq!(first["status"], "empty", "{first}");
    assert_eq!(first["category"], "languageServerIndexing", "{first}");
    assert!(
        first["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("still indexing")),
        "{first}"
    );
    assert_eq!(first["lsp"]["serverAvailable"], true, "{first}");
    assert_eq!(first["isPartial"], true, "{first}");
    let retry = first["next"]["retry"]["query"]["queries"][0].clone();
    assert_eq!(retry, expected_retry, "{first}");
    let pids = std::fs::read_to_string(root.join("pids.txt")).expect("server pid");
    assert_eq!(
        pids.lines().count(),
        2,
        "no semantic request before retry: {pids}"
    );
    std::fs::write(root.join("indexing-ready"), "").expect("indexing gate");

    let retried = serde_json::from_value(retry).expect("retry query");
    let second = runtime
        .block_on(crate::tools::lsp_search::execute(
            retried,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
        ))
        .expect("retry row");
    assert_row_contract(ToolId::LspSearch, &query_row, &second);
    assert_eq!(second["kind"], "hover", "{second}");
    assert_eq!(second["hover"]["contents"], "ready", "{second}");
    let pids = std::fs::read_to_string(root.join("pids.txt")).expect("server pid");
    assert_eq!(
        pids.lines().count(),
        3,
        "one server and one retry request: {pids}"
    );
    runtime.block_on(pool.clear_all());
}

/// A server that dies mid-request fails the pending request fast with a
/// typed `ConnectionClosed` (`lsp.serverCrashed`), not a timeout.
#[test]
fn server_crash_mid_request_fails_pending_fast_as_server_crashed() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("fake server starts");
        let started = Instant::now();
        let error = tokio::time::timeout(
            Duration::from_secs(10),
            client.get_hover(root.join("a.ts").to_string_lossy().into_owned(), 0, 6),
        )
        .await
        .expect("pending request settles well before the request timeout")
        .expect_err("server crashed mid-request");
        assert!(
            matches!(error.kind(), ErrorKind::ConnectionClosed),
            "{error:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(
            LspFailure::from_engine(&error).code,
            ErrorCode::ServerCrashed
        );
        let _ = client.stop().await;
    });
}

/// Repeated acquire/stop through the pool leaves no server or grandchild
/// process running.
#[test]
fn acquire_stop_loop_leaves_no_child_processes() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let pids = root.join("pids.txt");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = LspClientPool::new(LspPoolOptions {
            idle_timeout_ms: 60_000,
            max_entries: 4,
            max_resident_bytes: 0,
            client: FAKE_SERVER_CLIENT,
        });
        let config = config(&root, &script, Some(&pids));
        for _ in 0..5 {
            let (client, lease) = pool
                .acquire_leased(config.clone())
                .await
                .expect("acquire")
                .expect("client");
            assert!(client.is_alive().await);
            drop(lease);
            assert!(pool.clear(&config).await.expect("clear"));
        }
        assert!(pool.is_empty());
    });
    let recorded = std::fs::read_to_string(&pids).expect("pid log");
    let recorded = recorded.lines().collect::<Vec<_>>();
    assert_eq!(recorded.len(), 10, "5 servers × (server + grandchild)");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let survivors = recorded.iter().filter(|pid| alive(pid)).collect::<Vec<_>>();
        if survivors.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "leaked processes: {survivors:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A prewarm whose file is over the open cap (or unreadable) warmed
/// only the server, so it reports not-warm and frees its key; a small file
/// is opened and the warm completes.
#[test]
fn prewarm_frees_its_key_when_the_file_is_not_opened() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let large = root.join("large.ts");
    std::fs::write(&large, "x".repeat(2 * 1024 * 1024 + 1)).expect("large source");
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = fake_server_pool();
        let config = config(&root, &script, None);
        let file = |path: PathBuf| path.to_string_lossy().into_owned();
        assert!(
            !crate::tools::lsp_search::prewarm::warm(&pool, config.clone(), file(large.clone()))
                .await,
            "an over-cap file is not a completed warm"
        );
        assert!(
            !crate::tools::lsp_search::prewarm::warm(
                &pool,
                config.clone(),
                file(root.join("missing.ts"))
            )
            .await,
            "an unreadable file is not a completed warm"
        );
        assert!(
            crate::tools::lsp_search::prewarm::warm(&pool, config.clone(), file(root.join("a.ts")))
                .await
        );
        assert_eq!(pool.len(), 1, "every warm reused one pooled server");
        pool.clear_all().await;
    });
}

/// A V8 (Node) server reserves address space far past its resident use; it
/// must start and answer under the default Linux memory policy (RSS watchdog,
/// no address-space cap).
#[cfg(target_os = "linux")]
#[test]
fn node_server_starts_under_the_default_address_space_cap() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client
            .start()
            .await
            .expect("node initializes under the default memory policy");
        assert!(client.is_alive().await);
        let _ = client.stop().await;
    });
}

/// tsserver answers when octocode's tsgo cannot start, and the answer
/// says so; a target without a fallback still fails as `serverUnavailable`.
#[test]
fn a_tsgo_that_fails_to_start_falls_back_to_tsserver_and_says_so() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::{Exit, Target, acquire_leased_client};
    use serde_json::json;
    assert!(
        node_available(),
        "Node is required for the real LSP fixture"
    );
    let (_dir, root, script) = fixture();
    let missing = root.join("no-such-tsgo");
    let path = root.join("a.ts").to_string_lossy().into_owned();
    let uri = octocode_engine::lsp::uri::path_to_uri(&path).expect("uri");
    let query: crate::tools::lsp_search::LspSearchQuery = serde_json::from_value(json!({
        "operation":"hover", "mainGoal":"test", "reasoning":"test",
        "path":uri, "symbolName":"a", "line":1
    }))
    .expect("query");
    let target = |fallback| Target {
        path: path.clone(),
        uri: uri.clone(),
        root_only: false,
        config: config(&root, &missing, None),
        fallback,
        root_note: None,
    };
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = LspClientPool::new(LspPoolOptions {
            idle_timeout_ms: 60_000,
            max_entries: 4,
            max_resident_bytes: 0,
            client: FAKE_SERVER_CLIENT,
        });
        let fallback = config(&root, &script, None);
        let leased =
            acquire_leased_client(&query, &target(Some(fallback.clone())), &pool, &NeverCancel)
                .await
                .map_err(|_| "the fallback must answer")
                .expect("leased");
        assert_eq!(leased.config.command, fallback.command);
        let reason = leased
            .fell_back_from
            .as_deref()
            .expect("fallback disclosed");
        assert!(reason.contains("no-such-tsgo"), "{reason}");
        assert!(leased.client.is_alive().await);

        // The failed key sits in its start backoff: the next request falls
        // back at once instead of retrying the broken server.
        let again = acquire_leased_client(&query, &target(Some(fallback)), &pool, &NeverCancel)
            .await
            .map_err(|_| "the fallback must answer")
            .expect("leased again");
        assert!(again.fell_back_from.is_some());

        match acquire_leased_client(&query, &target(None), &pool, &NeverCancel).await {
            Err(Exit::Row(row)) => {
                assert_eq!(row["errorCode"], "serverUnavailable", "{row}");
            }
            _ => panic!("no fallback means a serverUnavailable row"),
        }
        drop(leased);
        drop(again);
        pool.clear_all().await;
    });
}
