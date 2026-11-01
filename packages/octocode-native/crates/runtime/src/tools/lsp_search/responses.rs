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
/// after earlier windows opened files the server now also searches. Every
/// page is its own call: within it, a repeated request reuses the answer
/// even without a fingerprint.
pub(super) fn response_scope(
    query: &LspSearchQuery,
    content_digest: String,
    fingerprint: Option<&str>,
) -> octocode_engine::lsp::client::ResponseScope {
    let continuation = (lsp_search_query::page(query).is_some_and(|page| page > 1)
        || lsp_search_query::importer_page(query) > 1)
        && lsp_search_query::snapshot(query).is_some();
    let call = Some(octocode_engine::lsp::client::CallId::next());
    match fingerprint {
        Some(fingerprint) if REUSED_OPERATIONS.contains(&lsp_search_query::op(query)) => {
            octocode_engine::lsp::client::ResponseScope {
                reuse: true,
                generation: octocode_engine::digest::sha256(
                    format!("{content_digest}\u{0}{fingerprint}").as_bytes(),
                ),
                call,
            }
        }
        _ => octocode_engine::lsp::client::ResponseScope {
            reuse: continuation,
            generation: content_digest,
            call,
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
        scope
            .walk_fingerprint(cancel)
            .await?
            .map(|(digest, entries)| {
                scope.set_walk_entries(entries);
                digest
            })
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
/// key them apart, so no later request (in this call or another) reuses
/// them.
pub(super) fn quarantine_responses() {
    let _ = octocode_engine::lsp::client::RESPONSE_SCOPE.try_with(|scope| {
        let mut scope = scope.borrow_mut();
        scope.reuse = false;
        scope.call = None;
        scope.generation.push_str("\u{0}loading");
    });
}

/// Disclose an unfinished project load, the inferred-project and compile
/// database context, and the answering provider.
pub(super) fn annotate(
    result: &mut Value,
    query: &LspSearchQuery,
    target: &Target,
    session: &Session,
    scope: &scope::Scope,
    resolved_symbol: Option<Value>,
    stopped: &dyn Fn() -> bool,
) {
    if session.open_readiness.as_deref() == Some("timeout")
        && result.get("status") != Some(&"error".into())
    {
        partial_with_related_rerun(
            result,
            query,
            crate::response::paging::PartialReason::LanguageServerIndexing,
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
    let rust = receipt::RustAnalysis::of(&session.config);
    failure::annotate_empty_includes(result, query, rust.build_scripts, stopped);
    server_coverage::annotate(
        result,
        query,
        language_id,
        server_coverage::Server {
            anchor: &target.path,
            root: &target.config.workspace_root,
        },
        scope,
        rust,
        stopped,
    );
    receipt::attach_provider_context(result, query, &target.uri, resolved_symbol, &session.config);
}

/// Tell a server that registered for `workspace/didChangeWatchedFiles`
/// which scope files changed since this process's last request to it:
/// without a watcher of its own, a file no request opened (a new
/// importer, a deleted module, regenerated output) would never reach its
/// index. The first request of a server process records the snapshot
/// only (the process indexed the disk itself). The walk is the scope
/// fingerprint's, reused when the request already made it.
pub(super) async fn sync_watched_files(
    scope: &scope::Scope,
    client: &octocode_engine::lsp::client::NativeLspClient,
    cancel: &dyn CancellationCheck,
) -> Result<(), LspFailure> {
    if !client.watches_files().await {
        return Ok(());
    }
    let entries = match scope.walk_entries() {
        Some(entries) => entries,
        None => {
            let Some((digest, entries)) = scope.walk_fingerprint(cancel).await? else {
                // The request stopped the walk or a file could not be
                // stamped (the trace counts it): no snapshot, no events.
                return Ok(());
            };
            scope.set_fingerprint(digest);
            scope.set_walk_entries(entries.clone());
            entries
        }
    };
    let Some(changes) = client.watched_files_diff(&entries) else {
        return Ok(());
    };
    let root = std::path::Path::new(&scope.root);
    let changes = changes
        .into_iter()
        .filter_map(|(relative, kind)| {
            octocode_engine::lsp::uri::path_to_uri(&root.join(relative).to_string_lossy())
                .ok()
                .map(|uri| (uri, kind))
        })
        .collect::<Vec<_>>();
    // A failed notification is the connection's to report on the next
    // request; this answer is unaffected.
    let _ = client.notify_watched_files(changes).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callers() -> LspSearchQuery {
        serde_json::from_value(serde_json::json!({
            "operation": "callers", "path": "/repo/a.ts", "symbolName": "f", "line": 1
        }))
        .expect("lsp query")
    }

    #[test]
    fn every_page_is_its_own_call_whatever_its_reuse() {
        let query = callers();
        let unbounded = response_scope(&query, "content".into(), None);
        let fingerprinted = response_scope(&query, "content".into(), Some("fp"));
        assert!(unbounded.call.is_some(), "a page past the walk bounds");
        assert!(fingerprinted.call.is_some());
        assert_ne!(unbounded.call, fingerprinted.call, "pages are not one call");
    }

    #[tokio::test]
    async fn a_quarantined_page_reuses_nothing_even_within_its_call() {
        let scope = response_scope(&callers(), "content".into(), Some("fp"));
        let generation = scope.generation.clone();
        let quarantined = octocode_engine::lsp::client::RESPONSE_SCOPE
            .scope(std::cell::RefCell::new(scope), async {
                quarantine_responses();
                octocode_engine::lsp::client::RESPONSE_SCOPE.with(|scope| scope.borrow().clone())
            })
            .await;
        assert!(!quarantined.reuse);
        assert_eq!(quarantined.call, None);
        assert_ne!(quarantined.generation, generation);
    }
}
