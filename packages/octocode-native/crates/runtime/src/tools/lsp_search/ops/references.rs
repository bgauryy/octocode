//! References: server answers, importer recovery and declarations.
use super::*;

impl Operation<'_, '_> {
    /// References from the anchor, plus alias and verified-importer
    /// recoveries, each recovered row labelled with its source. Importer
    /// page 1 lists the anchor's answer and the first importer window; a
    /// later page (`importerPage` > 1) lists only its window's recovered
    /// rows. A recovered row in a candidate file belongs to that file's
    /// window, so each row is listed once across importer pages.
    pub(super) async fn references(mut self) -> Result<Value, LspFailure> {
        let query = self.query;
        let include_declaration = lsp_search_query::include_declaration(query).unwrap_or(true);
        let (mut found, respelled, withheld) = get_spelled_locations(
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
        let anchor_file = super::super::scope::canonical_uri(self.path);
        self.scope.answer_anchor_lines(
            self.path,
            self.line,
            found
                .iter()
                .chain(&recovered)
                .chain(&from_importers)
                .filter(|snippet| super::super::scope::canonical_uri(&snippet.uri) == anchor_file)
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
        if !later_window {
            disclose_withheld(&mut row, withheld);
        }
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
            super::super::recovery::disclose_alias_cap(&mut row, query, self.scope);
        }
        Ok(row)
    }

    /// The references the server reports from each verified importer's
    /// anchor, in the files that importer window owns, not already `seen`.
    /// An importer whose request fails is recorded in `failures`;
    /// cancellation ends the request. A listed site whose name binds another
    /// declaration (a candidate's own same-named binding, which the server
    /// may list as a contextually typed property) is left out.
    ///
    /// Each importer is asked with its declarations included: from an
    /// importer, `includeDeclaration` would drop its own import binding (a
    /// reference to the anchor's symbol) and keep the symbol's declaration.
    /// The `declared` sites (the anchor's declaration when the query leaves
    /// declarations out) are dropped here instead.
    pub(super) async fn importer_references(
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
                let site = site_start(snippet);
                importers.owns(&site.0)
                    && declared.is_none_or(|declared| !declared.contains(&site))
                    && !importers.binds_elsewhere(&site)
                    && seen.insert(snippet_identity(snippet))
            }));
        }
        Ok(found)
    }

    /// Where the anchor's symbol is declared (`definition` from the
    /// anchor), by canonical file and start. The same server just answered
    /// `references` from this anchor, so a failure fails the request: a
    /// declaration left in would read as a reference.
    pub(super) async fn declarations(&self) -> Result<HashSet<(String, u32, u32)>, LspFailure> {
        let found = self.location_request(LocationRequest::Definition).await?;
        Ok(found.iter().map(site_start).collect())
    }

    /// The symbol whose importers recovery verifies, when the server may
    /// have missed importers (TS/JS incoming operations). An `importerPage`
    /// past the first names a window only importer recovery has.
    pub(super) fn importer_symbol(&self) -> Result<Option<String>, LspFailure> {
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
    pub(super) async fn importers(
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
}

/// Flag an answer whose read policy withheld `withheld` of the server's
/// locations: the count is coverage, and the row is partial, not smaller.
fn disclose_withheld(row: &mut Value, withheld: usize) {
    if withheld == 0 {
        return;
    }
    row["coverage"]["withheldLocations"] = json!(withheld);
    crate::response::paging::mark_partial(
        row,
        "locationsWithheld",
        &[format!(
            "{withheld} reference(s) outside the allowed read roots were omitted; add the path to ALLOWED_PATHS to read them."
        )],
    );
}
