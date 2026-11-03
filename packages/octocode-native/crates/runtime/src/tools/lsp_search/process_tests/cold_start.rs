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
