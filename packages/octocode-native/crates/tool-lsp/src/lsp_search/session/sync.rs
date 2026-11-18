//! Syncing the anchor document into the leased server.
use super::*;

/// The first didOpen of a document can trigger a project load (tsserver
/// starts one only then); wait for it so queries do not race it. An
/// outline (`documentSymbols`) is the file's own syntax, answered without
/// the project (typescript-language-server routes it to its syntax server),
/// so it syncs without that wait and reports no open readiness. The document
/// still owes the wait: an empty outline of a file with text pays it and asks
/// once more (`NativeLspClient::get_document_symbols_settled`).
pub(in crate::lsp_search) async fn sync_document(
    query: &LspSearchQuery,
    target: &Target,
    document: Option<&source::Source>,
    sources: &mut SourceCache<'_>,
    client: &octocode_engine::lsp::client::NativeLspClient,
    cancel: &dyn CancellationCheck,
) -> Result<Option<Readiness>, Exit> {
    if let Some(document) = document {
        let path = target.path.clone();
        // A building index is no load to wait for: the row says it is partial.
        let synced = if lsp_search_query::op(query) == Op::DocumentSymbols || client.is_indexing() {
            cancellable(cancel, client.open_document(path, &document.content))
                .await?
                .map(|()| None)
        } else {
            reporting(
                "Indexing with",
                &target.config,
                cancellable(
                    cancel,
                    client.open_document_and_wait(path, &document.content, None, None),
                ),
            )
            .await?
        };
        return match synced {
            Ok(readiness) => Ok(readiness),
            Err(error) => Err(target.fail(
                query,
                ErrorCode::DocumentSyncFailed,
                &format!("The source document could not be synchronized: {error}"),
                true,
            )),
        };
    }
    // Some servers (tsserver: "No Project") cannot answer workspace-wide
    // queries until a document of the project is open. Best-effort: a failed
    // sync just leaves the server to answer (or error) as before.
    if target.root_only
        && let Some(representative) =
            directory_representative(&target.path, &target.config.workspace_root)
        && let Some(source) = sources.get(&representative).await
        && let Ok(readiness) = reporting(
            "Indexing with",
            &target.config,
            cancellable(
                cancel,
                client.open_document_and_wait(representative, &source.content, None, None),
            ),
        )
        .await?
    {
        return Ok(readiness);
    }
    Ok(None)
}
