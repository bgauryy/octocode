//! The twelve `lspSearch` operations asked of the editor's providers, and
//! their answers shaped by the same payload builders a server answer uses,
//! so snapshots, pagination and `responsePagination` are the same.
use super::client::Bridge;
use super::{Fault, hierarchy, snippets};
use crate::execution::cancel::CancellationCheck;
use crate::tools::lsp_search::failure::no_answer;
use crate::tools::lsp_search::heritage;
use crate::tools::lsp_search::locations::{
    items_payload, public_hover, public_workspace_symbol, salted_locations,
};
use crate::tools::lsp_search::ops::{dedupe_diagnostics, diagnostic_items};
use crate::tools::lsp_search::ops::{flag_scan_only, scan_only, scan_row};
use crate::tools::lsp_search::render::items;
use crate::tools::lsp_search::source::{SourceCache, filter_authorized_items};
use crate::tools::lsp_search::{LspSearchQuery, Op, cancellable, lsp_search_query};
use serde_json::{Value, json};

/// One page's question: the anchor, what answers it, and what a page may spend.
pub(super) struct Question<'a, 'p> {
    pub(super) bridge: &'a Bridge,
    pub(super) query: &'a LspSearchQuery,
    pub(super) sources: &'a mut SourceCache<'p>,
    pub(super) cancel: &'a dyn CancellationCheck,
    /// Where lexical scans search; the heritage scan reads its files.
    pub(super) scope: &'a crate::tools::lsp_search::scope::Scope,
    /// Canonical file (or workspace-root directory) of the request.
    pub(super) path: &'a str,
    /// The request names a workspace root, not a file: `path` is a directory.
    pub(super) root_only: bool,
    /// Zero-based LSP anchor (0,0 for document-wide operations).
    pub(super) line: u32,
    pub(super) character: u32,
    pub(super) budget: usize,
    pub(super) security: &'a crate::security::ContentSecurity,
}

impl Question<'_, '_> {
    /// The bridge's answer to `method`, bounded by the request's
    /// cancellation and deadline.
    async fn ask(&self, method: &str, params: Value) -> Result<Value, Fault> {
        Ok(cancellable(self.cancel, self.bridge.ask(method, params)).await??)
    }

    /// The anchor as the bridge takes it.
    pub(super) fn anchor(&self) -> Value {
        json!({ "path": self.path, "line": self.line, "character": self.character })
    }

    pub(super) async fn run(mut self) -> Result<Value, Fault> {
        let query = self.query;
        match lsp_search_query::op(query) {
            Op::Definition => self.locations("definition", "definition").await,
            Op::TypeDefinition => self.locations("typeDefinition", "typeDefinition").await,
            Op::Implementation => self.implementation().await,
            Op::References => self.references().await,
            Op::Hover => {
                let hover = self.ask("hover", self.anchor()).await?;
                Ok(if hover.is_null() {
                    no_answer(
                        query,
                        "noHover",
                        "hoverProvider returned no hover at this position",
                    )
                } else {
                    json!({
                        "lsp": { "serverAvailable": true, "provider": "hoverProvider" },
                        "operation": "hover",
                        "hover": public_hover(hover)
                    })
                })
            }
            Op::DocumentSymbols => {
                let mut symbols = self
                    .ask("documentSymbols", json!({ "path": self.path }))
                    .await?;
                let content = self
                    .sources
                    .get(self.path)
                    .await
                    .map(|source| source.content.clone());
                crate::tools::lsp_search::render::name_type_aliases_in(
                    &mut symbols,
                    octocode_engine::lsp::config::detect_language_id(self.path).as_deref(),
                    content.as_deref(),
                );
                Ok(items_payload(
                    query,
                    "documentSymbols",
                    symbols,
                    self.budget,
                ))
            }
            Op::Diagnostic => {
                let report = self
                    .ask("diagnostics", json!({ "path": self.path }))
                    .await?;
                let (found, _) = diagnostic_items(Some(report));
                Ok(items_payload(
                    query,
                    "diagnostics",
                    dedupe_diagnostics(found),
                    self.budget,
                ))
            }
            Op::WorkspaceSymbol => self.workspace_symbol().await,
            Op::Callers | Op::Callees | Op::Supertypes | Op::Subtypes => {
                hierarchy::run(&self).await
            }
        }
    }

    /// A location answer (`definition`, `typeDefinition`, `implementation`).
    async fn locations(&mut self, kind: &str, method: &str) -> Result<Value, Fault> {
        let answer = self.ask(method, self.anchor()).await?;
        Ok(self.page(kind, answer).await)
    }

    /// `implementation`: the editor's answer, with the heritage scan's
    /// declarations it left out for a type anchor, as a server answer has.
    async fn implementation(&mut self) -> Result<Value, Fault> {
        let answer = self.ask("implementation", self.anchor()).await?;
        let found = snippets::snippets_of(&answer, self.sources).await;
        let content = self
            .sources
            .get(self.path)
            .await
            .map(|source| source.content.clone())
            .unwrap_or_default();
        let scannable = lsp_search_query::symbol_name(self.query).is_some_and(|name| {
            heritage::anchors_type_declaration(&content, self.path, self.line, name)
        });
        let scan = if scannable {
            // A scan that cannot run adds nothing; only cancellation stops.
            match heritage::scan_workspace(
                self.query,
                self.scope,
                self.sources.policy(),
                self.cancel,
            )
            .await
            {
                Err(failure) if failure.is_cancelled() => return Err(failure.into()),
                Ok(scan) => scan,
                Err(_) => None,
            }
        } else {
            None
        };
        let extra = scan
            .map(|scan| scan_only(&found, scan).0)
            .unwrap_or_default();
        let rows = found
            .iter()
            .map(|snippet| serde_json::to_value(snippet).unwrap_or(Value::Null))
            .chain(extra.iter().map(scan_row))
            .collect();
        let mut row = self.page_of("implementation", rows).await;
        flag_scan_only(&mut row, &extra);
        Ok(row)
    }

    async fn references(&mut self) -> Result<Value, Fault> {
        let answer = self.ask("references", self.anchor()).await?;
        let mut found = snippets::from_locations(&answer, self.sources).await;
        crate::tools::lsp_search::roles::assign_roles(&mut found, self.sources, self.cancel)
            .await?;
        Ok(self.page_of("references", found).await)
    }

    async fn page(&mut self, kind: &str, answer: Value) -> Value {
        let found = snippets::from_locations(&answer, self.sources).await;
        self.page_of(kind, found).await
    }

    async fn page_of(&mut self, kind: &str, found: Vec<Value>) -> Value {
        let provider =
            crate::tools::lsp_search::receipt::provider_for_operation(kind).unwrap_or_default();
        salted_locations(
            self.query,
            self.sources,
            kind,
            provider,
            found,
            None,
            self.budget,
            self.security,
        )
        .await
    }

    async fn workspace_symbol(&mut self) -> Result<Value, Fault> {
        let query = self.query;
        let name = lsp_search_query::symbol_query(query)
            .map(str::to_owned)
            .ok_or_else(|| {
                crate::tools::lsp_search::LspFailure::invalid_query(
                    "workspaceSymbol requires symbolQuery",
                )
            })?;
        // A workspace root is no document: with no `path` the editor opens
        // none. A file anchor is sent for the editor's own project choice.
        let mut params = json!({ "query": name });
        if !self.root_only {
            params["path"] = json!(self.path);
        }
        let answer = self.ask("workspaceSymbol", params).await?;
        let symbols = items(&filter_authorized_items(answer, self.sources.policy()))
            .iter()
            .map(public_workspace_symbol)
            .collect::<Vec<_>>();
        Ok(items_payload(query, "symbols", json!(symbols), self.budget))
    }
}
