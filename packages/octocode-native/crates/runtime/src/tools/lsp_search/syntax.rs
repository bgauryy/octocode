//! The syntax rung of the resolution ladder ([`super::ladder::run_page`]):
//! when no language server was found or started for a file with a grammar,
//! every operation answers from tree-sitter facts (declarations, calls,
//! heritage clauses, error lines) and the scope's text scan, matched by
//! name and unverified. Rows are shaped and paged by the same payload
//! builders as server answers, then labelled `level: syntax`, `source:
//! native-graph-facts`, partial (`serverUnanswered`), with the server's
//! failure, its install hint and the text-search leads of the failed row.
//!
//! Diagnostics are the parse's error line spans (`error_lines` of the graph
//! facts: each run of lines the recovered parse skipped, whole lines from
//! column 0), one `severity: error`, `source: tree-sitter` row per span, not
//! one row per tree-sitter `ERROR` or `MISSING` node.
use super::level::{Level, install_hint};
use super::locations::{items_payload, locations, public_hover, public_workspace_symbol};
use super::scope::Scope;
use super::*;
use octocode_engine::lsp::types::LspCodeSnippet;
use std::collections::BTreeSet;

mod expansion;
mod facts;
mod hierarchy;
use facts::File;

/// Whether the syntax rung answers for `failed`: no server was found, none
/// started, or the repository's toolchain was refused. A policy refusal or
/// any other failure stays terminal.
pub(super) fn takes_over(failed: &Value) -> bool {
    failed.get("errorCode") == Some(&json!(ErrorCode::ServerUnavailable))
}

/// One request the server rung could not answer.
pub(super) struct Request<'a> {
    pub(super) query: &'a LspSearchQuery,
    pub(super) path: &'a str,
    pub(super) paths: &'a PathPolicy,
    pub(super) cancel: &'a dyn CancellationCheck,
    pub(super) budget: usize,
    pub(super) security: &'a crate::security::ContentSecurity,
    /// The request's language server discovery, which roots the anchor's project.
    pub(super) discovery: &'a octocode_engine::lsp::config::LspDiscoveryOptions,
}

/// The anchor file and the scope the syntax answer reads.
struct Site {
    file: File,
    scope: Scope,
    line: u32,
}

impl Request<'_> {
    /// The syntax row for `failed`, or `failed` itself (the text rung) when
    /// the file has no grammar or the request names a directory.
    ///
    /// This rung proves the revision a continuation pins, as the server and
    /// IDE rungs do: a later page re-derives the answer from the source, and
    /// a scope that changed since the first page answers `staleSnapshot`.
    pub(super) async fn answer(self, failed: Value) -> Result<Value, Exit> {
        // Before any read of the request's sources: a file written after it
        // may be read by this page before the write.
        let started = octocode_engine::stamp::settle_clock();
        let Some(site) = self.site().await else {
            return Err(Exit::Row(failed));
        };
        let query = self.query;
        let document =
            SourceCache::new(self.paths).insert(&site.file.path, site.file.content.clone());
        if !scope_responses(query, &site.scope, Some(&document), self.cancel).await? {
            return Ok(locations::snapshot_changed(query));
        }
        let row = self.row(&site).await?;
        let mut row = with_next(query, label(row, &failed));
        if lsp_search_query::snapshot(query).is_none()
            && continuations::declares_continuation(&mut row)?
        {
            seal_scope_revision(query, &site.scope, Some(&document), started, self.cancel).await?;
        }
        Ok(row)
    }

    async fn site(&self) -> Option<Site> {
        if Path::new(self.path).is_dir() {
            return None;
        }
        let content = read_bounded_source_async(self.path.into()).await.ok()?;
        let file = File {
            path: self.path.to_owned(),
            uri: octocode_engine::lsp::uri::path_to_uri(self.path).ok()?,
            content,
            language_id: octocode_engine::lsp::config::detect_language_id(self.path),
        };
        file.outline()?;
        let anchor =
            anchor::resolve_anchor(self.query, &file.path, &file.uri, Some(&file.content)).ok()?;
        let (workspace, _) =
            resolve_workspace(self.query, self.path, self.paths, self.discovery).ok()?;
        let scope = Scope::resolve(
            self.path,
            &workspace,
            lsp_search_query::workspace_root(self.query),
            self.paths,
            file.language_id.as_deref(),
            self.discovery,
        );
        scope.anchor_site(self.path, anchor.line, anchor.character);
        scope.asks(scope::Question::of(lsp_search_query::op(self.query)));
        Some(Site {
            file,
            scope,
            line: anchor.line,
        })
    }

    /// The files of the scope that spell `name`, the anchor file among them.
    async fn spelling(&self, site: &Site, name: &str) -> Result<BTreeSet<String>, Exit> {
        let name = site.file.canonical_name(name);
        let mut files = site
            .scope
            .text_files(name, self.paths, self.cancel)
            .await?
            .map(|files| (*files).clone())
            .unwrap_or_default();
        files.insert(site.file.path.clone());
        Ok(files)
    }

    /// `work` off the async workers, stopped with the request.
    async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&(dyn Fn() -> bool + Sync)) -> T + Send + 'static,
    ) -> Result<T, Exit> {
        blocking_cancellable(self.cancel, work)
            .await?
            .ok_or_else(|| {
                Exit::Failed(LspFailure::unestablished(
                    "The tree-sitter scan stopped before it finished.",
                ))
            })
    }

    async fn row(&self, site: &Site) -> Result<Value, Exit> {
        let query = self.query;
        let name = lsp_search_query::symbol_name(query)
            .unwrap_or_default()
            .to_owned();
        let op = lsp_search_query::op(query);
        match op {
            Op::DocumentSymbols => {
                let outline = site
                    .file
                    .outline()
                    .map_or(Value::Null, |o| facts::outline(&o));
                Ok(items_payload(
                    query,
                    "documentSymbols",
                    outline,
                    self.budget,
                ))
            }
            Op::Diagnostic => {
                let errors = site
                    .file
                    .facts()
                    .map_or(json!([]), |f| facts::syntax_errors(&f));
                let items = super::ops::diagnostic_items(Some(errors));
                Ok(items_payload(query, "diagnostics", items, self.budget))
            }
            Op::Hover => Ok(self.hover(site, &name)),
            Op::WorkspaceSymbol => {
                let wanted = lsp_search_query::symbol_fuzzy(query)
                    .unwrap_or_default()
                    .to_owned();
                let files = self.spelling(site, &wanted).await?;
                let (symbols, _) = self
                    .blocking(move |stopped| facts::symbols(&files, &wanted, stopped))
                    .await?;
                let symbols = symbols
                    .iter()
                    .map(public_workspace_symbol)
                    .collect::<Vec<_>>();
                Ok(items_payload(query, "symbols", json!(symbols), self.budget))
            }
            Op::Definition | Op::TypeDefinition | Op::References | Op::Implementation => {
                let files = self.spelling(site, &name).await?;
                let (found, unread) = self
                    .blocking(move |stopped| match op {
                        Op::References => facts::occurrences(&files, &name, stopped),
                        Op::Implementation => heritage::scan_subtypes(&files, &name, stopped),
                        _ => facts::declared(&files, &name, stopped),
                    })
                    .await?;
                let mut row = self.locations(op, found).await?;
                if !unread.is_empty() {
                    let warnings = unread.iter().map(|path| format!(
                        "Syntax scan unread source: {path}. Its code occurrences could not be established; use the related text search to inspect it."
                    )).collect::<Vec<_>>();
                    crate::response::paging::mark_partial(
                        &mut row,
                        crate::response::paging::PartialReason::ServerUnanswered,
                        &warnings,
                    );
                }
                Ok(row)
            }
            Op::Callers | Op::Callees | Op::Supertypes | Op::Subtypes => {
                self.hierarchy(site, op, &name).await
            }
        }
    }

    /// Location rows of `found`; references rows carry each site's `role`, as
    /// a server's do, so the declaration occurrence reads `declaration`, and
    /// `includeDeclaration: false` leaves those rows out, as a server does.
    async fn locations(&self, op: Op, found: Vec<LspCodeSnippet>) -> Result<Value, Exit> {
        let mut sources = SourceCache::new(self.paths);
        let provider = receipt::provider_for_operation(op.name()).unwrap_or_default();
        let mut found = found
            .into_iter()
            .map(|snippet| serde_json::to_value(snippet).unwrap_or(Value::Null))
            .collect::<Vec<_>>();
        if op == Op::References {
            super::roles::assign_roles(&mut found, &mut sources, self.cancel, None).await?;
            if lsp_search_query::include_declaration(self.query) == Some(false) {
                found.retain(|row| row["role"] != "declaration");
            }
        }
        Ok(locations(
            self.query,
            &mut sources,
            op.name(),
            provider,
            found,
            self.budget,
            self.security,
        )
        .await)
    }

    /// The anchor declaration's heading: its first line.
    fn hover(&self, site: &Site, name: &str) -> Value {
        let declaration = hierarchy::anchor_declaration(&site.file, name, site.line);
        let Some(range) = declaration
            .as_ref()
            .map(|found| found.node["range"].clone())
        else {
            return failure::no_answer(
                self.query,
                "noHover",
                "No declaration of this name holds the anchor line.",
            );
        };
        let line = range
            .pointer("/start/line")
            .and_then(Value::as_u64)
            .and_then(|line| usize::try_from(line).ok())
            .unwrap_or(0);
        let heading = site
            .file
            .content
            .lines()
            .nth(line)
            .unwrap_or_default()
            .trim();
        json!({
            "lsp": {"serverAvailable": false},
            "operation": "hover",
            "hover": public_hover(json!({
                "contents": {"kind": "plaintext", "value": heading},
                "range": range,
            })),
        })
    }
}

/// `row` labelled as the syntax rung's answer to the request `failed`
/// could not get from a server: level, source and what the server rung
/// said, partial with the start failure (and install hint) as its warning,
/// and the failed row's leads. A stale-snapshot row passes unchanged.
fn label(mut row: Value, failed: &Value) -> Value {
    let Some(fields) = row.as_object_mut() else {
        return row;
    };
    if !fields.contains_key("lsp") {
        return row;
    }
    let reason = failed
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let receipt = failed.pointer("/lsp/receipt");
    let mut lsp = json!({
        "serverAvailable": false,
        "level": Level::Syntax.name(),
        "source": "native-graph-facts",
        "fellBackFrom": {"level": Level::Server.name(), "reason": reason},
    });
    if let Some(receipt) = receipt {
        lsp["receipt"] = receipt.clone();
    }
    fields.insert("lsp".into(), lsp);
    if let Some(path) = failed.get("path") {
        fields.entry("path").or_insert_with(|| path.clone());
    }
    let mut warnings = vec![format!(
        "No language server answered ({reason}); these rows are tree-sitter declarations and name matches, unverified."
    )];
    warnings.extend(install_hint(receipt, reason));
    crate::response::paging::mark_partial(
        &mut row,
        crate::response::paging::PartialReason::ServerUnanswered,
        &warnings,
    );
    for lead in failed
        .get("related")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        crate::response::result::attach_related(&mut row, lead.clone());
    }
    row
}

#[cfg(test)]
mod tests;
