//! The source revision each page proves (the anchor text and, for a scoped
//! operation, the scope as of one instant), and the project-load and
//! provider annotations on a result. The scope is walked for a revision
//! only by a page that proves one: a continuation re-proving its first
//! page's, or a page that issues one ([`seal_scope_revision`]); a fresh
//! answer that issues none walks it for nothing.
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Flag a non-error row partial when the open wait that preceded it timed
/// out: the server was still loading the project. The note reads the same on
/// every row so flagged: a notice, said once per response, while each such
/// row keeps its own reason and rerun.
pub(super) fn flag_still_loading(
    result: &mut Value,
    query: &LspSearchQuery,
    readiness: Option<Readiness>,
) {
    if readiness == Some(Readiness::Timeout) && result.get("status") != Some(&"error".into()) {
        let reason = crate::response::paging::PartialReason::LanguageServerIndexing;
        partial_with_related_rerun(result, query, reason, &[]);
        crate::response::paging::push_notice(result, still_loading_notice());
    }
}

/// What a row answered while the project was still loading says.
fn still_loading_notice() -> String {
    format!(
        "The language server was still loading the project after {} ms; results may be incomplete.",
        octocode_engine::lsp::client::OPEN_READY_TIMEOUT_MS
    )
}

#[cfg(test)]
mod still_loading_tests {
    use super::*;
    use serde_json::json;

    /// Two rows answered while the project loaded and one after: the note is
    /// said once (R2), both flagged rows keep their reason and rerun, and no
    /// response says what the reason means (R1).
    #[test]
    fn the_still_loading_note_is_said_once_without_the_reason_meaning() {
        let query: LspSearchQuery = serde_json::from_value(json!({
            "operation": "references", "path": "/w/a.rs", "symbolName": "f", "line": 1
        }))
        .expect("query");
        let rows = [Some(Readiness::Timeout), Some(Readiness::Timeout), None]
            .into_iter()
            .map(|readiness| {
                let mut row =
                    json!({"files": [{"path": "/w/a.rs", "matches": []}], "matchCount": 0});
                flag_still_loading(&mut row, &query, readiness);
                row
            })
            .collect::<Vec<_>>();
        let response =
            crate::tools::test_support::staged_response(crate::id::ToolId::LspSearch, rows);
        let text = response.to_string();
        assert_eq!(
            text.matches("still loading the project").count(),
            1,
            "{response}"
        );
        crate::tools::test_support::checks::assert_no_reason_meaning(&response);
        for index in 0..2 {
            assert_eq!(
                response["results"][index]["data"]["partialReasons"],
                json!(["languageServerIndexing"]),
                "{response}"
            );
        }
    }
}

/// Incoming relation answers depend on the scope as well as the anchor.
pub(super) const SCOPED_OPERATIONS: [Op; 3] = [Op::References, Op::Callers, Op::Callees];

/// Separates a scoped revision's instant (Unix nanoseconds) from its digest;
/// an unscoped revision (the anchor digest, hex) never holds it.
const OBSERVED_AT: char = '@';

/// A fresh operation owns its provider answers. The revision only proves
/// whether an issued continuation still describes the same source scope:
/// the anchor digest and, for a scoped operation, the scope fingerprint as
/// of `observed` (none when that walk proved none), with that instant, so a
/// later page observes the scope in the same shape at the same instant.
pub(super) fn response_scope(
    query: &LspSearchQuery,
    content_digest: String,
    observed: SystemTime,
    fingerprint: Option<&str>,
) -> octocode_engine::lsp::client::ResponseScope {
    let generation = if SCOPED_OPERATIONS.contains(&lsp_search_query::op(query)) {
        let nanos = observed
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let digest = fingerprint.map_or(content_digest.clone(), |fingerprint| {
            octocode_engine::digest::sha256(
                format!("{content_digest}\u{0}{fingerprint}").as_bytes(),
            )
        });
        format!("{nanos}{OBSERVED_AT}{digest}")
    } else {
        content_digest
    };
    octocode_engine::lsp::client::ResponseScope {
        generation,
        ..Default::default()
    }
}

pub(super) async fn scope_responses(
    query: &LspSearchQuery,
    scope: &scope::Scope,
    document: Option<&source::Source>,
    cancel: &dyn CancellationCheck,
) -> Result<bool, LspFailure> {
    let Some(document) = document else {
        return Ok(true);
    };
    let digest = octocode_engine::digest::sha256(document.content.as_bytes());
    // A continuation observes the scope as of its first page: the
    // fingerprint digests a recently modified file's content, so observing
    // it at another instant would change the revision of an unchanged scope
    // once that file ages.
    let pinned_observed = octocode_engine::lsp::client::RESPONSE_SCOPE
        .try_with(|scope| observed_at(&scope.borrow().generation))
        .ok()
        .flatten();
    let observed = pinned_observed.unwrap_or_else(SystemTime::now);
    // Only a continuation (a generation that names its instant) re-proves
    // the scope here; a fresh page seals it when it issues one.
    let fingerprint =
        if SCOPED_OPERATIONS.contains(&lsp_search_query::op(query)) && pinned_observed.is_some() {
            scope
                .walked(observed, cancel)
                .await?
                .fingerprint
                .map(|(digest, _)| digest)
        } else {
            None
        };
    let next = response_scope(query, digest, observed, fingerprint.as_deref());
    Ok(octocode_engine::lsp::client::RESPONSE_SCOPE
        .try_with(|scope| {
            let mut scope = scope.borrow_mut();
            if !scope.generation.is_empty() && scope.generation != next.generation {
                return false;
            }
            scope.generation = next.generation;
            true
        })
        .unwrap_or(true))
}

/// A sealed revision no later page can match: the scope changed while the
/// page that sealed it ran, so its rows may not describe the source it pins.
/// A digest is hex, never this.
const UNSETTLED: &str = "unsettled";

/// Pin the revision a fresh page that issues a continuation proves: the
/// anchor text and the scope fingerprint, from the request's one scope walk
/// ([`scope::Scope::walked`]). A walk made before the page's reads (a
/// watching server's) proves it. A walk made now, after them, proves it only
/// if no file it covers was written since `started`, the instant the page
/// began ([`scope::written_since`]); otherwise the revision is
/// [`UNSETTLED`], and the next page answers `staleSnapshot` with its restart
/// lead, the refusal of any changed scope. A file the answer cites that is
/// gone by then ([`scope::Scope::answered_removed`]) leaves no stamp, so it
/// unsettles the revision too.
pub(super) async fn seal_scope_revision(
    query: &LspSearchQuery,
    scope: &scope::Scope,
    document: Option<&source::Source>,
    started: SystemTime,
    cancel: &dyn CancellationCheck,
) -> Result<(), LspFailure> {
    let Some(document) = document else {
        return Ok(());
    };
    if !SCOPED_OPERATIONS.contains(&lsp_search_query::op(query)) {
        return Ok(());
    }
    let before_reads = scope.is_walked();
    let walked = scope.walked(SystemTime::now(), cancel).await?;
    let digest = octocode_engine::digest::sha256(document.content.as_bytes());
    let mut sealed = response_scope(
        query,
        digest,
        walked.observed,
        walked
            .fingerprint
            .as_ref()
            .map(|(digest, _)| digest.as_str()),
    );
    if !before_reads
        && (walked
            .fingerprint
            .as_ref()
            .is_some_and(|(_, entries)| scope::written_since(entries, started))
            || scope.answered_removed(cancel).await?)
    {
        let nanos = walked
            .observed
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        sealed.generation = format!("{nanos}{OBSERVED_AT}{UNSETTLED}");
    }
    let _ = octocode_engine::lsp::client::RESPONSE_SCOPE.try_with(|scope| {
        scope.borrow_mut().generation = sealed.generation;
    });
    Ok(())
}

/// The instant a scoped revision observed its scope at; `None` for a
/// fresh request or an unscoped revision (the anchor digest alone).
fn observed_at(generation: &str) -> Option<SystemTime> {
    let (nanos, _) = generation.split_once(OBSERVED_AT)?;
    let nanos = u64::try_from(nanos.parse::<u128>().ok()?).ok()?;
    UNIX_EPOCH.checked_add(std::time::Duration::from_nanos(nanos))
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
    flag_still_loading(result, query, session.open_readiness);
    server_coverage::flag_server_health(
        result,
        query,
        session.client.health(),
        &target.path,
        scope,
    );
    if let Some(reason) = &session.fell_back_from {
        let reason = crate::security::quoted(reason);
        crate::response::paging::push_warnings(
            result,
            [format!(
                "The native TypeScript server (tsgo) did not start ({reason}); typescript-language-server (tsserver) answered."
            )],
        );
    }
    if let Some((resident, budget)) = session.pool_over_budget {
        crate::response::paging::push_warnings(
            result,
            [super::session::disclose_pool_overflow(resident, budget)],
        );
    }
    super::session::record_waits(
        result,
        disclose_waits(
            &session.config,
            session.client.waits_since(session.began),
            session.began.elapsed(),
        ),
    );
    let language_id = target.config.language_id.as_deref();
    inferred_project::annotate(result, query, language_id, &target.path, scope);
    inferred_project::compile_database::annotate_compile_database(
        result,
        query,
        &target.config,
        &target.path,
        scope,
    );
    let rust = receipt::RustAnalysis::of(&session.config)
        .allowing_build_scripts(target.allow_build_scripts);
    failure::annotate_empty_includes(result, query, rust, stopped);
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
/// index. The first request records the disk the server initially indexed.
/// Capture and comparison run on blocking workers after taking the client's
/// synchronization lock; a continuation's older observation is not reused.
/// The returned observation proves a page only if captured before its reads;
/// the caller owns that phase. A late registration still synchronizes the server,
/// but its post-read observation cannot bypass the seal's change checks.
/// A server that registers for no file events makes no synchronization walk.
pub(super) async fn sync_watched_files(
    scope: &scope::Scope,
    client: &octocode_engine::lsp::client::NativeLspClient,
    cancel: &dyn CancellationCheck,
) -> Result<Option<scope::Walked>, LspFailure> {
    if !client.watches_files().await {
        return Ok(None);
    }
    let mut captured = None;
    cancellable(
        cancel,
        client.sync_watched_files(|previous| {
            let captured = &mut captured;
            async move {
                // Always capture after acquiring the client's sync lock, never
                // reuse a continuation's cached historical observation.
                let observed = SystemTime::now();
                let Some(fingerprint) = scope.walk_fingerprint(observed, cancel).await? else {
                    cancel.check().map_err(LspFailure::cancelled)?;
                    return Err(watched_snapshot_failed());
                };
                let entries = std::sync::Arc::clone(&fingerprint.1);
                let root = std::path::PathBuf::from(&scope.root);
                let update = blocking_cancellable(cancel, move |stopped| {
                    octocode_engine::lsp::client::WatchedFileUpdate::new(
                        &root,
                        previous.as_ref(),
                        std::sync::Arc::unwrap_or_clone(entries),
                        |relative, before, after| {
                            scope::same_file(&root.join(relative), before, after, stopped)
                        },
                    )
                })
                .await?
                .ok_or_else(watched_snapshot_failed)??;
                cancel.check().map_err(LspFailure::cancelled)?;
                *captured = Some(scope::Walked {
                    observed,
                    fingerprint: Some(fingerprint),
                });
                Ok::<_, LspFailure>(update)
            }
        }),
    )
    .await??;
    cancel.check().map_err(LspFailure::cancelled)?;
    Ok(captured)
}

fn watched_snapshot_failed() -> LspFailure {
    LspFailure {
        code: ErrorCode::DocumentSyncFailed,
        message: "Could not establish a complete source snapshot for language-server file events. A source or directory could not be read consistently. Retry when the source files are stable and readable.".to_owned(),
        retryable: true,
        lexical_fallback: false,
    }
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
    fn every_page_is_its_own_call_whatever_its_revision() {
        let query = callers();
        let unbounded = response_scope(&query, "content".into(), SystemTime::now(), None);
        let fingerprinted = response_scope(&query, "content".into(), SystemTime::now(), Some("fp"));
        assert!(unbounded.call.is_some(), "a page past the walk bounds");
        assert!(fingerprinted.call.is_some());
        assert_ne!(unbounded.call, fingerprinted.call, "pages are not one call");
    }

    /// A scoped revision names the instant it observed its scope at, with
    /// or without a fingerprint, which a continuation page observes again;
    /// an unscoped revision names none.
    #[test]
    fn a_scoped_revision_carries_its_fingerprint_instant() {
        let observed = UNIX_EPOCH + std::time::Duration::from_nanos(1_700_000_000_123_456_789);
        let scoped = response_scope(&callers(), "content".into(), observed, Some("fp"));
        assert_eq!(observed_at(&scoped.generation), Some(observed));
        let later = response_scope(
            &callers(),
            "content".into(),
            observed + std::time::Duration::from_secs(1),
            Some("fp"),
        );
        assert_ne!(later.generation, scoped.generation);
        let unfingerprinted = response_scope(&callers(), "content".into(), observed, None);
        assert_eq!(observed_at(&unfingerprinted.generation), Some(observed));
        assert_ne!(unfingerprinted.generation, scoped.generation);
        let hover = serde_json::from_value(serde_json::json!({
            "operation": "hover", "path": "/repo/a.ts", "symbolName": "f", "line": 1
        }))
        .expect("lsp query");
        let unscoped = response_scope(&hover, "content".into(), observed, Some("fp"));
        assert_eq!(unscoped.generation, "content");
        assert_eq!(observed_at(&unscoped.generation), None);
    }

    #[test]
    fn fresh_pages_own_independent_provider_evidence() {
        let observed = SystemTime::now();
        let first = response_scope(&callers(), "content".into(), observed, Some("fp"));
        let next = response_scope(&callers(), "content".into(), observed, Some("fp"));
        assert_eq!(first.generation, next.generation);
        assert!(!std::sync::Arc::ptr_eq(
            first.answers.as_ref().expect("answers"),
            next.answers.as_ref().expect("answers")
        ));
    }
}
