//! One page of an operation: the target it runs on, pinned and unpinned
//! pages, and the per-target run.
use super::*;
use octocode_engine::lsp::client::EmptyAnswer;

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
    /// What is wrong with the configuration behind this route.
    pub(super) config_notes: Vec<String>,
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

/// A saved page requires its source snapshot. Reacquisition is explicit related work.
pub(super) fn unpinned_page_row(query: &LspSearchQuery) -> Value {
    json!({
        "status": "error",
        "errorCode": ErrorCode::InvalidInput,
        "error": "A saved LSP page requires its snapshot; restart the request to acquire a new answer.",
        "related": [crate::response::result::restart_query(
                crate::id::ToolId::LspSearch,
                &lsp_search_query::to_row(query),
            )]
    })
}

pub(super) fn unpinned_later_page(query: &LspSearchQuery) -> bool {
    lsp_search_query::page(query).is_some_and(|page| page > 1)
        && lsp_search_query::snapshot(query).is_none()
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
    let notes = target
        .root_note
        .into_iter()
        .chain(target.config_notes)
        .collect::<Vec<_>>();
    if notes.is_empty() {
        return outcome;
    }
    match outcome {
        Ok(mut row) => {
            crate::response::paging::push_warnings(&mut row, notes);
            Ok(row)
        }
        Err(Exit::Row(mut row)) => {
            crate::response::paging::push_warnings(&mut row, notes);
            Err(Exit::Row(row))
        }
        failed => failed,
    }
}

/// The row of an anchor that resolves to nothing, with its recovery.
fn anchor_unresolved(
    query: &LspSearchQuery,
    target: &Target,
    text: Option<&str>,
    error: &str,
) -> Exit {
    let mut row = failure(
        query,
        &target.uri,
        ErrorCode::AnchorUnresolved,
        error,
        target.server_resolves(),
    );
    failure::anchor_recovery(&mut row, query, text);
    Exit::Row(row)
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
    // Before any read of the request's sources: a file written after it
    // may be read by this page before the write.
    let started = std::time::SystemTime::now();
    let language_id = target.config.language_id.as_deref();
    let scope = scope::Scope::resolve(
        &target.path,
        &target.config.workspace_root,
        lsp_search_query::workspace_root(query),
        paths,
        language_id,
    );
    // Read the anchor document once (bounded, regular files only). The same
    // text is sent in didOpen, resolves the anchor, and serves same-file
    // context windows; a bad document or anchor costs no server start.
    let mut sources = SourceCache::new(paths);
    let document = read_anchor_document(query, target, &mut sources).await?;
    if !scope_responses(query, &scope, document.as_deref(), cancel).await? {
        return Ok(locations::snapshot_changed(query));
    }
    let text = document.as_deref().map(|source| source.content.as_str());
    let anchor = anchor::resolve_anchor(query, &target.path, &target.uri, text)
        .map_err(|error| anchor_unresolved(query, target, text, &error))?;
    // Every code-use check of the scope (the operation's and the one below)
    // bounds the scanned files by the anchor declaration's reach.
    scope.anchor_site(&target.path, anchor.line, anchor.character);
    scope.asks(scope::Question::of(lsp_search_query::op(query)));
    let session = open_session(
        query,
        target,
        document.as_deref(),
        &mut sources,
        cancel,
        pool,
    )
    .await?;
    sync_watched_files(&scope, &session.client, cancel).await?;
    // One request reads each snippet file once, however many responses
    // name it.
    let snippet_policy = snippet_policy(paths).shared_reads();
    let load_deadline = session.client.load_deadline();
    // Absence needs evidence: an empty answer stands only once the server
    // shows it has finished loading. A later page reads the first page's
    // answers, which were judged when they were asked.
    let mut result = loop {
        let asked = session.client.asked_at();
        let mut row = ops::Operation {
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
            budget: crate::response::page_windows::page_budget(response_window),
            response_window,
        }
        .run()
        .await
        .map_err(|failure| crashed(query, &target.uri, &session.config, failure))?;
        if lsp_search_query::snapshot(query).is_some() || !claims_absence(&row) {
            break row;
        }
        match cancellable(
            cancel,
            session.client.judge_empty_answer(asked, load_deadline),
        )
        .await?
        {
            EmptyAnswer::Stands => break row,
            EmptyAnswer::AskAgain => {}
            // The declarations it can see stay: partial, they claim nothing.
            EmptyAnswer::StillLoading
                if row.get("status").and_then(Value::as_str) != Some("empty") =>
            {
                failure::partial_with_related_rerun(
                    &mut row,
                    query,
                    crate::response::paging::PartialReason::LanguageServerIndexing,
                    &[],
                );
                break row;
            }
            EmptyAnswer::StillLoading => break failure::still_indexing(query),
        }
    };
    if needs_text_scan(query, language_id, &result)
        && let Some(name) = lsp_search_query::symbol_name(query)
    {
        scope.text_files(name, paths, cancel).await?;
        scope.check_code_uses(name, cancel).await?;
        // Ask the server what each file's spelling names: only one it cannot
        // place elsewhere can be a use it missed.
        if server_coverage::lacks_signal(language_id)
            && server_coverage::lists_no_use(&result)
            && !target.root_only
            && scope
                .text_only_files(name)
                .is_some_and(|files| !files.is_empty())
        {
            let checked = importers::verified_anchors(
                &session.client,
                &mut sources,
                &snippet_policy,
                cancel,
                importers::ImporterSearch {
                    symbol: name,
                    scope: &scope,
                    declaration: super::anchor::Position {
                        path: &target.path,
                        line: anchor.line,
                        character: anchor.character,
                    },
                    known_files: &std::collections::HashSet::new(),
                    window: importers::WindowRequest::default(),
                    proof: importers::SiteProof::DefinitionChain,
                    reads_imports: false,
                },
            )
            .await
            .map_err(|failure| crashed(query, &target.uri, &session.config, failure))?;
            scope.answer(checked.rejected);
        }
    }
    annotate(
        &mut result,
        query,
        target,
        &session,
        &scope,
        anchor.resolved_symbol,
        &|| cancel.check().is_err(),
    );
    importers::flag_markup_importers(
        &mut result,
        query,
        language_id,
        (&scope, &target.path, text),
        paths,
        cancel,
    )
    .await?;
    if matches!(lsp_search_query::op(query), Op::References | Op::Callers) {
        locations::attach_read_lead(&mut result);
    }
    let mut row = with_next(query, result);
    // A continuation page re-proved the revision its first page pinned.
    if lsp_search_query::snapshot(query).is_none()
        && continuations::declares_continuation(&mut row)?
    {
        seal_scope_revision(query, &scope, document.as_deref(), started, cancel).await?;
    }
    Ok(row)
}

/// Whether `row` reads as a settled answer that nothing was found: no use
/// listed (empty, or declarations alone), and not already flagged incomplete.
fn claims_absence(row: &Value) -> bool {
    server_coverage::lists_no_use(row)
        && row.get("isPartial").and_then(Value::as_bool) != Some(true)
}

/// A server stopped for its memory cannot answer this scope again: its
/// row leads to a text search and names the server that was stopped. Any
/// other failure stays as it is.
pub(super) fn crashed(
    query: &LspSearchQuery,
    canonical_uri: &str,
    config: &octocode_engine::lsp::types::LspLanguageServerConfig,
    failure: LspFailure,
) -> Exit {
    if !failure.lexical_fallback {
        return Exit::Failed(failure);
    }
    receipt::naming(
        Exit::Row(failure.row(query, canonical_uri)),
        receipt::server_receipt(config, true),
    )
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
            || matches!(language_id, Some("python" | "rust"))
            || server_coverage::lists_no_use(row))
        && lsp_search_query::symbol_name(query).is_some_and(|name| !name.trim().is_empty())
        && row.get("status").and_then(Value::as_str) != Some("error")
}
