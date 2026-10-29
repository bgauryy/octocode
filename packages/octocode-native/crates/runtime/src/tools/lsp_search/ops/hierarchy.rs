//! Type hierarchy and implementations, with the heritage scan when the
//! server has no hierarchy support.
use super::*;

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
            match self.implemented(true, false).await? {
                Implemented::Server { found, .. } => {
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
                lsp_search_query::graph_depth(query).unwrap_or(1),
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
        let mut row = salted_items_payload(
            query,
            kind,
            json!(items),
            candidates.as_ref().map(|window| window.digest.as_str()),
            self.budget,
            super::super::render::plain_sizes,
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
        super::super::walk::mark_partial_expansion(&mut row, query, &walk.failures);
        super::super::walk::mark_truncation(&mut row, query, &[(expansion, &walk)]);
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
                    super::super::walk::MAX_HIERARCHY_NODES
                )],
            );
            if let Some(name) = lsp_search_query::symbol_name(query) {
                crate::response::result::attach_related(
                    &mut row,
                    text_search_lead(name, &self.scope.root, &self.scope.include, None).build(),
                );
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
    pub(super) async fn implementation(self) -> Result<Value, LspFailure> {
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
        if let Some((rows, _)) = &extra
            && !rows.is_empty()
            && row.get("errorCode").is_none()
        {
            // The label's meaning lives in the contract; the whole answer's
            // count is said once, on page 1.
            crate::response::paging::mark_partial(&mut row, SCAN_ONLY, &[]);
            crate::response::paging::push_notice(
                &mut row,
                format!(
                    "{} implementation(s) are source:\"{SCAN_SOURCE}\" (matched by name).",
                    rows.len()
                ),
            );
        }
        Ok(row)
    }

    /// The server's `textDocument/implementation` answer. The heritage scan
    /// (when `scannable`) runs while the request is pending: a failure that
    /// has the scan as fallback takes it without a second wait, and a server
    /// still silent a grace after the scan finishes yields to it. `keep`
    /// also waits for the scan beside a server answer.
    pub(super) async fn implemented(
        &self,
        scannable: bool,
        keep: bool,
    ) -> Result<Implemented, LspFailure> {
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
    pub(super) async fn heritage_scan(&self) -> Result<Option<heritage::HeritageScan>, LspFailure> {
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
        let Some((found, unread)) =
            super::super::blocking_cancellable(self.cancel, move |stopped| {
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
            // A deeper walk follows the last importer window, not a page
            // that still has windows to verify.
            if row.pointer("/next/nextImporterPage").is_some()
                && let Some(next) = row.get_mut("next").and_then(Value::as_object_mut)
            {
                next.remove("walkDeeper");
            }
        }
        Ok(row)
    }
}
