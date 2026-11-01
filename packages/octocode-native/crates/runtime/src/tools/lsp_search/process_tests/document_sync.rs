//! Anchor document sync: which operations wait for the project load a
//! first didOpen can start.

use super::*;

/// An outline is the file's syntax: its first sync sends the didOpen and
/// returns without the post-open wait (no readiness to report), while a
/// semantic operation (`diagnostic`) still waits on its first open.
#[test]
fn document_symbols_sync_without_the_open_wait_and_diagnostics_keep_it() {
    use crate::execution::cancel::NeverCancel;
    use crate::tools::lsp_search::page::Target;
    use crate::tools::lsp_search::session::sync_document;
    use crate::tools::lsp_search::source::SourceCache;
    use serde_json::json;
    if !require_node() {
        return;
    }
    let (_dir, root, script) = fixture();
    let paths = crate::tools::test_support::workspace_policy(&root);
    tokio::runtime::Runtime::new().expect("rt").block_on(async {
        let client = NativeLspClient::new(config(&root, &script, None));
        client.start().await.expect("server starts");
        for (operation, file, waits) in [("documentSymbols", "a.ts", false), ("diagnostic", "b.ts", true)] {
            let path = root.join(file);
            std::fs::write(&path, "const a = 1;\n").expect("source");
            let path = path.to_string_lossy().into_owned();
            let query: LspSearchQuery =
                serde_json::from_value(json!({"operation": operation, "path": path}))
                    .expect("query");
            let target = Target {
                uri: octocode_engine::lsp::uri::path_to_uri(&path).expect("uri"),
                path: path.clone(),
                root_only: false,
                config: config(&root, &script, None),
                fallback: None,
                root_note: None,
            };
            let mut sources = SourceCache::new(&paths);
            let document = sources.insert(&path, "const a = 1;\n".to_owned());
            let readiness = sync_document(
                &query,
                &target,
                Some(&document),
                &mut sources,
                &client,
                &NeverCancel,
            )
            .await
            .unwrap_or_else(|_| panic!("{operation}: the document syncs"));
            assert_eq!(
                readiness.is_some(),
                waits,
                "{operation}: a first open reports readiness only after the open wait ({readiness:?})"
            );
        }
        client.stop().await.expect("fixture server closed");
    });
}
