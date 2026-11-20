//! Location lookups (definition, hover, locations), workspace symbols and
//! their ranking, and document diagnostics.
use super::*;

/// Relevance tier of a workspace symbol name for the query: exact, exact
/// ignoring case, prefix, substring, then any other (fuzzy) match.
pub(super) fn match_tier(candidate: &str, query: &str) -> u8 {
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
pub(super) fn language_name(extension: &str) -> &'static str {
    octocode_engine::signatures::languages::find_entry(extension.trim_start_matches('.'))
        .and_then(|entry| entry.language_id)
        .unwrap_or("unknown")
}

/// Servers return fuzzy matches in their own order (rust-analyzer:
/// alphabetical), which can bury the exact name past the first page. Public
/// rows are ranked by `symbolName` tier; within a tier, declarations come
/// before variable bindings (tsserver lists every `import { Scene }` as a
/// `variable` named Scene ahead of `class Scene`). Stable sort keeps server
/// order otherwise.
pub(super) fn rank_workspace_symbols(symbols: &mut [Value], name: &str) {
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
pub(super) fn binding_rank(kind: &str) -> u8 {
    u8::from(matches!(kind, "variable" | "field" | "property"))
}

/// The canonical files a references answer already names.
pub(super) fn known_files(
    found: &[octocode_engine::lsp::types::LspCodeSnippet],
    recovered: &[octocode_engine::lsp::types::LspCodeSnippet],
) -> HashSet<String> {
    found
        .iter()
        .chain(recovered)
        .map(|snippet| super::super::scope::canonical_uri(&snippet.uri))
        .collect()
}

/// A location's canonical file and start position.
pub(super) fn site_start(
    snippet: &octocode_engine::lsp::types::LspCodeSnippet,
) -> (String, u32, u32) {
    (
        super::super::scope::canonical_uri(&snippet.uri),
        snippet.range.start.line,
        snippet.range.start.character,
    )
}

impl Operation<'_, '_> {
    pub(super) async fn definition(mut self) -> Result<Value, LspFailure> {
        let query = self.query;
        let (found, warnings, withheld) = resolve_definition_chain(
            self.client,
            self.sources,
            self.snippet_policy,
            self.cancel,
            self.path,
            self.line,
            self.character,
        )
        .await?;
        let source = self.source_definition(&found).await;
        let mut row = if found.is_empty() && withheld > 0 {
            super::super::locations::withheld(query, withheld)
        } else {
            self.locations("definition", "definitionProvider", found)
                .await
        };
        if !warnings.is_empty() {
            octocode_runtime_base::response::paging::mark_partial(
                &mut row,
                octocode_runtime_base::response::paging::PartialReason::DefinitionHopFailed,
                &warnings,
            );
        }
        if let Some(read) = source {
            octocode_runtime_base::response::result::insert_next(
                &mut row["next"],
                octocode_contract_types::LspSearchChannel::SourceDefinition,
                octocode_runtime_base::response::result::Continuation::continued_query(read),
            );
        }
        Ok(row)
    }

    /// A definition the server found in a package's build output (an
    /// `outDir` `.d.ts`; no declaration map leads back) offers the source it
    /// was compiled from: the declaration of the same name there, read whole
    /// (`next.sourceDefinition`).
    async fn source_definition(
        &mut self,
        found: &[octocode_engine::lsp::types::LspCodeSnippet],
    ) -> Option<octocode_contract_types::LocalFetchQuery> {
        use super::super::build_output::{
            BuildOutput, declaration_lines, identifier_at, is_declaration_file,
        };
        let policy = self.sources.policy();
        for snippet in found {
            let file = uri_to_path(&snippet.uri);
            let path = std::path::Path::new(&file);
            if !is_declaration_file(path) {
                continue;
            }
            let Some(source) =
                BuildOutput::containing(path, policy).and_then(|output| output.source_of(path))
            else {
                continue;
            };
            let Some(name) = self.sources.get(&file).await.and_then(|emitted| {
                identifier_at(
                    &emitted.content,
                    snippet.range.start.line,
                    snippet.range.start.character,
                )
            }) else {
                continue;
            };
            let source = source.to_string_lossy().into_owned();
            let Some(lines) = self
                .sources
                .get(&source)
                .await
                .and_then(|text| declaration_lines(&text.content, &source, &name))
            else {
                continue;
            };
            if let Some(read) = octocode_tools_shared::source_view::local_fetch_query::ranges_lead(
                &source,
                &[lines],
            ) {
                return Some(read);
            }
        }
        None
    }

    pub(super) async fn hover(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let hover = cancellable(
            self.cancel,
            self.client
                .get_hover(self.path.to_owned(), self.line, self.character),
        )
        .await??;
        Ok(match hover {
            Value::Null => no_answer(
                query,
                "noHover",
                "hoverProvider returned no hover at this position",
            ),
            hover => {
                Box::pin(self.note_declaring_files()).await?;
                json!({
                    "lsp": { "serverAvailable": true, "provider": "hoverProvider" },
                    "operation": "hover",
                    "hover": public_hover(hover)
                })
            }
        })
    }

    /// Names the files that declare the anchor and its type as files this
    /// row's hover is about ([`crate::lsp_search::server_text::is_about`]): the server computed
    /// its text from them, so their whole-source judgment decides what it
    /// shows of their values. Two location requests under the request's
    /// cancellation and deadline, their snippet reads shared with the
    /// request's. A server that cannot answer one names no file: what the
    /// hover then quotes from an unnamed file is withheld.
    async fn note_declaring_files(&self) -> Result<(), LspFailure> {
        for request in [LocationRequest::Definition, LocationRequest::TypeDefinition] {
            match self.location_request(request).await {
                Ok(found) => found.iter().for_each(|snippet| {
                    crate::lsp_search::server_text::is_about(uri_to_path(&snippet.uri));
                }),
                Err(_) => self.cancel.check().map_err(LspFailure::cancelled)?,
            }
        }
        Ok(())
    }

    pub(super) async fn location_request(
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

    pub(super) async fn locations(
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
            self.security,
        )
        .await
    }

    pub(super) async fn workspace_symbol(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let name = lsp_search_query::symbol_fuzzy(query)
            .map(str::to_owned)
            .ok_or_else(|| LspFailure::invalid_query("workspaceSymbol requires symbolFuzzy"))?;
        let symbols =
            cancellable(self.cancel, self.client.workspace_symbol(name.clone())).await??;
        // The server's own cap is judged on its raw answer, before the
        // read policy drops any rows.
        let capped_at = self
            .client
            .workspace_symbol_limit()
            .filter(|limit| symbols.as_array().map_or(0, Vec::len) == *limit)
            .map(|limit| (limit, self.client.workspace_symbol_cap_advice()));
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
        if self.root_only {
            self.state_directory_route(&mut row, &name);
        }
        super::super::server_coverage::workspace_symbols(
            &mut row,
            query,
            &name,
            self.scope,
            document_anchor.as_deref(),
            capped_at,
        );
        Ok(row)
    }

    /// What a directory-path search searched: the language its files (or
    /// markers) route it by, and each other language the directory holds,
    /// which this search did not reach, with a ready call for each. The row is
    /// partial (`workspaceSymbolOneProject`), its warning naming the file whose
    /// project was searched; an empty row also names the language, and its
    /// hint that file.
    fn state_directory_route(&self, row: &mut Value, name: &str) {
        let languages = directory_languages(self.path, &self.scope.root);
        let searched = languages
            .first()
            .map_or("unknown", |extension| language_name(extension));
        let mut others = Vec::new();
        for extension in languages.iter().skip(1) {
            let Some(file) = representative_source_for(self.path, extension) else {
                continue;
            };
            others.push(language_name(extension));
            if let Some(lead) = lsp_search_query::workspace_symbol_lead(name, &file, None) {
                octocode_runtime_base::response::result::attach_related(
                    row,
                    octocode_runtime_base::response::result::Continuation::related_query(lead)
                        .build(),
                );
            }
        }
        if !others.is_empty() {
            octocode_runtime_base::response::paging::push_warnings(
                row,
                [format!(
                    "This directory search used the {searched} server; its {} sources were not searched (related).",
                    others.join(", ")
                )],
            );
        }
        // The server answers for the one project it loads for the directory
        // (or workspace root): symbols of other projects are not in the
        // list, so an empty one is no absence.
        let project = directory_representative(self.path, &self.scope.root).map(|file| {
            std::path::Path::new(&file)
                .strip_prefix(&self.scope.root)
                .map_or(file.clone(), |relative| {
                    relative.to_string_lossy().into_owned()
                })
        });
        let note = match &project {
            Some(file) => format!("The {searched} server searched only the project of {file}."),
            None => format!(
                "The {searched} server found no {searched} source file here to load a project; it searched only what it had loaded."
            ),
        };
        octocode_runtime_base::response::paging::mark_partial(
            row,
            octocode_runtime_base::response::paging::PartialReason::WorkspaceSymbolOneProject,
            &[note],
        );
        if row.pointer("/status").and_then(Value::as_str) != Some("empty") {
            return;
        }
        row["lsp"]["language"] = json!(searched);
        let hint = match project {
            Some(file) => format!(
                "Directory search: the {searched} server searched the project of {file}; for another project, pass path there."
            ),
            None => format!(
                "Directory search found no {searched} source file to load a project; pass path for a file in the symbol's project."
            ),
        };
        octocode_runtime_base::response::result::add_hints(row, [hint]);
    }

    pub(super) async fn diagnostic(self) -> Result<Value, LspFailure> {
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
        let superseded = report
            .as_ref()
            .and_then(|report| report.get("superseded"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let items = diagnostic_items(report);
        let mut row = items_payload(query, "diagnostics", dedupe_diagnostics(items), self.budget);
        if !published {
            mark_diagnostics_unpublished(&mut row, query, PUSH_DIAGNOSTICS_WAIT_MS);
        }
        if superseded {
            octocode_runtime_base::response::paging::push_warnings(
                &mut row,
                [format!(
                    "The server published these diagnostics before the document's last change and sent no newer report while it worked or in the {PUSH_DIAGNOSTICS_WAIT_MS} ms after; a server that publishes only when its diagnostics change (typescript-language-server) has nothing newer to say."
                )],
            );
        }
        // The codes on the listed diagnostics (rust-analyzer's
        // unexpanded-macro ones too) are this answer's whole statement.
        Ok(row)
    }
}
