//! Per-operation language-server requests and their row shaping, run after
//! `execute` has admitted the query, acquired a leased client, synchronized
//! the document, and resolved the anchor.

use super::anchor::Position;
use super::cancellable;
use super::failure::{
    ALIAS_RECOVERY_FAILED, IMPORTER_REQUEST_FAILED, LspFailure, RequestFailures, empty,
    mark_anchor_line_errors, mark_diagnostics_unpublished, partial_with_retry, shown_paths,
    text_search_lead,
};
use super::heritage;
use super::importers::{self, Importers, WindowRequest};
use super::locations::{
    RECOVERED_ALIAS, items_payload, locations, public_hover, public_range, public_workspace_symbol,
    salted_items_payload, salted_locations, snapshot_changed,
};
use super::recovery::{
    AliasSearch, get_locations, get_spelled_locations, recover_aliases, resolve_definition_chain,
    snippet_identity,
};
use super::render::items;
use super::render::uri_to_path;
use super::scope::Scope;
use super::source::{SourceCache, filter_authorized_items};
use super::walk::{HierarchyRequest, ImporterRecovery, hierarchy};
use super::{LspSearchQuery, Op};
use crate::execution::cancel::CancellationCheck;
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::response::result::Confidence;
use crate::tools::lsp_search::lsp_search_query;
use octocode_engine::lsp::client::{LocationRequest, NativeLspClient, SnippetReadPolicy};
use octocode_engine::lsp::config::{representative_source_for, workspace_root_languages};
use serde_json::{Value, json};
use std::collections::HashSet;

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
    let Some(candidate_page) = crate::response::paging::next_cursor(
        row,
        ToolId::LspSearch,
        None,
        "candidatePage",
        crate::num::usize_of(lsp_search_query::candidate_page(query)).saturating_add(1),
    ) else {
        return;
    };
    let mut next = lsp_search_query::to_row(query);
    if let Some(fields) = next.as_object_mut() {
        fields
            .retain(|key, value| !value.is_null() && !matches!(key.as_str(), "page" | "snapshot"));
        fields.insert("candidatePage".into(), json!(candidate_page));
        fields.insert("snapshot".into(), json!(digest));
    }
    row["next"]["nextCandidatePage"] =
        crate::response::result::Continuation::new(ToolId::LspSearch, next)
            .confidence(Confidence::Exact)
            .build();
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
    partial_with_retry(row, query, SERVER_UNANSWERED, &[note]);
    if unread.is_empty() || !row.is_object() {
        return;
    }
    // The warning and the lead name each file the way the response does.
    let unread = shown_paths(paths, unread);
    partial_with_retry(
        row,
        query,
        HERITAGE_UNREAD,
        &[heritage::describe_unread(&unread)],
    );
    if let Some(base) = lsp_search_query::symbol_name(query) {
        row["next"]["findText"] =
            text_search_lead(base, &scope.root, &scope.include, Some(&unread)).build();
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
        let (client, path, line, cancel) = (self.client, self.path, self.line, self.cancel);
        let anchored = !matches!(
            lsp_search_query::op(self.query),
            Op::DocumentSymbols | Op::WorkspaceSymbol | Op::Diagnostic
        );
        let mut row = self.answer().await?;
        if anchored
            && row.get("kind").and_then(Value::as_str) == Some("empty")
            && let Some(diagnostics) = anchor_diagnostics(client, path, cancel).await?
        {
            mark_anchor_line_errors(&mut row, &diagnostics, line + 1);
        }
        Ok(row)
    }

    async fn answer(self) -> Result<Value, LspFailure> {
        let query = self.query;
        match lsp_search_query::op(query) {
            Op::Definition => self.definition().await,
            Op::References => self.references().await,
            Op::TypeDefinition => {
                let found = self
                    .location_request(LocationRequest::TypeDefinition)
                    .await?;
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

    async fn definition(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let (found, warnings) = resolve_definition_chain(
            self.client,
            self.sources,
            self.snippet_policy,
            self.cancel,
            self.path,
            self.line,
            self.character,
        )
        .await?;
        let mut row = self
            .locations("definition", "definitionProvider", found)
            .await;
        if !warnings.is_empty() {
            super::failure::partial_with_retry(&mut row, query, "definitionHopFailed", &warnings);
        }
        Ok(row)
    }

    /// References from the anchor, plus alias and verified-importer
    /// recoveries, each recovered row labelled with its source. Importer
    /// page 1 lists the anchor's answer and the first importer window; a
    /// later page (`importerPage` > 1) lists only its window's recovered
    /// rows. A recovered row in a candidate file belongs to that file's
    /// window, so each row is listed once across importer pages.
    async fn references(mut self) -> Result<Value, LspFailure> {
        let query = self.query;
        let include_declaration = lsp_search_query::include_declaration(query).unwrap_or(true);
        let (mut found, respelled) = get_spelled_locations(
            self.client,
            self.snippet_policy,
            self.cancel,
            LocationRequest::References {
                include_declaration,
            },
            (self.path, self.line, self.character),
        )
        .await?;
        let recovered = recover_aliases(
            self.client,
            self.sources,
            self.snippet_policy,
            self.cancel,
            AliasSearch {
                symbol: lsp_search_query::symbol_name(query),
                include_declaration,
                anchor: self.position(),
                provider: &found,
            },
        )
        .await?;
        let alias_scan_capped = recovered.capped;
        let alias_failures = recovered.failures;
        let mut recovered = recovered.snippets;
        // Files the answer names, its declaration's included: answered
        // even when the declaration row is left out.
        let known_files = known_files(&found, &recovered);
        let importers = self.importers(&known_files).await?;
        if importers.as_ref().is_some_and(|importers| importers.stale) {
            return Ok(snapshot_changed(query));
        }
        // The server drops the declaration under the spelling it knows; a
        // symlinked twin and every importer's answer still carry it.
        let asks_importers = importers
            .as_ref()
            .is_some_and(|importers| importers.per_file().next().is_some());
        let declared = if include_declaration || !(respelled || asks_importers) {
            None
        } else {
            let declared = self.declarations().await?;
            let undeclared = |snippet: &octocode_engine::lsp::types::LspCodeSnippet| {
                !declared.contains(&site_start(snippet))
            };
            found.retain(undeclared);
            recovered.retain(undeclared);
            Some(declared)
        };
        let mut seen = found
            .iter()
            .chain(&recovered)
            .map(snippet_identity)
            .collect::<HashSet<_>>();
        let later_window = lsp_search_query::importer_page(query) > 1;
        let mut importer_failures = RequestFailures::default();
        let from_importers = match &importers {
            Some(importers) => {
                self.importer_references(
                    importers,
                    declared.as_ref(),
                    &mut seen,
                    &mut importer_failures,
                )
                .await?
            }
            None => Vec::new(),
        };
        // Recovered references are not reported from the anchor:
        // label each so callers can weigh them as such.
        let labelled = |label: &'static str| {
            move |snippet| {
                let mut value = serde_json::to_value(snippet).unwrap_or(Value::Null);
                value["source"] = json!(label);
                value
            }
        };
        self.scope.answer(
            std::iter::once(self.path.to_owned())
                .chain(known_files)
                .chain(
                    from_importers
                        .iter()
                        .map(|snippet| uri_to_path(&snippet.uri)),
                )
                .chain(
                    importers
                        .iter()
                        .flat_map(Importers::per_file)
                        .map(|anchor| anchor.path.clone()),
                )
                .chain(importers.iter().flat_map(|found| found.rejected.clone()))
                .chain(importers.iter().flat_map(Importers::covered)),
        );
        let anchor_file = super::scope::canonical_uri(self.path);
        self.scope.answer_anchor_lines(
            self.path,
            self.line,
            found
                .iter()
                .chain(&recovered)
                .chain(&from_importers)
                .filter(|snippet| super::scope::canonical_uri(&snippet.uri) == anchor_file)
                .map(|snippet| snippet.range.start.line),
        );
        // A later importer page leaves out the anchor's answer (listed on
        // importer page 1); it still deduplicates this window's rows.
        if later_window {
            found.clear();
        }
        let recovered = if later_window { Vec::new() } else { recovered };
        let found = found
            .drain(..)
            .map(|snippet| serde_json::to_value(snippet).unwrap_or(Value::Null))
            .chain(recovered.into_iter().map(labelled(RECOVERED_ALIAS)))
            .chain(from_importers.into_iter().map(labelled(RECOVERED_IMPORTER)))
            .collect::<Vec<_>>();
        let mut row = salted_locations(
            self.query,
            self.sources,
            "references",
            "referencesProvider",
            found,
            importers.as_ref().and_then(Importers::digest),
            self.budget,
            self.response_window,
        )
        .await;
        if let Some(importers) = &importers {
            importers.annotate(&mut row, query, self.scope);
        }
        importer_failures.disclose(
            &mut row,
            query,
            IMPORTER_REQUEST_FAILED,
            "Importer recovery",
        );
        // Alias recovery belongs to importer page 1, which lists its rows.
        if !later_window {
            alias_failures.disclose(&mut row, query, ALIAS_RECOVERY_FAILED, "Alias recovery");
        }
        if !later_window && alias_scan_capped && row.pointer("/coverage").is_some() {
            if let Some(name) = lsp_search_query::symbol_name(query) {
                self.scope
                    .text_files(name, self.sources.policy(), self.cancel)
                    .await?;
            }
            super::recovery::disclose_alias_cap(&mut row, query, self.scope);
        }
        Ok(row)
    }

    /// The references the server reports from each verified importer's
    /// anchor, in the files that importer window owns, not already `seen`.
    /// An importer whose request fails is recorded in `failures`;
    /// cancellation ends the request.
    ///
    /// Each importer is asked with its declarations included: from an
    /// importer, `includeDeclaration` would drop its own import binding (a
    /// reference to the anchor's symbol) and keep the symbol's declaration.
    /// The `declared` sites (the anchor's declaration when the query leaves
    /// declarations out) are dropped here instead.
    async fn importer_references(
        &self,
        importers: &Importers,
        declared: Option<&HashSet<(String, u32, u32)>>,
        seen: &mut HashSet<String>,
        failures: &mut RequestFailures,
    ) -> Result<Vec<octocode_engine::lsp::types::LspCodeSnippet>, LspFailure> {
        let mut found = Vec::new();
        for anchor in importers.per_file() {
            let extra = match get_locations(
                self.client,
                self.snippet_policy,
                self.cancel,
                LocationRequest::References {
                    include_declaration: true,
                },
                &anchor.path,
                anchor.line,
                anchor.character,
            )
            .await
            {
                Ok(extra) => extra,
                Err(failure) => {
                    failures.record(failure)?;
                    continue;
                }
            };
            found.extend(extra.into_iter().filter(|snippet| {
                importers.owns(&super::scope::canonical_uri(&snippet.uri))
                    && declared.is_none_or(|declared| !declared.contains(&site_start(snippet)))
                    && seen.insert(snippet_identity(snippet))
            }));
        }
        Ok(found)
    }

    /// Where the anchor's symbol is declared (`definition` from the
    /// anchor), by canonical file and start. The same server just answered
    /// `references` from this anchor, so a failure fails the request: a
    /// declaration left in would read as a reference.
    async fn declarations(&self) -> Result<HashSet<(String, u32, u32)>, LspFailure> {
        let found = self.location_request(LocationRequest::Definition).await?;
        Ok(found.iter().map(site_start).collect())
    }

    async fn hover(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let hover = cancellable(
            self.cancel,
            self.client
                .get_hover(self.path.to_owned(), self.line, self.character),
        )
        .await??;
        Ok(match hover {
            Value::Null => empty(
                query,
                "noHover",
                "hoverProvider returned no hover at this position",
                true,
            ),
            hover => json!({
                "lsp": { "serverAvailable": true, "provider": "hoverProvider" },
                "kind": "hover",
                "hover": public_hover(hover)
            }),
        })
    }

    /// `supertypes`/`subtypes` on a server with no type hierarchy: the
    /// resolved immediate heritage edges in the shared bounded walker.
    /// Unverified syntax substitutes remain partial and name their source.
    async fn type_hierarchy_without_server_support(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let kind = lsp_search_query::operation(query);
        let expansion = if kind == "subtypes" {
            super::walk::Expansion::Subtypes
        } else {
            super::walk::Expansion::Supertypes
        };
        let mut candidates = if kind == "subtypes" {
            match self.implemented(true, false).await? {
                Implemented::Server { found, .. } => {
                    let Some(window) =
                        super::fallback_hierarchy::CandidateWindow::cut(query, found)
                    else {
                        return Ok(snapshot_changed(query));
                    };
                    Some(window)
                }
                Implemented::Scan {
                    found,
                    note,
                    unread,
                } => {
                    if lsp_search_query::candidate_page(query) > 1 {
                        return Ok(snapshot_changed(query));
                    }
                    let (scope, paths) = (self.scope, self.sources.policy());
                    scope.answer(
                        std::iter::once(self.path)
                            .chain(found.iter().map(|snippet| snippet.uri.as_str())),
                    );
                    let mut row = self.locations(kind, "implementationProvider", found).await;
                    row["lsp"]["source"] = json!(SCAN_SOURCE);
                    mark_scan_answer(&mut row, query, scope, paths, note, &unread);
                    return Ok(row);
                }
            }
        } else {
            None
        };
        let source = super::fallback_hierarchy::Fallback::new(
            self.client,
            self.sources,
            self.snippet_policy,
            self.cancel,
        );
        let anchor = Position {
            path: self.path,
            line: self.line,
            character: self.character,
        };
        let walk = match source
            .run(
                anchor,
                expansion,
                lsp_search_query::graph_depth(query).unwrap_or(1),
                candidates
                    .as_mut()
                    .map(|window| std::mem::take(&mut window.items)),
            )
            .await
        {
            Ok(walk) => walk,
            Err(failure) if failure.is_cancelled() => return Err(failure),
            Err(failure) => {
                let mut row = empty(
                    query,
                    "noLocations",
                    "The fallback could not establish the type hierarchy anchor.",
                    true,
                );
                partial_with_retry(
                    &mut row,
                    query,
                    "typeHierarchyExpansionFailed",
                    &[failure.message],
                );
                return Ok(row);
            }
        };
        self.scope.answer(
            std::iter::once(self.path).chain(
                walk.edges
                    .iter()
                    .filter_map(|edge| edge.node.get("uri").and_then(Value::as_str)),
            ),
        );
        let items = walk
            .edges
            .iter()
            .map(|edge| super::walk::public_edge(expansion, edge, None))
            .collect::<Vec<_>>();
        let mut row = salted_items_payload(
            query,
            kind,
            json!(items),
            candidates.as_ref().map(|window| window.digest.as_str()),
            self.budget,
            super::render::plain_sizes,
        );
        if row.get("errorCode").is_some() {
            return Ok(row);
        }
        row["lsp"]["provider"] = json!(heritage::fallback_provider(kind));
        row["lsp"]["source"] = json!(if kind == "subtypes" {
            "implementation"
        } else {
            "heritage"
        });
        super::walk::mark_partial_expansion(&mut row, query, &walk.failures);
        super::walk::mark_truncation(&mut row, query, &[(expansion, &walk)]);
        if let Some(window) = &candidates
            && window.has_more
        {
            mark_more_candidates(&mut row, query, &window.digest);
        }
        if source.candidate_limited() {
            crate::response::paging::terminal_limit(
                &mut row,
                "hierarchyCandidateLimit",
                &[format!(
                    "Only the first {} implementation candidates per type were checked for immediate heritage; narrow the scope or inspect the remaining candidates with implementation.",
                    super::walk::MAX_HIERARCHY_NODES
                )],
            );
            if let Some(name) = lsp_search_query::symbol_name(query) {
                row["next"]["findText"] =
                    text_search_lead(name, &self.scope.root, &self.scope.include, None).build();
            }
        }
        Ok(row)
    }

    /// `implementation`: the server's answer, or, for a type declaration the
    /// server cannot answer in time, the labelled heritage scan standing in
    /// for it. Another anchor (a member, a function) has no heritage clause
    /// a scan could read, so its failure stays the failure. Beside a server
    /// answer for a type, the scan's declarations the server left out (a
    /// function-local impl, say) follow its rows, each labelled.
    async fn implementation(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let content = self
            .sources
            .get(self.path)
            .await
            .map(|source| source.content.clone())
            .unwrap_or_default();
        let scannable = lsp_search_query::symbol_name(query).is_some_and(|name| {
            heritage::anchors_type_declaration(&content, self.path, self.line, name)
        });
        let (found, scanned, extra) = match self.implemented(scannable, scannable).await? {
            Implemented::Server { found, scan } => {
                let extra = scan.map(|scan| scan_only(&found, scan));
                (found, None, extra)
            }
            Implemented::Scan {
                found,
                note,
                unread,
            } => (found, Some((note, unread)), None),
        };
        let (scope, paths) = (self.scope, self.sources.policy());
        let extra_rows = extra.as_ref().map_or(&[][..], |(rows, _)| rows.as_slice());
        scope.answer(
            std::iter::once(self.path).chain(
                found
                    .iter()
                    .chain(extra_rows)
                    .map(|snippet| snippet.uri.as_str()),
            ),
        );
        let rows = found
            .into_iter()
            .map(|snippet| serde_json::to_value(snippet).unwrap_or(Value::Null))
            .chain(extra_rows.iter().map(|snippet| {
                let mut value = serde_json::to_value(snippet).unwrap_or(Value::Null);
                value["source"] = json!(SCAN_SOURCE);
                value
            }))
            .collect::<Vec<_>>();
        let mut row = self
            .locations("implementation", "implementationProvider", rows)
            .await;
        if let Some((note, unread)) = scanned {
            row["lsp"]["source"] = json!(SCAN_SOURCE);
            mark_scan_answer(&mut row, query, scope, paths, note, &unread);
        }
        if let Some((rows, base)) = &extra
            && !rows.is_empty()
            && row.get("errorCode").is_none()
        {
            crate::response::paging::mark_partial(
                &mut row,
                SCAN_ONLY,
                &[format!(
                    "{} implementation(s) labelled `source: \"{SCAN_SOURCE}\"` are not in the language server's answer: their extends/implements clause names `{base}`, read from source. They are unverified: the base is matched by name, not resolved to this declaration.",
                    rows.len()
                )],
            );
        }
        Ok(row)
    }

    /// The server's `textDocument/implementation` answer. The heritage scan
    /// (when `scannable`) runs while the request is pending: a failure that
    /// has the scan as fallback takes it without a second wait, and a server
    /// still silent a grace after the scan finishes yields to it. `keep`
    /// also waits for the scan beside a server answer.
    async fn implemented(&self, scannable: bool, keep: bool) -> Result<Implemented, LspFailure> {
        let scan = async {
            if scannable {
                self.heritage_scan().await
            } else {
                Ok(None)
            }
        };
        let raced = heritage::hedged(
            self.location_request(LocationRequest::Implementation),
            scan,
            heritage::implementation_failure_has_fallback,
            |scan| matches!(scan, Ok(Some(_))).then_some(heritage::SERVER_GRACE_AFTER_SCAN),
            |wait| {
                LspFailure::overdue(format!(
                    "no answer within {} s of the syntax scan finishing",
                    wait.as_secs()
                ))
            },
            keep,
        )
        .await;
        match raced {
            Ok((found, scan)) => Ok(Implemented::Server {
                found,
                // A scan that could not run adds nothing to the server's
                // answer; only cancellation stops the request.
                scan: match scan {
                    Some(Err(failure)) if failure.is_cancelled() => return Err(failure),
                    Some(Ok(scan)) => scan,
                    _ => None,
                },
            }),
            Err((failure, Some(scan))) => match scan? {
                Some(scan) => Ok(Implemented::Scan {
                    note: heritage::describe_scan(
                        &failure,
                        &scan.base,
                        scan.files,
                        lsp_search_query::operation(self.query),
                    ),
                    found: scan.found,
                    unread: scan.unread,
                }),
                None => Err(failure),
            },
            Err((failure, None)) => Err(failure),
        }
    }

    /// Subtypes read from the heritage clauses of every workspace file that
    /// spells the queried name: the syntax answer standing by for a failed
    /// implementation request. `None` when no name is queried or the text
    /// scan of the workspace could not run.
    async fn heritage_scan(&self) -> Result<Option<heritage::HeritageScan>, LspFailure> {
        let Some(base) =
            lsp_search_query::symbol_name(self.query).filter(|name| !name.trim().is_empty())
        else {
            return Ok(None);
        };
        let Some(files) = self
            .scope
            .text_files(base, self.sources.policy(), self.cancel)
            .await?
        else {
            return Ok(None);
        };
        let name = base.to_owned();
        let scanned = files.clone();
        let Some((found, unread)) = super::blocking_cancellable(self.cancel, move |stopped| {
            heritage::scan_subtypes(&scanned, &name, stopped)
        })
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(heritage::HeritageScan {
            base: base.to_owned(),
            files: files.len(),
            found,
            unread,
        }))
    }

    /// A call or type hierarchy walk; TS/JS incoming walks also verify the
    /// importers the server's level-1 answer does not cover.
    async fn hierarchy(self) -> Result<Value, LspFailure> {
        let query = self.query;
        if matches!(lsp_search_query::op(query), Op::Supertypes | Op::Subtypes)
            && !self
                .client
                .has_capability("typeHierarchyProvider".to_owned())
        {
            return self.type_hierarchy_without_server_support().await;
        }
        let recovery = match self.importer_symbol()? {
            Some(symbol) => Some(ImporterRecovery {
                symbol,
                snippet_policy: self.snippet_policy,
                window: WindowRequest::of(query),
            }),
            None => None,
        };
        let request = HierarchyRequest {
            query,
            scope: self.scope,
            anchor: self.position(),
            recovery,
            budget: self.budget,
        };
        let (mut row, importers) =
            hierarchy(self.client, self.sources, self.cancel, request).await?;
        if let Some(importers) = &importers {
            importers.annotate(&mut row, query, self.scope);
        }
        Ok(row)
    }

    /// The symbol whose importers recovery verifies, when the server may
    /// have missed importers (TS/JS incoming operations). An `importerPage`
    /// past the first names a window only importer recovery has.
    fn importer_symbol(&self) -> Result<Option<String>, LspFailure> {
        let operation = lsp_search_query::operation(self.query);
        let symbol = (importers::applies(self.language_id, operation) && !self.root_only)
            .then(|| lsp_search_query::symbol_name(self.query))
            .flatten()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned);
        if symbol.is_none() && lsp_search_query::importer_page(self.query) > 1 {
            return Err(LspFailure::invalid_query(
                "importerPage pages TypeScript/JavaScript importer recovery for references and callers; copy it from that recovery's continuation.",
            ));
        }
        Ok(symbol)
    }

    /// Verified importer anchors when the server may have missed importers
    /// (TS/JS incoming operations); `None` when recovery does not apply.
    async fn importers(
        &mut self,
        known_files: &HashSet<String>,
    ) -> Result<Option<Importers>, LspFailure> {
        let Some(symbol) = self.importer_symbol()? else {
            return Ok(None);
        };
        importers::verified_anchors(
            self.client,
            self.sources,
            self.snippet_policy,
            self.cancel,
            importers::ImporterSearch {
                symbol: &symbol,
                scope: self.scope,
                declaration: self.position(),
                known_files,
                window: WindowRequest::of(self.query),
            },
        )
        .await
        .map(Some)
    }

    async fn location_request(
        &self,
        request: LocationRequest,
    ) -> Result<Vec<octocode_engine::lsp::types::LspCodeSnippet>, LspFailure> {
        get_locations(
            self.client,
            self.snippet_policy,
            self.cancel,
            request,
            self.path,
            self.line,
            self.character,
        )
        .await
    }

    async fn locations(
        self,
        kind: &str,
        provider: &str,
        found: Vec<impl serde::Serialize>,
    ) -> Value {
        locations(
            self.query,
            self.sources,
            kind,
            provider,
            found,
            self.budget,
            self.response_window,
        )
        .await
    }

    async fn workspace_symbol(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let name = lsp_search_query::symbol_name(query)
            .map(str::to_owned)
            .ok_or_else(|| LspFailure::invalid_query("workspaceSymbol requires symbolName"))?;
        let symbols =
            cancellable(self.cancel, self.client.workspace_symbol(name.clone())).await??;
        // The server's own cap is judged on its raw answer, before the
        // read policy drops any rows.
        let capped_at = self
            .client
            .workspace_symbol_limit()
            .filter(|limit| symbols.as_array().map_or(0, Vec::len) == *limit);
        // workspace/symbol URIs are server-controlled and span the whole
        // project; drop any that fall outside the read policy before emitting.
        let mut symbols = items(&filter_authorized_items(symbols, self.sources.policy()))
            .iter()
            .map(public_workspace_symbol)
            .collect::<Vec<_>>();
        rank_workspace_symbols(&mut symbols, &name);
        let document_anchor = if self.root_only {
            symbols
                .first()
                .and_then(|symbol| symbol["path"].as_str())
                .filter(|path| std::path::Path::new(path).is_file())
        } else {
            Some(self.path)
        }
        .map(str::to_owned);
        let mut row = items_payload(query, "symbols", json!(symbols), self.budget);
        if self.root_only && row.pointer("/kind").and_then(Value::as_str) == Some("empty") {
            // A source directory (`src`) has no markers: name the languages
            // of the project root it was searched from.
            let mut languages = workspace_root_languages(self.path);
            if languages.is_empty()
                && let Some(root) = lsp_search_query::workspace_root(self.query)
            {
                languages = workspace_root_languages(root);
            }
            let searched = languages
                .first()
                .map_or("unknown", |ext| language_name(ext));
            row["lsp"]["language"] = json!(searched);
            // The server answered from one representative file's project;
            // other project markers in this root name languages it never
            // searched: route to each with a real source file as uri.
            let mut others = Vec::new();
            for extension in languages.iter().skip(1) {
                let Some(file) = representative_source_for(self.path, extension) else {
                    continue;
                };
                let language = language_name(extension);
                others.push(language);
                row["next"][format!("search{}", capitalized(language))] =
                    crate::response::result::Continuation::new(
                        ToolId::LspSearch,
                        json!({
                            "operation": "workspaceSymbol",
                            "symbolName": name,
                            "path": file
                        }),
                    )
                    .confidence(Confidence::Medium)
                    .build();
            }
            let hint = if others.is_empty() {
                format!(
                    "workspaceRoot-only search covered the {searched} project of one file; pass path for a source file in the symbol's project."
                )
            } else {
                format!(
                    "workspaceRoot-only search used the {searched} server; this root also holds {} unsearched projects.",
                    others.join(", ")
                )
            };
            row["hints"] = json!([hint]);
        }
        super::server_coverage::workspace_symbols(&mut row, document_anchor.as_deref(), capped_at);
        Ok(row)
    }

    async fn diagnostic(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let report = if self.client.has_capability("diagnosticProvider".to_owned()) {
            Some(
                cancellable(
                    self.cancel,
                    self.client.get_diagnostics(self.path.to_owned()),
                )
                .await??,
            )
        } else {
            cancellable(
                self.cancel,
                self.client
                    .get_push_diagnostics(self.path.to_owned(), Some(PUSH_DIAGNOSTICS_WAIT_MS)),
            )
            .await??
        };
        let published = report.is_some();
        let (items, truncated) = diagnostic_items(report);
        let mut row = items_payload(query, "diagnostics", items, self.budget);
        if !published {
            mark_diagnostics_unpublished(&mut row, query, PUSH_DIAGNOSTICS_WAIT_MS);
        }
        if truncated {
            crate::response::paging::terminal_limit(&mut row, "diagnosticsTruncated", &[]);
        }
        // Success rows drop `hints` (response hint policy), so this note is a
        // warning: it explains results, it is not a recovery step.
        if disabled_macro_diagnostics(&row["matches"]) {
            crate::response::paging::push_warnings(&mut row, [RUST_MACRO_NOTE.to_owned()]);
        }
        Ok(row)
    }
}

/// The published diagnostics the server holds for `path`, waiting at most
/// [`ANCHOR_DIAGNOSTICS_WAIT_MS`]. `None` when none arrive in time or the
/// request fails; only cancellation is an error.
async fn anchor_diagnostics(
    client: &NativeLspClient,
    path: &str,
    cancel: &dyn CancellationCheck,
) -> Result<Option<Value>, LspFailure> {
    let report = if client.has_capability("diagnosticProvider".to_owned()) {
        let wait = std::time::Duration::from_millis(ANCHOR_DIAGNOSTICS_WAIT_MS.into());
        cancellable(
            cancel,
            tokio::time::timeout(wait, client.get_diagnostics(path.to_owned())),
        )
        .await?
        .ok()
        .and_then(Result::ok)
    } else {
        cancellable(
            cancel,
            client.get_push_diagnostics(path.to_owned(), Some(ANCHOR_DIAGNOSTICS_WAIT_MS)),
        )
        .await?
        .ok()
        .flatten()
    };
    Ok(report.map(|report| diagnostic_items(Some(report)).0))
}

/// rust-analyzer codes for macros it did not expand. The engine starts it
/// headless (no build scripts or proc-macros), so these are expected noise.
const DISABLED_MACRO_CODES: &[&str] = &[
    "macro-error",
    "unresolved-proc-macro",
    "proc-macro-disabled",
];
const RUST_MACRO_NOTE: &str = "Proc-macros and build scripts are off by default (they run repository code), so rust-analyzer reports unexpanded macros; pass rustContext {buildScripts:true, procMacros:true} to expand them.";

pub(super) fn disabled_macro_diagnostics(items: &Value) -> bool {
    super::render::items(items).iter().any(|item| {
        item.get("code")
            .and_then(Value::as_str)
            .is_some_and(|code| DISABLED_MACRO_CODES.contains(&code))
    })
}

/// Replace the zero-based LSP `range` on a diagnostic (and on each related
/// location) with the one-based public `displayRange` every other result uses.
pub(super) fn publish_diagnostic_ranges(mut item: Value) -> Value {
    fn swap(object: &mut Value) {
        if let Some(display) = object.get("range").and_then(public_range)
            && let Some(map) = object.as_object_mut()
        {
            map.remove("range");
            map.insert("displayRange".into(), display);
        }
    }
    swap(&mut item);
    // LSP `DiagnosticSeverity` 1–4, by name like every other Octocode
    // diagnostic; an unknown number passes through unchanged.
    let label = match item.get("severity").and_then(Value::as_u64) {
        Some(1) => Some("error"),
        Some(2) => Some("warning"),
        Some(3) => Some("information"),
        Some(4) => Some("hint"),
        _ => None,
    };
    if let Some(label) = label {
        item["severity"] = json!(label);
    }
    if let Some(related) = item
        .get_mut("relatedInformation")
        .and_then(Value::as_array_mut)
    {
        for info in related {
            if let Some(location) = info.get_mut("location") {
                swap(location);
            }
        }
    }
    item
}

/// Flatten a pull (`DocumentDiagnosticReport`) or cached push report into the
/// list of diagnostics it carries, plus whether the push cache truncated it.
/// `unchanged` pull reports and absent reports carry no items.
pub(super) fn diagnostic_items(report: Option<Value>) -> (Value, bool) {
    let Some(report) = report else {
        return (json!([]), false);
    };
    let truncated = report.get("truncated").and_then(Value::as_bool) == Some(true);
    let items = match report {
        Value::Array(items) => items,
        Value::Object(mut object) => match object.remove("items") {
            Some(Value::Array(items)) => items,
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    (
        Value::Array(items.into_iter().map(publish_diagnostic_ranges).collect()),
        truncated,
    )
}

/// Relevance tier of a workspace symbol name for the query: exact, exact
/// ignoring case, prefix, substring, then any other (fuzzy) match.
fn match_tier(candidate: &str, query: &str) -> u8 {
    let (lower, wanted) = (candidate.to_lowercase(), query.to_lowercase());
    if candidate == query {
        0
    } else if lower == wanted {
        1
    } else if lower.starts_with(&wanted) {
        2
    } else if lower.contains(&wanted) {
        3
    } else {
        4
    }
}

/// The language id of a project-marker extension (`.rs` → `rust`), from the
/// engine's grammar registry.
fn language_name(extension: &str) -> &'static str {
    octocode_engine::signatures::languages::find_entry(extension.trim_start_matches('.'))
        .and_then(|entry| entry.language_id)
        .unwrap_or("unknown")
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Servers return fuzzy matches in their own order (rust-analyzer:
/// alphabetical), which can bury the exact name past the first page. Public
/// rows are ranked by `symbolName` tier; within a tier, declarations come
/// before variable bindings (tsserver lists every `import { Scene }` as a
/// `variable` named Scene ahead of `class Scene`). Stable sort keeps server
/// order otherwise.
fn rank_workspace_symbols(symbols: &mut [Value], name: &str) {
    symbols.sort_by_key(|symbol| {
        (
            match_tier(
                symbol
                    .get("symbolName")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                name,
            ),
            binding_rank(symbol.get("kind").and_then(Value::as_str).unwrap_or("")),
        )
    });
}

/// Bindings (variables, fields, properties) rank after declarations of the
/// same name: an import alias or a property is rarely the definition sought.
fn binding_rank(kind: &str) -> u8 {
    u8::from(matches!(kind, "variable" | "field" | "property"))
}

/// The canonical files a references answer already names.
fn known_files(
    found: &[octocode_engine::lsp::types::LspCodeSnippet],
    recovered: &[octocode_engine::lsp::types::LspCodeSnippet],
) -> HashSet<String> {
    found
        .iter()
        .chain(recovered)
        .map(|snippet| super::scope::canonical_uri(&snippet.uri))
        .collect()
}

/// A location's canonical file and start position.
fn site_start(snippet: &octocode_engine::lsp::types::LspCodeSnippet) -> (String, u32, u32) {
    (
        super::scope::canonical_uri(&snippet.uri),
        snippet.range.start.line,
        snippet.range.start.character,
    )
}

#[cfg(test)]
mod tests {
    use super::{binding_rank, language_name, match_tier, rank_workspace_symbols};

    fn snippet(path: &str, start: u32, end: u32) -> octocode_engine::lsp::types::LspCodeSnippet {
        use octocode_engine::lsp::types::{LspExactPosition, LspRange};
        let at = |line| LspExactPosition { line, character: 0 };
        octocode_engine::lsp::types::LspCodeSnippet {
            uri: format!("file://{path}"),
            range: LspRange {
                start: at(start),
                end: at(end),
            },
            content: String::new(),
            symbol_kind: None,
            display_range: None,
        }
    }

    /// The heritage scan beside a server answer adds only declarations the
    /// answer does not cover: a server location spanning the scan row's
    /// line (at its name, or over its whole declaration) is the same row.
    #[test]
    fn the_scan_adds_only_the_declarations_the_server_left_out() {
        let found = [snippet("/r/a.ts", 3, 3), snippet("/r/b.ts", 10, 20)];
        let scan = super::heritage::HeritageScan {
            base: "Base".into(),
            files: 3,
            found: vec![
                snippet("/r/a.ts", 3, 3),
                snippet("/r/b.ts", 12, 12),
                snippet("/r/b.ts", 25, 25),
                snippet("/r/c.ts", 3, 3),
            ],
            unread: Vec::new(),
        };
        let (extra, base) = super::scan_only(&found, scan);
        let left = extra
            .iter()
            .map(|row| {
                (
                    row.uri.rsplit('/').next().unwrap_or_default(),
                    row.range.start.line,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(left, [("b.ts", 25), ("c.ts", 3)]);
        assert_eq!(base, "Base");
    }

    /// Every project-marker family a workspace-root search names takes its
    /// language id from the grammar registry.
    #[test]
    fn root_marker_families_are_named_by_language_id() {
        let named = [".ts", ".rs", ".go", ".py"].map(language_name);
        assert_eq!(named, ["typescript", "rust", "go", "python"]);
        assert_eq!(language_name(".unknown-ext"), "unknown");
    }

    /// A heritage answer missing files it could not read says which,
    /// stays partial for them, and leads to a text search over exactly them.
    #[test]
    fn a_scan_answer_names_its_unread_files_and_leads_to_them() {
        let query: super::LspSearchQuery = serde_json::from_value(serde_json::json!({
            "path": "/r/a.ts", "symbolName": "Base", "line": 1, "operation": "subtypes"
        }))
        .expect("query");
        let scope = super::Scope::new("/r".into(), Vec::new());
        let paths = crate::tools::test_support::workspace_policy(std::path::Path::new("/r"));
        let unread = vec!["/r/big.ts".to_owned()];
        let mut row = serde_json::json!({});
        super::mark_scan_answer(
            &mut row,
            &query,
            &scope,
            &paths,
            "scan note".into(),
            &unread,
        );
        assert_eq!(
            row["partialReasons"],
            serde_json::json!(["serverUnanswered", "heritageUnread"])
        );
        let warnings = row["warnings"].to_string();
        assert!(
            warnings.contains("big.ts") && !warnings.contains("/r/"),
            "{warnings}"
        );
        let lead = &row["next"]["findText"];
        assert_eq!(lead["tool"], "localSearch", "{row}");
        // The warning and the lead name the file the same way.
        assert_eq!(lead["query"]["queries"][0]["path"], "big.ts", "{row}");

        let mut complete = serde_json::json!({});
        super::mark_scan_answer(
            &mut complete,
            &query,
            &scope,
            &paths,
            "scan note".into(),
            &[],
        );
        assert_eq!(
            complete["partialReasons"],
            serde_json::json!(["serverUnanswered"])
        );
        assert!(complete["next"]["findText"].is_null());
    }

    /// The published rows (`symbolName`, not the server's `name`) rank
    /// the exact name ahead of rust-analyzer's alphabetical fuzzy matches.
    #[test]
    fn public_workspace_symbols_rank_the_exact_name_first() {
        let raw = |name: &str| {
            serde_json::json!({"name": name, "kind": 12,
                "location": {"uri": "file:///repo/a.rs", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}}})
        };
        let mut rows = ["a_keeps_alive", "b_alive_pool", "is_alive_now", "is_alive"]
            .map(|name| super::super::locations::public_workspace_symbol(&raw(name)));
        rank_workspace_symbols(&mut rows, "is_alive");
        assert_eq!(rows[0]["symbolName"], "is_alive", "{rows:?}");
        assert_eq!(rows[1]["symbolName"], "is_alive_now", "{rows:?}");
    }

    #[test]
    fn workspace_symbols_rank_declarations_before_same_name_bindings() {
        let mut rows = [
            ("Scene", "variable"),
            ("scene", "property"),
            ("Scene", "class"),
        ];
        rows.sort_by_key(|(name, kind)| (match_tier(name, "Scene"), binding_rank(kind)));
        assert_eq!(rows[0], ("Scene", "class"));
        assert_eq!(rows[1], ("Scene", "variable"));
    }

    #[test]
    fn workspace_symbols_rank_exact_before_fuzzy() {
        let mut names = vec![
            "a_keeps_alive",
            "process_is_alive",
            "is_alive_now",
            "IS_ALIVE",
            "is_alive",
        ];
        names.sort_by_key(|name| match_tier(name, "is_alive"));
        assert_eq!(
            names,
            vec![
                "is_alive",
                "IS_ALIVE",
                "is_alive_now",
                "process_is_alive",
                "a_keeps_alive"
            ]
        );
    }
}
