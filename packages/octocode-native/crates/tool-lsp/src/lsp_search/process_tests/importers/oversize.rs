//! Importer candidates too large to send a language server (a vendored
//! bundle): counted in the row's coverage, never a failed scan.

use super::*;

/// One served references row of `target` over the fixture.
fn served_references(
    root: &Path,
    execution: &crate::lsp_search::LspExecutionConfig,
) -> serde_json::Value {
    let pool = fake_server_pool();
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
    let query = serde_json::from_value(serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    }))
    .expect("query");
    let runtime = tokio::runtime::Runtime::new().expect("rt");
    let row = runtime
        .block_on(crate::lsp_search::execute(
            query,
            &octocode_runtime_base::execution::cancel::NeverCancel,
            &pool,
            &paths,
            execution,
            None,
            crate::lsp_search::test_answers(),
        ))
        .expect("row");
    runtime.block_on(pool.clear_all());
    assert_row_contract(ToolId::LspSearch, &row);
    let published = octocode_runtime_base::test_support::published(ToolId::LspSearch, &row);
    published
        .pointer("/results/0/data")
        .cloned()
        .unwrap_or(published)
}

/// A vendored file that imports and calls the name but is larger than any
/// source a language server is sent: the scan counts it in
/// `coverage.tooLargeFiles` and leads a text search to it; the answer the
/// server verified stays complete.
#[test]
fn an_oversize_importer_candidate_is_counted_not_a_failed_scan() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_of(&[("f", 2)]);
    let vendor = root.join("vendor");
    std::fs::create_dir_all(&vendor).expect("vendor");
    let mut big = String::from("import { target } from '../a';\ntarget();\n");
    let pad = format!("// {}\n", "x".repeat(76));
    let limit = usize::try_from(octocode_engine::lsp::MAX_LSP_SOURCE_BYTES).expect("limit");
    while big.len() <= limit {
        big.push_str(&pad);
    }
    std::fs::write(vendor.join("big.js"), big).expect("big file");
    octocode_engine::stamp::settle_written_so_far();
    let row = served_references(&root, &execution);
    let text = row.to_string();
    assert!(
        row.get("partialReasons").is_none() && row.get("isPartial").is_none(),
        "the verified answer stays complete: {row}"
    );
    assert!(!text.contains("importerScanFailed"), "{row}");
    assert!(!text.contains("could not be read"), "{row}");
    assert_eq!(row["coverage"]["tooLargeFiles"], 1, "{row}");
    let leads = paths_in(&row["related"]);
    assert!(
        leads.iter().any(|path| path.ends_with("vendor/big.js")),
        "a text search reads the file the server was not sent: {row}"
    );
    let mut files = reference_rows(&row)
        .into_iter()
        .map(|(file, _, _)| file)
        .collect::<Vec<_>>();
    files.dedup();
    assert!(files.contains(&"f000.ts".to_owned()), "{row}");
    assert!(!files.contains(&"big.js".to_owned()), "{row}");
}
