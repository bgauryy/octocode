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
    /// start, routed only when asked: none where there is no substitute.
    pub(super) fallback: Fallback,
    /// Why the server runs at another root than the one inferred for the
    /// file: a warning every row of the request carries.
    pub(super) root_note: Option<String>,
    /// What is wrong with the configuration behind this route.
    pub(super) config_notes: Vec<String>,
    /// `lsp.allowBuildScripts`: whether a `rustContext` may run build scripts.
    pub(super) allow_build_scripts: bool,
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
            || self.fallback.config().is_some_and(server_command_resolves)
    }
}

/// Where the target's file would route without octocode's native TypeScript
/// server. Routing it resolves the file's server twice, so it runs only on a
/// path that needs it (a start that failed, a row saying whether one could
/// start), once per request.
pub(super) struct Fallback {
    pub(super) path: String,
    pub(super) workspace: String,
    pub(super) root_only: bool,
    pub(super) discovery: octocode_engine::lsp::config::LspDiscoveryOptions,
    pub(super) routed:
        std::sync::OnceLock<Option<octocode_engine::lsp::types::LspLanguageServerConfig>>,
}

impl Fallback {
    /// A fallback already routed to `config`.
    #[cfg(test)]
    pub(super) fn known(
        config: Option<octocode_engine::lsp::types::LspLanguageServerConfig>,
    ) -> Self {
        Self {
            path: String::new(),
            workspace: String::new(),
            root_only: false,
            discovery: Default::default(),
            routed: std::sync::OnceLock::from(config),
        }
    }

    pub(super) fn config(&self) -> Option<&octocode_engine::lsp::types::LspLanguageServerConfig> {
        self.routed
            .get_or_init(|| {
                if self.root_only {
                    typescript_server_fallback_for_workspace_root(&self.workspace, &self.discovery)
                } else {
                    typescript_server_fallback(&self.path, &self.workspace, &self.discovery)
                }
            })
            .as_ref()
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
            level::stamp(&mut row);
            level::omit_healthy_receipt(&mut row);
            Ok(row)
        }
        Err(Exit::Failed(failure)) if failure.lexical_fallback => {
            let mut row = failure.row(&query, lsp_search_query::path(&query).unwrap_or_default());
            failure::stamp_coverage(&mut row);
            level::stamp(&mut row);
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
    security: &crate::security::ContentSecurity,
    response_window: Option<usize>,
) -> Result<Value, Exit> {
    // Before any read of the request's sources: a file written after it
    // may be read by this page before the write.
    let started = octocode_engine::stamp::settle_clock();
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
    // The revision a continuation pins is proved by the rung that answers it:
    // here, once the server runs. A server that does not start leaves it to
    // the syntax rung, which proves its own.
    if !scope_responses(query, &scope, document.as_deref(), cancel).await? {
        return Ok(locations::snapshot_changed(query));
    }
    // An outline reads only the anchor, which its open synchronized: no other
    // file's change bears on it, so it walks no scope.
    if lsp_search_query::op(query) != Op::DocumentSymbols {
        sync_watched_files(&scope, &session.client, cancel).await?;
    }
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
            security,
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
    unanswered::check(
        unanswered::Request {
            query,
            target,
            session: &session,
            scope: &scope,
            anchor: &anchor,
            snippet_policy: &snippet_policy,
            paths,
            cancel,
        },
        &mut sources,
        &result,
    )
    .await?;
    let answers_sites = matches!(lsp_search_query::op(query), Op::References | Op::Callers);
    if answers_sites && let Some(name) = lsp_search_query::symbol_name(query) {
        scope.check_answer_gaps(name, cancel).await?;
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
    if answers_sites {
        failure::flag_answer_gaps(&mut result, query, &scope, paths);
    }
    importers::flag_markup_importers(
        &mut result,
        query,
        language_id,
        (&scope, &target.path, text),
        paths,
        cancel,
    )
    .await?;
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

/// Whether coverage annotation may flag `row` with a text lead: an incoming,
/// name-anchored, non-error row, in any language: every answer can miss a file.
/// Its `textOnlyFiles` needs the scope's text scan first.
pub(super) fn needs_text_scan(query: &LspSearchQuery, row: &Value) -> bool {
    inferred_project::is_incoming(lsp_search_query::operation(query))
        && lsp_search_query::symbol_name(query).is_some_and(|name| !name.trim().is_empty())
        && row.get("status").and_then(Value::as_str) != Some("error")
}
