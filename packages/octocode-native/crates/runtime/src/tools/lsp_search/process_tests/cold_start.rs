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
    std::fs::write(&script, server).expect("slow server");
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

/// A call that waits on a cold server's start says what it waited on and
/// that later calls reuse the running server; the next call, on the same
/// pool, starts nothing (one spawn in all) and has nothing to disclose.
#[test]
fn a_cold_start_is_disclosed_and_the_next_call_reuses_the_server() {
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
    std::fs::write(&script, server).expect("slow server");
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
    let waited = |row: &serde_json::Value| {
        row["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .filter(|note| note.starts_with("This call waited "))
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
        notes
            .iter()
            .any(|note| note.contains("language server to start")
                && note.contains("later calls reuse the running server")),
        "the cold start is disclosed: {notes:?} / {first}"
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
