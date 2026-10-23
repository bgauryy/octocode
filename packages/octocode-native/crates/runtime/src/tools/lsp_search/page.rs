//! One page of an operation: the target it runs on, pinned and unpinned
//! pages, and the per-target run.
use super::*;

/// The request as the server sees it: the canonical file (or root), its
/// URI, and the discovered server configuration.
pub(super) struct Target {
    pub(super) path: String,
    pub(super) uri: String,
    pub(super) root_only: bool,
    pub(super) config: octocode_engine::lsp::types::LspLanguageServerConfig,
    /// The server to use when `config` (octocode's native tsgo) fails to
    /// start; `None` where there is no substitute.
    pub(super) fallback: Option<octocode_engine::lsp::types::LspLanguageServerConfig>,
    /// Why the server runs at another root than the one inferred for the
    /// file: a warning every row of the request carries.
    pub(super) root_note: Option<String>,
}

impl Target {
    pub(super) fn fail(
        &self,
        query: &LspSearchQuery,
        code: ErrorCode,
        message: &str,
        available: bool,
    ) -> Exit {
        Exit::Row(failure(query, &self.uri, code, message, available))
    }

    /// Whether the server (or its fallback) would launch, by resolution
    /// only: a row that fails before any server starts still says whether
    /// one could.
    pub(super) fn server_resolves(&self) -> bool {
        use octocode_engine::lsp::config::server_command_resolves;
        server_command_resolves(&self.config)
            || self.fallback.as_ref().is_some_and(server_command_resolves)
    }
}

pub(super) async fn execute_page(
    mut query: LspSearchQuery,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
    response_window: Option<usize>,
) -> Result<Value, LspFailure> {
    cancel.check().map_err(LspFailure::cancelled)?;
    if unpinned_later_page(&query) {
        return Ok(unpinned_page_row(&query));
    }
    let original_root = lsp_search_query::workspace_root(&query).map(str::to_owned);
    let path = match resolve_paths(&mut query, paths) {
        Ok(path) => path,
        Err(failure) => return failure::path_failure(&query, paths, failure),
    };
    match run_page(
        &query,
        path,
        cancel,
        pool,
        paths,
        execution_config,
        response_window,
    )
    .await
    {
        Ok(mut row) | Err(Exit::Row(mut row)) => {
            restore_root_spelling(
                &mut row,
                lsp_search_query::workspace_root(&query),
                original_root.as_deref(),
            );
            failure::stamp_coverage(&mut row);
            Ok(row)
        }
        Err(Exit::Failed(failure)) if failure.lexical_fallback => {
            let mut row = failure.row(&query, lsp_search_query::path(&query).unwrap_or_default());
            failure::stamp_coverage(&mut row);
            Ok(row)
        }
        Err(Exit::Failed(failure)) => Err(failure),
    }
}

/// A later page that names neither the snapshot of the page before it nor
/// a `pageSize`: its boundaries would follow the response window shared by
/// the batch, so the same `page` could address different rows.
/// The refusal of a later page with no snapshot and no `pageSize`, led
/// back to the walk's first page (`next.restart`), whose `next.nextPage`
/// carries the snapshot.
pub(super) fn unpinned_page_row(query: &LspSearchQuery) -> Value {
    json!({
        "status": "error",
        "errorCode": ErrorCode::InvalidInput,
        "error": "page > 1 needs the snapshot its next page carries, or an explicit pageSize: without either, pages are cut from this call's response window, which depends on the other rows of the batch.",
        "next": {
            "restart": crate::response::result::restart_query(
                crate::id::ToolId::LspSearch,
                &lsp_search_query::to_row(query),
            )
        }
    })
}

pub(super) fn unpinned_later_page(query: &LspSearchQuery) -> bool {
    lsp_search_query::page(query).is_some_and(|page| page > 1)
        && lsp_search_query::snapshot(query).is_none()
        && lsp_search_query::page_size(query).is_none()
}

pub(super) async fn run_page(
    query: &LspSearchQuery,
    path: String,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    execution_config: &LspExecutionConfig,
    response_window: Option<usize>,
) -> Result<Value, Exit> {
    let target = discover_server(query, path, paths, execution_config)?;
    let outcome = run_target(query, &target, cancel, pool, paths, response_window).await;
    let Some(note) = target.root_note else {
        return outcome;
    };
    match outcome {
        Ok(mut row) => {
            crate::response::paging::push_warnings(&mut row, [note]);
            Ok(row)
        }
        Err(Exit::Row(mut row)) => {
            crate::response::paging::push_warnings(&mut row, [note]);
            Err(Exit::Row(row))
        }
        failed => failed,
    }
}

/// [`run_page`] once the server is discovered.
pub(super) async fn run_target(
    query: &LspSearchQuery,
    target: &Target,
    cancel: &dyn CancellationCheck,
    pool: &LspClientPool,
    paths: &PathPolicy,
    response_window: Option<usize>,
) -> Result<Value, Exit> {
    let language_id = target.config.language_id.as_deref();
    let scope = scope::Scope::resolve(
        &target.path,
        &target.config.workspace_root,
        paths,
        language_id,
    );
    // Read the anchor document once (bounded, regular files only). The same
    // text is sent in didOpen, resolves the anchor, and serves same-file
    // context windows; a bad document or anchor costs no server start.
    let mut sources = SourceCache::new(paths);
    let document = read_anchor_document(query, target, &mut sources).await?;
    scope_responses(query, &scope, document.as_deref(), cancel).await?;
    let text = document.as_deref().map(|source| source.content.as_str());
    let anchor =
        anchor::resolve_anchor(query, &target.path, &target.uri, text).map_err(|error| {
            let mut row = failure(
                query,
                &target.uri,
                ErrorCode::AnchorUnresolved,
                &error,
                target.server_resolves(),
            );
            failure::anchor_recovery(&mut row, query, text);
            Exit::Row(row)
        })?;
    let session = open_session(
        query,
        target,
        document.as_deref(),
        &mut sources,
        cancel,
        pool,
    )
    .await?;
    if session.open_readiness.as_deref() == Some("timeout") {
        quarantine_responses();
    }
    // One request reads each snippet file once, however many responses
    // name it.
    let snippet_policy = snippet_policy(paths).shared_reads();
    let mut result = ops::Operation {
        client: &session.client,
        query,
        sources: &mut sources,
        snippet_policy: &snippet_policy,
        cancel,
        path: &target.path,
        scope: &scope,
        root_only: target.root_only,
        line: anchor.line,
        character: anchor.character,
        language_id,
        budget: crate::response::page_windows::page_budget(
            response_window,
            &lsp_search_query::to_row(query),
        ),
        response_window,
    }
    .run()
    .await?;
    if needs_text_scan(query, language_id, &result)
        && let Some(name) = lsp_search_query::symbol_name(query)
    {
        scope.text_files(name, paths, cancel).await?;
    }
    annotate(
        &mut result,
        query,
        target,
        &session,
        &scope,
        anchor.resolved_symbol,
    );
    receipt::relative_server_roots(&mut result, paths);
    if matches!(lsp_search_query::op(query), Op::References | Op::Callers) {
        locations::attach_read_lead(&mut result);
    }
    Ok(with_next(query, result))
}

/// Whether coverage annotation may flag `row` with a text lead: an
/// incoming, name-anchored, non-error row of a language with a coverage
/// signal. Its `textOnlyFiles` needs the scope's text scan first.
pub(super) fn needs_text_scan(
    query: &LspSearchQuery,
    language_id: Option<&str>,
    row: &Value,
) -> bool {
    use octocode_engine::lsp::family::{is_c_family, is_typescript_family};
    inferred_project::is_incoming(lsp_search_query::operation(query))
        && (is_typescript_family(language_id)
            || is_c_family(language_id)
            || matches!(language_id, Some("python" | "rust")))
        && lsp_search_query::symbol_name(query).is_some_and(|name| !name.trim().is_empty())
        && row.get("status").and_then(Value::as_str) != Some("error")
}
