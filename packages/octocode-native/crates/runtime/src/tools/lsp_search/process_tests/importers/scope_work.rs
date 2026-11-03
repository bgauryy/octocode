//! A request walks its scope for what it needs, once: a fresh answer that
//! issues no continuation and watches no files walks it for the text scan
//! alone; a page that does issue one, or re-proves one, fingerprints it
//! once; and the text scan resolves its root once, not each matched file.

use super::*;
use crate::tools::lsp_search::scope::probe::{Work, measure};
use std::sync::Arc;

/// Run one `references` page under a work probe.
async fn counted(
    pool: &LspClientPool,
    root: &Path,
    execution: &crate::tools::lsp_search::LspExecutionConfig,
    row: serde_json::Value,
    budget: Option<usize>,
) -> (serde_json::Value, Arc<Work>) {
    let paths = crate::tools::test_support::workspace_policy(root);
    let work = Arc::new(Work::default());
    let query = serde_json::from_value(row).expect("query");
    let result = measure(
        &work,
        crate::tools::lsp_search::execute(
            query,
            &crate::execution::cancel::NeverCancel,
            pool,
            &paths,
            execution,
            budget,
            crate::tools::lsp_search::test_answers(),
        ),
    )
    .await
    .expect("page");
    assert_ne!(result["status"], "error", "{result}");
    (result, work)
}

fn references(root: &Path) -> serde_json::Value {
    serde_json::json!({
        "operation": "references", "path": root.join("a.ts").to_string_lossy(),
        "symbolName": "target", "line": 1, "workspaceRoot": root.to_string_lossy()
    })
}

/// A one-page fresh answer proves no revision for anyone: it walks the scope
/// for no fingerprint, and resolves the text scan's root once whatever
/// number of files matched.
#[test]
fn a_one_page_fresh_answer_walks_no_fingerprint() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture_of(&[("f", 3)]);
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = fake_server_pool();
        let (row, work) = counted(&pool, &root, &execution, references(&root), None).await;
        assert!(row.pointer("/pagination/next").is_none(), "{row}");
        assert_eq!(Work::get(&work.walks), 0, "no continuation, no watcher");
        assert_eq!(Work::get(&work.text_scans), 1, "one text scan");
        assert_eq!(Work::get(&work.canonicalized), 1, "the scan's root alone");
    });
}

/// The page that issues a continuation fingerprints the scope once, and the
/// continuation page, which re-proves it, once.
#[test]
fn each_page_that_needs_the_fingerprint_walks_once() {
    if !require_node() {
        return;
    }
    let (_dir, root, execution) = importer_fixture();
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = fake_server_pool();
        let (first, work) = counted(&pool, &root, &execution, references(&root), Some(2000)).await;
        assert_eq!(Work::get(&work.walks), 1, "the issuing page");
        assert_eq!(Work::get(&work.canonicalized), 1, "the scan's root alone");
        assert!(Work::get(&work.stamped) > IMPORTERS);
        let next = next_row(&first, "nextPage")
            .or_else(|| {
                first
                    .pointer("/pagination/next/nextPage/input/queries/0")
                    .cloned()
            })
            .expect("a continuation");
        let (_, work) = counted(&pool, &root, &execution, next, Some(2000)).await;
        assert_eq!(Work::get(&work.walks), 1, "the re-proving page");
    });
}

/// A scope file written while page 1 runs may be read by it before the
/// write: the sealed revision cannot prove page 1's rows describe the
/// source it pins, so the next page refuses with `staleSnapshot` (the typed
/// refusal of a changed scope, with its restart lead) instead of
/// continuing over inconsistent evidence.
#[test]
fn a_file_written_while_page_one_runs_stales_its_continuation() {
    if !require_node() {
        return;
    }
    let editing = IMPORTER_SERVER.replace(
        "case 'textDocument/references': {",
        "case 'textDocument/references': {\n      require('fs').appendFileSync(root + '/f001.ts', '// edited\\n');",
    );
    assert_ne!(editing, IMPORTER_SERVER, "the hook is in the server");
    let (_dir, root, execution) = importer_fixture_serving(&[("f", IMPORTERS)], &editing);
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let pool = fake_server_pool();
        let (first, _) = counted(&pool, &root, &execution, references(&root), Some(2000)).await;
        let next = first
            .pointer("/pagination/next/nextPage/input/queries/0")
            .cloned()
            .expect("a continuation");
        let paths = crate::tools::test_support::workspace_policy(&root);
        let page = crate::tools::lsp_search::execute(
            serde_json::from_value(next).expect("query"),
            &crate::execution::cancel::NeverCancel,
            &pool,
            &paths,
            &execution,
            Some(2000),
            crate::tools::lsp_search::test_answers(),
        )
        .await
        .expect("page");
        assert_eq!(page["errorCode"], "staleSnapshot", "{page}");
        let restart = &crate::tools::test_support::checks::related_call(&page, "lspSearch", |_| {
            true
        })["input"]["queries"][0];
        for cursor in crate::contracts::state_fields(crate::id::ToolId::LspSearch) {
            assert!(restart.get(cursor).is_none(), "{cursor}: {restart}");
        }
    });
}
