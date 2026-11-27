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
        let later_window = lsp_search_query::importer_page(query) > 1;
        let sent_before = self.client.documents_sent();
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
        let alias_failures = recovered.failures;
        let mut recovered = recovered.snippets;
        // Files the answer names, its declaration's included: answered
        // even when the declaration row is left out.
        let mut known_files = known_files(&found, &recovered);
        let mut importers = self.importers(&known_files).await?;
        if importers.as_ref().is_some_and(|importers| importers.stale) {
            return Ok(snapshot_changed(query));
        }
        // References must not depend on what the server had loaded: once
        // this request opened files, the anchor is asked again, so a first
        // call lists what its identical repeat would, and importer anchors in
        // files that answer then covers are the server's own.
        if let Some(importers) = importers.as_mut()
            && !later_window
            && importers::SiteProof::of(self.language_id) == importers::SiteProof::DefinitionChain
            && self.client.documents_sent() != sent_before
        {
            let (again, _, _) = octocode_engine::lsp::client::asked_again(get_spelled_locations(
                self.client,
                self.snippet_policy,
                self.cancel,
                LocationRequest::References {
                    include_declaration,
                },
                (self.path, self.line, self.character),
            ))
            .await?;
            let listed = found.iter().map(snippet_identity).collect::<HashSet<_>>();
            found.extend(
                again
                    .into_iter()
                    .filter(|snippet| !listed.contains(&snippet_identity(snippet))),
            );
            known_files = self::known_files(&found, &recovered);
            importers.anchors.retain(|anchor| {
                !known_files.contains(&super::super::scope::canonical(&anchor.path))
            });
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
        self.scope.answer_lines(
            false,
            std::iter::once((self.path, self.line)).chain(
                found
                    .iter()
                    .chain(&recovered)
                    .chain(&from_importers)
                    .map(|snippet| (snippet.uri.as_str(), snippet.range.start.line)),
            ),
        );
        keep_this_pages_answer(
            &mut found,
            importers.as_ref().filter(|_| self.scope.opened_by_text()),
            &anchor_file,
            later_window,
        );
        let recovered = if later_window { Vec::new() } else { recovered };
        let mut found = found
            .drain(..)
            .map(|snippet| serde_json::to_value(snippet).unwrap_or(Value::Null))
            .chain(recovered.into_iter().map(labelled(RECOVERED_ALIAS)))
            .chain(from_importers.into_iter().map(labelled(RECOVERED_IMPORTER)))
            .collect::<Vec<_>>();
        super::super::roles::assign_roles(&mut found, self.sources, self.cancel, Some(self.client))
            .await?;
        let mut row = salted_locations(
            self.query,
            self.sources,
            "references",
            "referencesProvider",
            found,
            importers.as_ref().and_then(Importers::digest),
            self.budget,
            self.security,
        )
        .await;
        if !later_window {
            disclose_withheld(&mut row, &withheld, self.sources.policy());
        }
        self.check_code_uses().await?;
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
        Ok(row)
    }

    /// The references the server reports from each verified importer's
    /// anchor, in the files that importer window owns, not already `seen`.
    /// An importer whose request fails is recorded in `failures`;
    /// cancellation ends the request. A listed site whose name binds another
    /// declaration (a candidate's own same-named binding, which the server
    /// may list as a contextually typed property) is left out. A server
    /// that pipelines requests is asked for several importers at once.
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
        let request = |anchor: &importers::Anchor| {
            let (path, line, character) = (anchor.path.clone(), anchor.line, anchor.character);
            async move {
                Ok(get_locations(
                    self.client,
                    self.snippet_policy,
                    self.cancel,
                    LocationRequest::References {
                        include_declaration: true,
                    },
                    &path,
                    line,
                    character,
                )
                .await)
            }
        };
        let anchors = importers.per_file().collect::<Vec<_>>();
        // A server that pipelines requests is asked for several importers
        // at once; the answers keep importer order.
        let answers = if self.client.serves_requests_concurrently() {
            importers::ordered_concurrently(
                anchors,
                importers::CONCURRENT_FILE_VERIFICATIONS,
                request,
            )
            .await?
        } else {
            let mut answers = Vec::with_capacity(anchors.len());
            for anchor in anchors {
                answers.push(request(anchor).await?);
            }
            answers
        };
        let mut found = Vec::new();
        for answer in answers {
            let extra = match answer {
                Ok(extra) => extra,
                Err(failure) => {
                    failures.record(failure)?;
                    continue;
                }
            };
            found.extend(extra.into_iter().filter(|snippet| {
                let site = site_start(snippet);
                importers.owns(&site.0)
                    && !importers.mirrors_source(&site.0)
                    && importers.lists_on_this_page(&site.0)
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
    /// have missed importers (TS/JS and C-family incoming operations). An `importerPage`
    /// past the first names a window only importer recovery has.
    pub(super) fn importer_symbol(&self) -> Result<Option<String>, LspFailure> {
        let operation = lsp_search_query::operation(self.query);
        // An index answers for itself; text recovers only without one.
        let index_answers = importers::SiteProof::of(self.language_id)
            == importers::SiteProof::AnchorReferences
            && !self.scope.opened_by_text();
        let symbol =
            (importers::applies(self.language_id, operation) && !self.root_only && !index_answers)
                .then(|| lsp_search_query::symbol_name(self.query))
                .flatten()
                .filter(|name| !name.trim().is_empty())
                .map(str::to_owned);
        if symbol.is_none() && lsp_search_query::importer_page(self.query) > 1 {
            return Err(LspFailure::invalid_query(
                "importerPage pages TypeScript/JavaScript and C-family importer recovery for references and callers; copy it from that recovery's continuation.",
            ));
        }
        Ok(symbol)
    }

    /// Verified importer anchors when the server may have missed importers
    /// (TS/JS and C-family incoming operations); `None` when recovery does not apply.
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
                proof: importers::SiteProof::of(self.language_id),
                reads_imports: octocode_engine::lsp::family::is_typescript_family(self.language_id),
            },
        )
        .await
        .map(Some)
    }
}

/// What of the anchor's answer this page lists. A later importer page leaves it
/// out (listed on importer page 1); it still deduplicates that window's rows. An
/// index-backed server's answer (`index_backed`) holds every window opened so
/// far, so each page keeps the files of its own window, and the anchor's on the
/// first page: every file is listed once.
fn keep_this_pages_answer(
    found: &mut Vec<octocode_engine::lsp::types::LspCodeSnippet>,
    index_backed: Option<&Importers>,
    anchor_file: &str,
    later_window: bool,
) {
    if let Some(importers) = index_backed {
        found.retain(|snippet| {
            let file = super::super::scope::canonical_uri(&snippet.uri);
            if file == anchor_file {
                !later_window
            } else {
                importers.lists_on_this_page(&file)
            }
        });
    } else if later_window {
        found.clear();
    }
}

/// Flag an answer whose read policy withheld `withheld` of the server's
/// locations: the count is coverage, and the row is partial, not smaller.
fn disclose_withheld(
    row: &mut Value,
    withheld: &super::super::recovery::Withheld,
    paths: &PathPolicy,
) {
    if withheld.is_empty() {
        return;
    }
    row["coverage"]["withheldLocations"] = json!(withheld.count());
    octocode_runtime_base::response::paging::mark_partial(
        row,
        octocode_runtime_base::response::paging::PartialReason::LocationsWithheld,
        &[format!(
            "{} reference(s) outside the allowed read roots were omitted{}; add the path to ALLOWED_PATHS to read them.",
            withheld.count(),
            withheld.listed(paths)
        )],
    );
}
