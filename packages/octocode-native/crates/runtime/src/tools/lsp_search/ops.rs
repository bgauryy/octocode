//! Per-operation language-server requests and their row shaping, run after
//! `execute` has admitted the query, acquired a leased client, synchronized
//! the document, and resolved the anchor.

use super::anchor::Position;
use super::cancellable;
use super::failure::{
    ALIAS_RECOVERY_FAILED, IMPORTER_REQUEST_FAILED, LspFailure, RequestFailures, empty,
    mark_diagnostics_unpublished, partial_with_related_rerun, shown_paths, text_search_lead,
};
use super::heritage;
use super::importers::{self, Importers, WindowRequest};
use super::locations::{
    RECOVERED_ALIAS, items_payload, locations, public_hover, public_range, public_workspace_symbol,
    salted_items_payload, salted_locations, snapshot_changed,
};
use super::recovery::{
    AliasSearch, get_counted_locations, get_locations, get_spelled_locations, recover_aliases,
    resolve_definition_chain, snippet_identity,
};
use super::render::uri_to_path;
use super::render::{into_items, items};
use super::scope::Scope;
use super::source::{SourceCache, filter_authorized_items};
use super::walk::{HierarchyRequest, ImporterRecovery, hierarchy};
use super::{LspSearchQuery, Op};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::lsp_search_query;
use octocode_engine::lsp::client::{LocationRequest, NativeLspClient, SnippetReadPolicy};
use octocode_engine::lsp::config::{representative_source_for, workspace_root_languages};
use serde_json::{Value, json};
use std::collections::HashSet;

mod diagnostics;
mod hierarchy;
mod lookups;
mod references;
use diagnostics::{RUST_MACRO_NOTE, anchor_diagnostics};
pub(super) use diagnostics::{dedupe_diagnostics, diagnostic_items, disabled_macro_diagnostics};
use lookups::*;

/// Label for references found from a verified importer anchor.
const RECOVERED_IMPORTER: &str = "recoveredImporter";

pub(super) const PUSH_DIAGNOSTICS_WAIT_MS: u32 = 1_500;

/// How long an empty anchored answer waits for the file's diagnostics. The
/// note they feed is additive: without them the row stays as answered.
const ANCHOR_DIAGNOSTICS_WAIT_MS: u32 = 1_000;

/// `lsp.source` of rows read from source by the heritage scan, not from the
/// language server.
const SCAN_SOURCE: &str = "native-graph-facts";

/// Partial reason of a heritage-scan answer standing in for a server that
/// did not answer.
const SERVER_UNANSWERED: &str = "serverUnanswered";

/// Partial reason of a heritage-scan answer missing the files it could not
/// read.
const HERITAGE_UNREAD: &str = "heritageUnread";

/// Partial reason of a server answer beside which the heritage scan found
/// declarations the server left out (listed, labelled, unverified).
const SCAN_ONLY: &str = "scanOnlyImplementations";

/// How `textDocument/implementation` was answered.
enum Implemented {
    /// The server's answer, with the heritage scan when it was kept beside it.
    Server {
        found: Vec<octocode_engine::lsp::types::LspCodeSnippet>,
        scan: Option<heritage::HeritageScan>,
    },
    /// The heritage scan, with the note that labels it and the files it
    /// could not read.
    Scan {
        found: Vec<octocode_engine::lsp::types::LspCodeSnippet>,
        note: String,
        unread: Vec<String>,
    },
}

/// The anchor type has implementation candidates past this walk's window:
/// the row is partial, and on its last result page `next.nextCandidatePage`
/// walks the next window of candidates (each window checked exactly once).
fn mark_more_candidates(row: &mut Value, query: &LspSearchQuery, digest: &str) {
    crate::response::paging::mark_partial(row, "hierarchyCandidateLimit", &[]);
    if row.pointer("/pagination/hasMore").and_then(Value::as_bool) == Some(true) {
        return;
    }
    let mut base = lsp_search_query::to_row(query);
    if let Some(fields) = base.as_object_mut() {
        fields
            .retain(|key, value| !value.is_null() && !matches!(key.as_str(), "page" | "snapshot"));
    }
    let Some(next_candidates) = crate::response::paging::next_page_call(
        row,
        ToolId::LspSearch,
        None,
        base,
        "candidatePage",
        crate::num::usize_of(lsp_search_query::candidate_page(query)).saturating_add(1),
        &[("snapshot", json!(digest))],
    ) else {
        return;
    };
    row["next"]["nextCandidatePage"] = next_candidates;
}

/// Flag a heritage-scan row incomplete: its note states what it is, and the
/// retry asks the server again. Files the scan could not read are named,
/// with a text search over them, so the gap can be followed.
fn mark_scan_answer(
    row: &mut Value,
    query: &LspSearchQuery,
    scope: &Scope,
    paths: &PathPolicy,
    note: String,
    unread: &[String],
) {
    partial_with_related_rerun(row, query, SERVER_UNANSWERED, &[note]);
    if unread.is_empty() || !row.is_object() {
        return;
    }
    // The warning and the lead name each file the way the response does.
    let unread = shown_paths(paths, unread);
    crate::response::paging::mark_partial(
        row,
        HERITAGE_UNREAD,
        &[heritage::describe_unread(&unread)],
    );
    if let Some(base) = lsp_search_query::symbol_name(query) {
        crate::response::result::attach_related(
            row,
            text_search_lead(base, &scope.root, &scope.include, Some(&unread)).build(),
        );
    }
}

/// The heritage scan's declarations outside the server's answer, with the
/// base it matched: a scan row is in the answer when a server location in
/// its file spans its line (servers locate an implementation at its name or
/// over its whole declaration).
fn scan_only(
    found: &[octocode_engine::lsp::types::LspCodeSnippet],
    scan: heritage::HeritageScan,
) -> (Vec<octocode_engine::lsp::types::LspCodeSnippet>, String) {
    let file = |uri: &str| super::scope::canonical_uri(uri);
    let answered = found
        .iter()
        .map(|snippet| {
            (
                file(&snippet.uri),
                snippet.range.start.line,
                snippet.range.end.line,
            )
        })
        .collect::<Vec<_>>();
    let extra = scan
        .found
        .into_iter()
        .filter(|snippet| {
            let (path, line) = (file(&snippet.uri), snippet.range.start.line);
            !answered
                .iter()
                .any(|(file, start, end)| *file == path && (*start..=*end).contains(&line))
        })
        .collect();
    (extra, scan.base)
}

/// Everything one operation needs, borrowed from `execute`.
pub(super) struct Operation<'a, 'p> {
    pub(super) client: &'a NativeLspClient,
    pub(super) query: &'a LspSearchQuery,
    pub(super) sources: &'a mut SourceCache<'p>,
    pub(super) snippet_policy: &'a SnippetReadPolicy,
    pub(super) cancel: &'a dyn CancellationCheck,
    /// Canonical file (or workspace-root directory) path of the request.
    pub(super) path: &'a str,
    /// Where lexical scans and leads search; records the answer's files.
    pub(super) scope: &'a Scope,
    /// A `workspaceRoot`-only request (no source file).
    pub(super) root_only: bool,
    /// Zero-based LSP anchor (0,0 for document-wide operations).
    pub(super) line: u32,
    pub(super) character: u32,
    /// Language of the anchor file, for server-specific recovery.
    pub(super) language_id: Option<&'a str>,
    /// Characters a page of rows may spend (see `page_windows::page_budget`).
    pub(super) budget: usize,
    pub(super) response_window: Option<usize>,
}

impl<'a> Operation<'a, '_> {
    /// The request's zero-based anchor.
    fn position(&self) -> Position<'a> {
        Position {
            path: self.path,
            line: self.line,
            character: self.character,
        }
    }
}

impl Operation<'_, '_> {
    /// The operation's row; an empty answer at a position the server could
    /// not analyze is marked partial with that error (see
    /// `failure::mark_anchor_line_errors`).
    pub(super) async fn run(self) -> Result<Value, LspFailure> {
        let (client, path, line, cancel, query) =
            (self.client, self.path, self.line, self.cancel, self.query);
        let anchored = !matches!(
            lsp_search_query::op(self.query),
            Op::DocumentSymbols | Op::WorkspaceSymbol | Op::Diagnostic
        );
        let mut row = self.answer().await?;
        if anchored {
            let empty = row.get("kind").and_then(Value::as_str) == Some("empty");
            let diagnostics = anchor_diagnostics(client, path, cancel, empty).await?;
            if empty || diagnostics.is_some() {
                super::analysis_context::annotate_diagnostics(
                    &mut row,
                    query,
                    diagnostics
                        .as_ref()
                        .map(|(items, truncated)| (items, *truncated)),
                    line + 1,
                );
            }
        }
        Ok(row)
    }

    async fn answer(self) -> Result<Value, LspFailure> {
        let query = self.query;
        match lsp_search_query::op(query) {
            Op::Definition => self.definition().await,
            Op::References => self.references().await,
            Op::TypeDefinition => {
                let (found, withheld) = get_counted_locations(
                    self.client,
                    self.snippet_policy,
                    self.cancel,
                    LocationRequest::TypeDefinition,
                    (self.path, self.line, self.character),
                )
                .await?;
                if found.is_empty() && withheld > 0 {
                    return Ok(super::locations::withheld(self.query, withheld));
                }
                Ok(self
                    .locations("typeDefinition", "typeDefinitionProvider", found)
                    .await)
            }
            Op::Implementation => self.implementation().await,
            Op::Hover => self.hover().await,
            Op::DocumentSymbols => {
                let mut symbols = std::sync::Arc::unwrap_or_clone(
                    cancellable(
                        self.cancel,
                        self.client.get_document_symbols(self.path.to_owned()),
                    )
                    .await??,
                );
                if octocode_engine::lsp::family::is_typescript_family(self.language_id)
                    && let Some(source) = self.sources.get(self.path).await
                {
                    super::render::name_type_aliases(&mut symbols, &source.content);
                }
                Ok(items_payload(
                    query,
                    "documentSymbols",
                    symbols,
                    self.budget,
                ))
            }
            Op::WorkspaceSymbol => self.workspace_symbol().await,
            Op::Diagnostic => self.diagnostic().await,
            Op::Callers | Op::Callees | Op::Supertypes | Op::Subtypes => self.hierarchy().await,
        }
    }
}

#[cfg(test)]
mod tests;
