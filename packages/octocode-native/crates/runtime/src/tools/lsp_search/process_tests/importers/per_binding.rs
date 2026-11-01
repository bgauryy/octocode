//! A candidate's question is decided once per binding: every other site of
//! a binding the server already resolved is decided without a request.

use super::*;

/// [`IMPORTER_SERVER`] logging each `definition` request's file beside the
/// workspace (outside the candidate scan).
fn logging_server() -> String {
    IMPORTER_SERVER.replace(
        "function handle(msg) {",
        "function handle(msg) {\n  if (msg.method === 'textDocument/definition') require('fs').appendFileSync(root + '/../definitions.log', msg.params.textDocument.uri.split('/').pop() + '\\n');",
    )
}

/// Three importers and ten files that import a same-named symbol of
/// another module, each with an import and a call of the name: one
/// definition request per file decides it (the call shares the import's
/// binding), and the answer is unchanged: every importer's sites, no
/// other file.
#[test]
fn one_request_decides_each_binding_of_a_candidate() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) =
        importer_fixture_serving(&[("f", 3), ("n", 10)], &logging_server());
    let query = serde_json::from_value(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    }))
    .expect("query");
    let row = tokio::runtime::Runtime::new()
        .expect("rt")
        .block_on(crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            &fake_server_pool(),
            &crate::tools::test_support::workspace_policy(&root),
            &execution,
            None,
        ))
        .expect("row");
    assert_ne!(row["status"], "error", "{row}");
    let log = std::fs::read_to_string(root.join("../definitions.log")).unwrap_or_default();
    let mut per_file = std::collections::BTreeMap::<&str, usize>::new();
    for file in log
        .lines()
        .filter(|file| file.starts_with('f') || file.starts_with('n'))
    {
        *per_file.entry(file).or_default() += 1;
    }
    assert_eq!(per_file.len(), 13, "{log}");
    assert!(per_file.values().all(|count| *count == 1), "{per_file:?}");
    let rows = reference_rows(&row);
    for index in 0..3 {
        let name = format!("f{index:03}.ts");
        assert!(rows.iter().any(|(file, _, _)| *file == name), "{row}");
    }
    assert!(
        !rows.iter().any(|(file, _, _)| file.starts_with('n')),
        "{row}"
    );
}
