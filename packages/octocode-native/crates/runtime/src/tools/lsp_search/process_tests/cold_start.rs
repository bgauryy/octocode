//! A cold language server says it is starting: a call that waits on server
//! startup or indexing reports progress through the call's notice scope, so
//! an MCP client that sent a progress token is not left silent.

use super::lifecycle::execution_config;
use super::*;
use octocode_io::budget::REPORTED_FROM;

/// The server answers `initialize` only after a wait longer than the
/// shortest reported wait; the call's progress listener hears it, and the
/// answer is unchanged.
#[test]
fn a_slow_server_start_reports_progress_to_the_call() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let delay = (REPORTED_FROM * 2).as_millis();
    let server = FAKE_SERVER.replace(
        "    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });",
        &format!("    setTimeout(() => send({{ jsonrpc: '2.0', id: msg.id, result: {{ capabilities: {{ hoverProvider: true, textDocumentSync: 1 }} }} }}), {delay});"),
    );
    assert_ne!(server, FAKE_SERVER, "the slow start is injected");
    crate::tools::lsp_search::tests::write_executable(&script, &server);
    let execution = execution_config(&root, &script, "shellscript");
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "path": root.join("a.ts").to_string_lossy()
    }))
    .expect("query");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = std::sync::Arc::clone(&heard);
    let scope = octocode_io::notice::Scope::with_progress(move |line| {
        sink.lock().expect("lines").push(line.to_owned());
    });
    let pool = LspClientPool::new(LspPoolOptions::default());
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let row = runtime
        .block_on(scope.run(Box::pin(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::tools::lsp_search::test_answers(),
        ))))
        .expect("row");
    let lines = heard.lock().expect("lines").clone();
    assert!(
        lines.iter().any(|line| line.contains("language server")),
        "a start slower than {REPORTED_FROM:?} reports progress: {lines:?} / {row}"
    );
    runtime.block_on(pool.clear_all());
}

/// A call that waits on a cold server's start carries no warning for it (the
/// wait is progress, not evidence); the next call, on the same pool, starts
/// nothing (one spawn in all).
#[test]
fn a_cold_start_adds_no_warning_and_the_next_call_reuses_the_server() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let delay = (REPORTED_FROM * 2).as_millis();
    let server = FAKE_SERVER
        .replace(
            "const pids = process.env.FAKE_LSP_PIDS;",
            "const pids = __dirname + '/pids.txt';",
        )
        .replace(
            "    send({ jsonrpc: '2.0', id: msg.id, result: { capabilities: { hoverProvider: true, textDocumentSync: 1 } } });",
            &format!("    setTimeout(() => send({{ jsonrpc: '2.0', id: msg.id, result: {{ capabilities: {{ hoverProvider: true, textDocumentSync: 1 }} }} }}), {delay});"),
        );
    crate::tools::lsp_search::tests::write_executable(&script, &server);
    let execution = execution_config(&root, &script, "shellscript");
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "path": root.join("a.ts").to_string_lossy()
    }))
    .expect("query");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let pool = LspClientPool::new(LspPoolOptions::default());
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let call = || {
        runtime
            .block_on(crate::tools::lsp_search::execute(
                query.clone(),
                &crate::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
                crate::tools::lsp_search::test_answers(),
            ))
            .expect("row")
    };
    // The cold-call sentence the answer once carried in `warnings`.
    let waited = |row: &serde_json::Value| {
        crate::tools::test_support::staged_response(crate::id::ToolId::LspSearch, vec![row.clone()])
            ["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter(|note| note.starts_with("This cold call took "))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let spawns = || {
        std::fs::read_to_string(root.join("pids.txt"))
            .unwrap_or_default()
            .lines()
            .count()
            / 2
    };
    let first = call();
    let notes = waited(&first);
    assert!(
        notes.is_empty(),
        "a slow start is not a warning of the answer: {notes:?} / {first}"
    );
    assert_eq!(spawns(), 1);
    let second = call();
    assert_eq!(spawns(), 1, "the second call reuses the server");
    assert!(
        waited(&second).is_empty(),
        "a warm call waited on nothing: {second}"
    );
    runtime.block_on(pool.clear_all());
}

/// A pool holding more resident memory than its budget says so on the call
/// that acquired from it; a pool within its budget says nothing.
#[test]
fn a_pool_over_its_memory_budget_is_disclosed_and_one_within_it_is_not() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let server = FAKE_SERVER.replace(
        "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
        "  } else if (msg.method === 'textDocument/hover') {\n    send({ jsonrpc: '2.0', id: msg.id, result: { contents: 'ready' } });",
    );
    assert_ne!(server, FAKE_SERVER, "the hover answer is injected");
    crate::tools::lsp_search::tests::write_executable(&script, &server);
    let execution = execution_config(&root, &script, "shellscript");
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "hover", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "a", "line": 1
    }))
    .expect("query");
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let overflow_warnings = |pool: &LspClientPool| {
        let row = runtime
            .block_on(crate::tools::lsp_search::execute(
                query.clone(),
                &crate::execution::cancel::NeverCancel,
                pool,
                &paths,
                &execution,
                None,
                crate::tools::lsp_search::test_answers(),
            ))
            .expect("row");
        row["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter(|note| note.contains("language server pool holds"))
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    // The smallest budget: the one server the call leased cannot fit.
    let tight = LspClientPool::new(LspPoolOptions {
        max_resident_bytes: 1,
        ..LspPoolOptions::default()
    });
    // The server's resident memory is sampled by a watchdog some seconds
    // after the start, so warm calls on the same pool wait for the sample.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut warnings = overflow_warnings(&tight);
    while warnings.is_empty() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(250));
        warnings = overflow_warnings(&tight);
    }
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("never stopped"), "{warnings:?}");
    runtime.block_on(tight.clear_all());
    // The default budget is half the host's memory.
    let roomy = LspClientPool::new(LspPoolOptions::default());
    assert!(overflow_warnings(&roomy).is_empty());
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(overflow_warnings(&roomy).is_empty());
    runtime.block_on(roomy.clear_all());
}
