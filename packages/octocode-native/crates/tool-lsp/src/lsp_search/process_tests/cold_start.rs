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
    crate::lsp_search::tests::write_executable(&script, &server);
    let execution = execution_config(&root, &script, "shellscript");
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "path": root.join("a.ts").to_string_lossy()
    }))
    .expect("query");
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let heard = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = std::sync::Arc::clone(&heard);
    let scope = octocode_io::notice::Scope::with_progress(move |line| {
        sink.lock().expect("lines").push(line.to_owned());
    });
    let pool = LspClientPool::new(LspPoolOptions::default());
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let row = runtime
        .block_on(scope.run(Box::pin(crate::lsp_search::execute(
            query,
            &octocode_runtime_base::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::lsp_search::test_answers(),
        ))))
        .expect("row");
    let lines = heard.lock().expect("lines").clone();
    use crate::lsp_search::session::{KEPT, STARTING};
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with(STARTING) && line.ends_with(KEPT)),
        "a start slower than {REPORTED_FROM:?} reports that it started a server later calls reuse: {lines:?} / {row}"
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
    crate::lsp_search::tests::write_executable(&script, &server);
    let execution = execution_config(&root, &script, "shellscript");
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "path": root.join("a.ts").to_string_lossy()
    }))
    .expect("query");
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let pool = LspClientPool::new(LspPoolOptions::default());
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let call = || {
        runtime
            .block_on(crate::lsp_search::execute(
                query.clone(),
                &octocode_runtime_base::execution::cancel::NeverCancel,
                &pool,
                &paths,
                &execution,
                None,
                crate::lsp_search::test_answers(),
            ))
            .expect("row")
    };
    // The cold-call sentence the answer once carried in `warnings`.
    let waited = |row: &serde_json::Value| {
        octocode_runtime_base::test_support::staged_response(
            octocode_runtime_base::id::ToolId::LspSearch,
            vec![row.clone()],
        )["warnings"]
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

/// The overrun warning is for a busy server the pool will stop as its request
/// ends: the one server a call leased, alone over even the smallest budget, is
/// kept by design and reported by neither a tight nor a roomy pool.
#[test]
fn a_lone_leased_server_over_its_memory_budget_is_not_reported_as_an_overrun() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let server = FAKE_SERVER.replace(
        "  } else if (msg.method === 'textDocument/hover') {\n    process.exit(3);",
        "  } else if (msg.method === 'textDocument/hover') {\n    send({ jsonrpc: '2.0', id: msg.id, result: { contents: 'ready' } });",
    );
    assert_ne!(server, FAKE_SERVER, "the hover answer is injected");
    crate::lsp_search::tests::write_executable(&script, &server);
    let execution = execution_config(&root, &script, "shellscript");
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "hover", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "a", "line": 1
    }))
    .expect("query");
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let overflow_warnings = |pool: &LspClientPool| {
        let row = runtime
            .block_on(crate::lsp_search::execute(
                query.clone(),
                &octocode_runtime_base::execution::cancel::NeverCancel,
                pool,
                &paths,
                &execution,
                None,
                crate::lsp_search::test_answers(),
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
    // The smallest budget, which the one leased server cannot fit, and the
    // default (half the host's memory).
    for max_resident_bytes in [1, LspPoolOptions::default().max_resident_bytes] {
        let pool = LspClientPool::new(LspPoolOptions {
            max_resident_bytes,
            ..LspPoolOptions::default()
        });
        assert!(overflow_warnings(&pool).is_empty());
        // The watchdog samples the server's memory after its start: a warm
        // call sees the sample.
        std::thread::sleep(std::time::Duration::from_secs(3));
        assert!(overflow_warnings(&pool).is_empty());
        runtime.block_on(pool.clear_all());
    }
}
