//! Syncing the anchor document into the leased server.
use super::*;

/// The first didOpen of a document can trigger a project load (tsserver
/// starts one only then); wait for it so queries do not race it. An
/// outline (`documentSymbols`) is the file's own syntax, answered without
/// the project (typescript-language-server routes it to its syntax server),
/// so it syncs without that wait and reports no open readiness.
pub(in crate::tools::lsp_search) async fn sync_document(
    query: &LspSearchQuery,
    target: &Target,
    document: Option<&source::Source>,
    sources: &mut SourceCache<'_>,
    client: &octocode_engine::lsp::client::NativeLspClient,
    cancel: &dyn CancellationCheck,
) -> Result<Option<String>, Exit> {
    if let Some(document) = document {
        let path = target.path.clone();
        let synced = if lsp_search_query::op(query) == Op::DocumentSymbols {
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
            root_representative(&target.path, &target.config.workspace_root)
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

/// The source file a `workspaceRoot`-only query opens so the server loads a
/// project: the directory's own representative, else (a source directory
/// such as `src` has no project marker) one in the workspace root's
/// language, inside the directory when it has one.
pub(in crate::tools::lsp_search) fn root_representative(
    directory: &str,
    workspace_root: &str,
) -> Option<String> {
    workspace_root_representative_source(directory).or_else(|| {
        let extension = workspace_root_languages(workspace_root)
            .into_iter()
            .next()?;
        representative_source_for(directory, extension)
            .or_else(|| representative_source_for(workspace_root, extension))
    })
}
