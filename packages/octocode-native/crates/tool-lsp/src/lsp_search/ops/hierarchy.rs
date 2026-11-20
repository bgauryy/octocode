//! Type hierarchy and implementations, with the heritage scan when the
//! server has no hierarchy support.
use super::*;

/// The label of a subtype answer that a server's empty
/// `typeHierarchy/subtypes` left to its implementation answer.
const SUBTYPES_FROM_IMPLEMENTATION: &str = "The server's type hierarchy listed no subtypes; these rows come from its implementation answer for the type, each checked against the heritage clause it names.";

impl Operation<'_, '_> {
    /// `supertypes`/`subtypes` on a server with no type hierarchy: the
    /// resolved immediate heritage edges in the shared bounded walker.
    /// Unverified syntax substitutes remain partial and name their source.
    pub(super) async fn type_hierarchy_without_server_support(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let kind = lsp_search_query::operation(query);
        let expansion = if kind == "subtypes" {
            super::super::walk::Expansion::Subtypes
        } else {
            super::super::walk::Expansion::Supertypes
        };
        let mut candidates = if kind == "subtypes" {
            // The scan's declarations the server left out (an impl inside a
            // function body, say) are candidates like the server's own: the
            // walk checks each against the heritage clause it names.
            match self
                .implemented(
                    Some(&heritage::ScanAnchor::Declared(
                        heritage::Scannable::Subtypes,
                    )),
                    true,
                )
                .await?
            {
                Implemented::Server { mut found, scan } => {
                    if let Some(scan) = scan {
                        let (extra, _) = scan_only(&found, scan);
                        found.extend(extra);
                    }
                    let Some(window) =
                        super::super::fallback_hierarchy::CandidateWindow::cut(query, found)
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
        let source = super::super::fallback_hierarchy::Fallback::new(
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
                lsp_search_query::depth(query).unwrap_or(1),
                candidates
                    .as_mut()
                    .map(|window| std::mem::take(&mut window.items)),
            )
            .await
        {
            Ok(walk) => walk,
            Err(failure) if failure.is_cancelled() => return Err(failure),
            // Nothing was established: a typed failure, never an empty
            // answer an agent could read as absence.
            Err(failure) => return Err(failure),
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
            .map(|edge| super::super::walk::public_edge(expansion, edge, None))
            .collect::<Vec<_>>();
        // Bases named outside the read policy are the answer: a walk that
        // found only those resolved them, it did not come back empty.
        let mut row = if items.is_empty() && !walk.withheld_names.is_empty() {
            json!({
                "operation": lsp_search_query::operation(query),
                "matches": [],
                "lsp": {"serverAvailable": true},
                "pagination": octocode_runtime_base::response::pages::PageFacts::counted(0, 0, 0).to_value()
            })
        } else {
            salted_items_payload(
                query,
                kind,
                json!(items),
                candidates.as_ref().map(|window| window.digest.as_str()),
                self.budget,
                super::super::render::plain_sizes,
            )
        };
        if row.get("errorCode").is_some() {
            return Ok(row);
        }
        row["lsp"]["provider"] = json!(heritage::fallback_provider(kind));
        row["lsp"]["source"] = json!(if kind == "subtypes" {
            "implementation"
        } else {
            "heritage"
        });
        super::super::walk::mark_partial_expansion(
            &mut row,
            query,
            &walk.failures,
            self.sources.policy(),
        );
        super::super::walk::mark_truncation(
            &mut row,
            query,
            &[(expansion, &walk)],
            self.sources.policy(),
        );
        if let Some(window) = &candidates
            && window.has_more
        {
            mark_more_candidates(&mut row, query, &window.digest);
        }
        Ok(row)
    }

    /// `implementation`: the server's answer, or, for a type the server
    /// cannot answer in time, the labelled heritage scan standing in for
    /// it. Beside a server answer for a type, the scan's declarations the
    /// server left out (a function-local impl, say) follow its rows, each
    /// labelled; beside one for a member of a type (a trait method), the
    /// members of that name its subtypes declare do. An anchor that uses
    /// the name (an `impl Trait for X` header) scans what its declaration
    /// declares, so every anchor of one declaration gets one answer. A
    /// member's scan never stands in for the server, and another anchor (a
    /// function) has no heritage clause a scan could read: their failure
    /// stays the failure.
    pub(super) async fn implementation(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let content = self
            .sources
            .get(self.path)
            .await
            .map(|source| source.content.clone())
            .unwrap_or_default();
        let anchor = lsp_search_query::symbol_name(query)
            .and_then(|name| heritage::scan_anchor(&content, self.path, self.line, name));
        let (found, scanned, extra) =
            match self.implemented(anchor.as_ref(), anchor.is_some()).await? {
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
            .chain(extra_rows.iter().map(scan_row))
            .collect::<Vec<_>>();
        let mut row = self
            .locations("implementation", "implementationProvider", rows)
            .await;
        if let Some((note, unread)) = scanned {
            row["lsp"]["source"] = json!(SCAN_SOURCE);
            mark_scan_answer(&mut row, query, scope, paths, note, &unread);
        }
        if let Some((rows, _)) = &extra {
            flag_scan_only(&mut row, rows);
        }
        Ok(row)
    }

    /// The server's `textDocument/implementation` answer. The heritage scan
    /// (where `anchor` has one; a use first resolves its declaration) runs
    /// while the request is pending. Where it can stand in for the server
    /// ([`heritage::Scannable::stands_in`]), a failure that has the scan as
    /// fallback takes it without a second wait, and a server still silent a
    /// grace after the scan finishes yields to it. `keep` also waits for
    /// the scan beside a server answer.
    pub(super) async fn implemented(
        &self,
        anchor: Option<&heritage::ScanAnchor>,
        keep: bool,
    ) -> Result<Implemented, LspFailure> {
        let may_stand_in = anchor.is_some_and(heritage::ScanAnchor::may_stand_in);
        // The scan, with whether it may stand in for the server.
        let scan = async {
            let resolved;
            let (what, declaration) = match anchor {
                None => return Ok(None),
                Some(heritage::ScanAnchor::Declared(what)) => (what, self.declaration()),
                Some(heritage::ScanAnchor::Use) => match self.resolved_declaration().await? {
                    Some((what, declaration)) => {
                        resolved = what;
                        (&resolved, declaration)
                    }
                    None => return Ok(None),
                },
            };
            Ok::<_, LspFailure>(
                self.heritage_scan(what, &declaration)
                    .await?
                    .map(|scan| (what.stands_in(), scan)),
            )
        };
        let raced = heritage::hedged(
            self.location_request(LocationRequest::Implementation),
            scan,
            |failure| may_stand_in && heritage::implementation_failure_has_fallback(failure),
            |scan| matches!(scan, Ok(Some((true, _)))).then_some(heritage::SERVER_GRACE_AFTER_SCAN),
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
                    Some(Ok(scan)) => scan.map(|(_, scan)| scan),
                    _ => None,
                },
            }),
            Err((failure, Some(scan))) => match scan? {
                Some((true, scan)) => Ok(Implemented::Scan {
                    note: heritage::describe_scan(
                        &failure,
                        &scan.base,
                        scan.files,
                        lsp_search_query::operation(self.query),
                    ),
                    found: scan.found,
                    unread: scan.unread,
                }),
                _ => Err(failure),
            },
            Err((failure, None)) => Err(failure),
        }
    }

    /// What the declaration the anchor resolves to (the server's definition
    /// answer) lets a heritage scan read, with that declaration
    /// ([`heritage::resolved`]).
    /// A definition request that fails leaves no scan; only cancellation
    /// stops the request.
    async fn resolved_declaration(
        &self,
    ) -> Result<Option<(heritage::Scannable, heritage::Declaration)>, LspFailure> {
        let Some(name) = lsp_search_query::symbol_name(self.query) else {
            return Ok(None);
        };
        let definitions = match self.location_request(LocationRequest::Definition).await {
            Ok(found) => found,
            Err(failure) if failure.is_cancelled() => return Err(failure),
            Err(_) => return Ok(None),
        };
        heritage::resolved(&definitions, name, self.sources.policy(), self.cancel).await
    }

    /// The anchor as the declaration a heritage scan reads.
    fn declaration(&self) -> heritage::Declaration {
        heritage::Declaration {
            path: self.path.to_owned(),
            line: self.line,
            character: self.character,
        }
    }

    /// What the heritage clauses of the workspace files that spell the
    /// queried name say of its `declaration` ([`heritage::scan_for`]): the
    /// syntax answer beside, or standing by for, the implementation
    /// request. `None` when no name is queried or the text scan of the
    /// workspace could not run.
    pub(super) async fn heritage_scan(
        &self,
        what: &heritage::Scannable,
        declaration: &heritage::Declaration,
    ) -> Result<Option<heritage::HeritageScan>, LspFailure> {
        heritage::scan_for(
            what,
            declaration,
            self.query,
            self.scope,
            self.sources.policy(),
            self.cancel,
        )
        .await
    }

    /// A call or type hierarchy walk; TS/JS incoming walks also verify the
    /// importers the server's level-1 answer does not cover.
    pub(super) async fn hierarchy(self) -> Result<Value, LspFailure> {
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
                proof: super::importers::SiteProof::of(self.language_id),
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
        // A server can prepare the type yet list none of its subtypes while
        // its implementation answer names them (jdtls on an interface): an
        // empty subtype answer is checked against the implementation path
        // before it stands as absence.
        if lsp_search_query::op(query) == Op::Subtypes
            && row.get("category").and_then(Value::as_str) == Some("noLocations")
        {
            match self.type_hierarchy_without_server_support().await {
                Ok(mut found)
                    if found.get("errorCode").is_none()
                        && found.get("category").and_then(Value::as_str) != Some("noLocations") =>
                {
                    octocode_runtime_base::response::paging::push_warnings(
                        &mut found,
                        [SUBTYPES_FROM_IMPLEMENTATION.to_owned()],
                    );
                    return Ok(found);
                }
                Err(failure) if failure.is_cancelled() => return Err(failure),
                _ => return Ok(row),
            }
        }
        if let Some(importers) = &importers {
            self.check_code_uses().await?;
            importers.annotate(&mut row, query, self.scope);
            // A deeper walk follows the last importer window, not a page
            // that still has windows to verify.
            if row.pointer("/next/nextImporterWindow").is_some()
                && let Some(next) = row.get_mut("next").and_then(Value::as_object_mut)
            {
                next.remove("walkDeeper");
            }
        }
        Ok(row)
    }
}
