//! Receipts from a real child server: a capability the server does not
//! advertise lists the operations it does, and the opt-in trace names the
//! server's state without a path.

use super::*;

/// The fixture advertises only `hoverProvider`: `callers` fails as
/// `capabilityUnavailable`, and the row lists the operations to retry with.
#[test]
fn a_missing_capability_lists_the_operations_the_server_advertises() {
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let config = root.join("lsp-servers.json");
    std::fs::write(
        &config,
        serde_json::json!({"languageServers": {".ts": {
            "command": script.to_string_lossy(), "args": [], "languageId": "plaintext"
        }}})
        .to_string(),
    )
    .expect("config");
    let execution = crate::tools::lsp_search::LspExecutionConfig {
        octocode_home: Some(root.to_path_buf()),
        ..crate::tools::lsp_search::LspExecutionConfig::default()
    };
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "callers", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "a", "line": 1
    }))
    .expect("query");
    let pool = fake_server_pool();
    let paths = crate::tools::test_support::workspace_policy(&root);
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let row = runtime
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            None,
            crate::tools::lsp_search::test_answers(),
        ))
        .expect("capability row");
    assert_row_contract(ToolId::LspSearch, &row);
    assert_eq!(row["errorCode"], "capabilityUnavailable", "{row}");
    assert_eq!(
        row["lsp"],
        serde_json::json!({"serverAvailable": true, "receipt": {
            "command": script.to_string_lossy(), "source": "user",
            "operations": ["hover", "diagnostic"]
        }}),
        "{row}"
    );
    runtime.block_on(pool.clear_all());
}
