//! Response scoping across operations that reuse one server answer, and the
//! project-load and provider annotations on a result.
use super::*;

/// Operations whose first page may reuse server responses from an earlier
/// request: their answers depend only on the files the scope fingerprint
/// covers. Diagnostics and hover always ask the server.
pub(super) const REUSED_OPERATIONS: [Op; 3] = [Op::References, Op::Callers, Op::Callees];

/// The page's response scope. Every cached key carries the anchor content
/// digest; for [`REUSED_OPERATIONS`] it also carries the scope fingerprint
/// (any edit, addition, or removal of a family or project file misses), so
/// a first page may reuse an earlier request's answers. Without a
/// fingerprint (walk past its bounds) only continuation pages reuse, as the
/// snapshot check proves they belong to the same result set. A later
/// importer window is a continuation too: reusing the anchor's answer keeps
/// its candidate list (and so the window) the one its digest names, even
/// after earlier windows opened files the server now also searches.
pub(super) fn response_scope(
    query: &LspSearchQuery,
    content_digest: String,
    fingerprint: Option<&str>,
) -> octocode_engine::lsp::client::ResponseScope {
    let continuation = (lsp_search_query::page(query).is_some_and(|page| page > 1)
        || lsp_search_query::importer_page(query) > 1)
        && lsp_search_query::snapshot(query).is_some();
    match fingerprint {
        Some(fingerprint) if REUSED_OPERATIONS.contains(&lsp_search_query::op(query)) => {
            octocode_engine::lsp::client::ResponseScope {
                reuse: true,
                generation: octocode_engine::digest::sha256(
                    format!("{content_digest}\u{0}{fingerprint}").as_bytes(),
                ),
            }
        }
        _ => octocode_engine::lsp::client::ResponseScope {
            reuse: continuation,
            generation: content_digest,
        },
    }
}

/// Set this page's response scope from the anchor text and, for a
/// [`REUSED_OPERATIONS`] query, the scope fingerprint (one bounded walk).
pub(super) async fn scope_responses(
    query: &LspSearchQuery,
    scope: &scope::Scope,
    document: Option<&source::Source>,
    cancel: &dyn CancellationCheck,
) -> Result<(), LspFailure> {
    let Some(document) = document else {
        return Ok(());
    };
    let digest = octocode_engine::digest::sha256(document.content.as_bytes());
    let fingerprint = if REUSED_OPERATIONS.contains(&lsp_search_query::op(query)) {
        let walked = scope.walk_inputs();
        blocking_cancellable(cancel, move |stopped| walked.fingerprint(stopped))
            .await?
            .flatten()
    } else {
        None
    };
    if let Some(fingerprint) = &fingerprint {
        scope.set_fingerprint(fingerprint.clone());
    }
    let next = response_scope(query, digest, fingerprint.as_deref());
    let _ =
        octocode_engine::lsp::client::RESPONSE_SCOPE.try_with(|scope| *scope.borrow_mut() = next);
    Ok(())
}

/// Responses read while the project was still loading may be incomplete:
/// key them apart, so no later request reuses them.
pub(super) fn quarantine_responses() {
    let _ = octocode_engine::lsp::client::RESPONSE_SCOPE.try_with(|scope| {
        let mut scope = scope.borrow_mut();
        scope.reuse = false;
        scope.generation.push_str("\u{0}loading");
    });
}

/// Disclose an unfinished project load, the inferred-project and compile
/// database context, and the provider receipt.
pub(super) fn annotate(
    result: &mut Value,
    query: &LspSearchQuery,
    target: &Target,
    session: &Session,
    scope: &scope::Scope,
    resolved_symbol: Option<Value>,
) {
    if session.open_readiness.as_deref() == Some("timeout")
        && result.get("status") != Some(&"error".into())
    {
        partial_with_related_rerun(
            result,
            query,
            "languageServerIndexing",
            &[format!(
                "The language server was still loading the project after {} ms; results may be incomplete.",
                octocode_engine::lsp::client::OPEN_READY_TIMEOUT_MS
            )],
        );
    }
    if let Some(reason) = &session.fell_back_from {
        crate::response::paging::push_warnings(
            result,
            [format!(
                "The native TypeScript server (tsgo) did not start ({reason}); typescript-language-server (tsserver) answered."
            )],
        );
    }
    let language_id = target.config.language_id.as_deref();
    inferred_project::annotate(result, query, language_id, &target.path, scope);
    inferred_project::annotate_compile_database(result, query, language_id, &target.path, scope);
    server_coverage::annotate(
        result,
        query,
        language_id,
        server_coverage::Server {
            anchor: &target.path,
            root: &target.config.workspace_root,
        },
        scope,
    );
    receipt::attach_provider_context(
        result,
        query,
        &target.uri,
        resolved_symbol,
        &session.config,
        &session.client,
    );
}
