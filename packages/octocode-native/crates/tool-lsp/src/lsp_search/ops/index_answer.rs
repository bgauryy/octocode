//! Readying an index-backed server for one question (see `importers::index_proof`).
use super::*;

impl Operation<'_, '_> {
    /// Get a server that answers from an index ready for this question. Returns the
    /// files outside its index that were opened; one still unanswered is unproven.
    /// With no finished index, or more such files than one request opens, this
    /// question is `opened_by_text`. So is a later importer window whatever the
    /// index has done since: text began that walk, and its lead must reach the rest
    /// of the same list. A new question gets the index's answer.
    pub(super) async fn prepare_index_answer(&mut self) -> Result<HashSet<String>, LspFailure> {
        if self.root_only
            || importers::SiteProof::of(self.language_id) != importers::SiteProof::AnchorReferences
            || !matches!(
                lsp_search_query::op(self.query),
                Op::References | Op::Callers | Op::Implementation | Op::Subtypes
            )
        {
            return Ok(HashSet::new());
        }
        if lsp_search_query::importer_page(self.query) == 1
            && self.client.has_finished_index()
            && let Some(symbol) = lsp_search_query::symbol_name(self.query)
        {
            let freshened = importers::freshen_index(
                self.client,
                self.sources,
                self.snippet_policy,
                self.cancel,
                importers::IndexWindow {
                    symbol,
                    scope: self.scope,
                    anchor: self.path,
                    window: WindowRequest::of(self.query),
                },
                self.position(),
            )
            .await?;
            if let importers::Freshened::Opened(unknown) = freshened {
                return Ok(unknown);
            }
        }
        self.scope.mark_opened_by_text();
        self.index_candidates().await?;
        Ok(HashSet::new())
    }

    /// Open and index the files that spell the name, so the server can answer for
    /// them. `references` and `callers` page the rest through `importerPage`.
    async fn index_candidates(&mut self) -> Result<(), LspFailure> {
        let Some(symbol) = lsp_search_query::symbol_name(self.query) else {
            return Ok(());
        };
        importers::index_window(
            self.client,
            self.sources,
            self.cancel,
            importers::IndexWindow {
                symbol,
                scope: self.scope,
                anchor: self.path,
                window: WindowRequest::of(self.query),
            },
        )
        .await
    }
}
