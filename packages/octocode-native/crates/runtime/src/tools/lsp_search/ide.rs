//! The IDE rung of the resolution ladder: a running editor's own language
//! providers, reached through the Octocode extension's loopback bridge
//! (`<home>/ide/<pid>.json`), instead of a second, cold language server.
//!
//! The rung applies only when a live editor with a trusted lock file has the
//! file's folder open; with none, it is skipped without a trace, after one
//! stat of the lock directory. Once it applies, any failure (bridge
//! unreachable or refusing, an untrusted lock or directory, no provider for
//! the language, no answer in the rung's half of the request's time) falls
//! to the server rung, with the reason on the final row
//! (`lsp.fellBackFrom`). Policy refusals stay the server rung's
//! to give: the rung does not apply where one would, so no editor is asked
//! about code the server rung would refuse to open.
//!
//! - [`lock`] (shared, `tools/editor_lock`): lock files, trust and the choice among live editors
//! - [`client`]: the authenticated HTTP call
//! - [`ops`]: the operations asked of the bridge and shaped as server answers
//! - [`hierarchy`]: call and type hierarchies over the shared walk
use super::failure::LspFailure;
use super::level::Level;
use super::{LspExecutionConfig, LspSearchQuery, Op, anchor, locations, lsp_search_query, receipt};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::tools::editor_lock as lock;
use serde_json::{Value, json};

mod client;
mod hierarchy;
mod ops;
mod snippets;
#[cfg(test)]
mod tests;

use client::{Bridge, BridgeError};

/// Why the rung did not produce an answer it applied to.
pub(super) enum Fault {
    /// The rung failed: the server rung answers, and the row says why.
    Fallback(String),
    /// The request itself stopped (cancelled, past its deadline, invalid).
    Stop(LspFailure),
}

impl From<BridgeError> for Fault {
    fn from(error: BridgeError) -> Self {
        Self::Fallback(error.reason())
    }
}

impl From<LspFailure> for Fault {
    fn from(failure: LspFailure) -> Self {
        Self::Stop(failure)
    }
}

/// One page, asked of the editor.
pub(super) struct Request<'a> {
    pub(super) query: &'a LspSearchQuery,
    /// Canonical file (or workspace-root directory) of the request.
    pub(super) path: &'a str,
    pub(super) paths: &'a PathPolicy,
    pub(super) cancel: &'a dyn CancellationCheck,
    pub(super) execution_config: &'a LspExecutionConfig,
    pub(super) response_window: Option<usize>,
}

impl Request<'_> {
    /// The editor's answer: `Ok(None)` where the rung does not apply (no
    /// editor, or a request the bridge cannot express or a policy refuses).
    pub(super) async fn answer(self) -> Result<Option<Value>, Fault> {
        let Self {
            query,
            path,
            paths,
            cancel,
            execution_config,
            response_window,
        } = self;
        let Some(home) = execution_config.octocode_home.as_deref() else {
            return Ok(None);
        };
        // No lock directory (no editor ever ran the extension): nothing below
        // is worth its cost.
        if !lock::may_apply(home) {
            return Ok(None);
        }
        let operation = lsp_search_query::op(query);
        let root_only = std::path::Path::new(path).is_dir();
        // The bridge asks about a file; only a workspace search needs none.
        // It has no `includeDeclaration` either: leaving the declaration
        // out is the server's to do.
        if (root_only && operation != Op::WorkspaceSymbol)
            || (operation == Op::References
                && lsp_search_query::include_declaration(query) == Some(false))
            || (receipt::requests_build_scripts(query) && !execution_config.allow_build_scripts)
        {
            return Ok(None);
        }
        // The editor answers in the first half of what the request has left;
        // the server and syntax rungs get the rest.
        let ends = crate::execution::cancel::halfway(cancel);
        let Ok((workspace, _)) = super::resolve_workspace(query, path, paths) else {
            return Ok(None);
        };
        let discovery = execution_config.discovery().for_one_request();
        let lock =
            match lock::editor_lock(&discovery, home, path, &workspace, lock::Hints::of_process) {
                lock::Discovery::Absent => return Ok(None),
                lock::Discovery::Refused(reason) => return Err(Fault::Fallback(reason)),
                lock::Discovery::Found(lock) => lock,
            };
        let bridge = Bridge::of(&lock, ends);
        let Ok(uri) = octocode_engine::lsp::uri::path_to_uri(path) else {
            return Ok(None);
        };
        let mut sources = super::SourceCache::new(paths);
        let document = if root_only {
            None
        } else {
            // A document the server rung cannot read is its typed row to give.
            match super::read_bounded_source_async(std::path::PathBuf::from(path)).await {
                Ok(content) => Some(sources.insert(path, content)),
                Err(_) => return Ok(None),
            }
        };
        let started = octocode_engine::stamp::settle_clock();
        let scope = super::scope::Scope::resolve(
            path,
            &workspace,
            lsp_search_query::workspace_root(query),
            paths,
            None,
        );
        if !super::scope_responses(query, &scope, document.as_deref(), cancel).await? {
            return Ok(Some(locations::snapshot_changed(query)));
        }
        let text = document.as_deref().map(|source| source.content.as_str());
        // An anchor that resolves to nothing is the server rung's row to give.
        let Ok(anchor) = anchor::resolve_anchor(query, path, &uri, text) else {
            return Ok(None);
        };
        let mut row = ops::Question {
            bridge: &bridge,
            query,
            sources: &mut sources,
            cancel,
            scope: &scope,
            path,
            root_only,
            line: anchor.line,
            character: anchor.character,
            budget: crate::response::page_windows::page_budget(response_window),
            security: &execution_config.security,
        }
        .run()
        .await?;
        if row.get("errorCode").is_some() {
            return Ok(Some(row));
        }
        note_empty_answer(&mut row, query, &bridge, path, anchor.line, cancel).await?;
        receipt_of(
            &mut row,
            query,
            &uri,
            &workspace,
            &bridge,
            anchor.resolved_symbol,
        );
        let mut row = super::with_next(query, row);
        if lsp_search_query::snapshot(query).is_none()
            && super::continuations::declares_continuation(&mut row)?
        {
            super::seal_scope_revision(query, &scope, document.as_deref(), started, cancel).await?;
        }
        Ok(Some(row))
    }
}

/// An empty anchored answer carries what the document's diagnostics held, as
/// a server's does (`analysisContext`): the editor's report says whether the
/// emptiness may rest on errors. The note is additive; an editor that cannot
/// report leaves `unavailable`, and only cancellation fails the request.
async fn note_empty_answer(
    row: &mut Value,
    query: &LspSearchQuery,
    bridge: &Bridge,
    path: &str,
    anchor_line: u32,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<(), Fault> {
    let anchored = !matches!(
        lsp_search_query::op(query),
        Op::DocumentSymbols | Op::WorkspaceSymbol | Op::Diagnostic
    );
    if !anchored || row.get("status").and_then(Value::as_str) != Some("empty") {
        return Ok(());
    }
    let report =
        match super::cancellable(cancel, bridge.ask("diagnostics", json!({ "path": path }))).await?
        {
            Ok(report) => Some(super::ops::diagnostic_items(Some(report))),
            Err(_) => None,
        };
    super::analysis_context::annotate_diagnostics(
        row,
        query,
        report
            .as_ref()
            .map(|(items, truncated)| (items, *truncated)),
        anchor_line + 1,
    );
    Ok(())
}

/// The row's receipt: the editor answered (`level: ide`, `ide.name`), for
/// the anchor file in the workspace the editor has open.
fn receipt_of(
    row: &mut Value,
    query: &LspSearchQuery,
    uri: &str,
    workspace: &str,
    bridge: &Bridge,
    resolved_symbol: Option<Value>,
) {
    let Some(envelope) = row.as_object_mut() else {
        return;
    };
    envelope.insert("path".into(), json!(super::render::uri_to_path(uri)));
    if let Some(symbol) = receipt::public_resolved_symbol(query, resolved_symbol, uri) {
        envelope.insert("resolvedSymbol".into(), symbol);
    }
    if let Some(lsp) = envelope
        .entry("lsp")
        .or_insert_with(|| json!({}))
        .as_object_mut()
    {
        lsp.insert("serverAvailable".into(), json!(true));
        lsp.insert("level".into(), json!(Level::Ide.name()));
        lsp.insert("ide".into(), json!({ "name": bridge.ide }));
        receipt::settle_provider(lsp, lsp_search_query::operation(query));
    }
    envelope.insert("workspaceRoot".into(), json!(workspace));
}

/// Say on the final row that the IDE rung was tried and failed, and why. A
/// row that already names a rung it fell back from (the syntax rung's)
/// keeps it and carries this one as a warning.
pub(super) fn stamp_fell_back(row: &mut Value, reason: &str) {
    let Some(lsp) = row.as_object_mut().and_then(|row| {
        row.entry("lsp")
            .or_insert_with(|| json!({}))
            .as_object_mut()
    }) else {
        return;
    };
    if lsp.contains_key("fellBackFrom") {
        crate::response::paging::push_warnings(
            row,
            [format!("The IDE bridge did not answer first: {reason}")],
        );
    } else {
        lsp.insert(
            "fellBackFrom".into(),
            json!({ "level": Level::Ide.name(), "reason": reason }),
        );
    }
}
