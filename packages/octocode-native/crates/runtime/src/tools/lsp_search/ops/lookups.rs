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
    pub(super) async fn definition(self) -> Result<Value, LspFailure> {
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
        let mut row = if found.is_empty() && withheld > 0 {
            super::super::locations::withheld(query, withheld)
        } else {
            self.locations("definition", "definitionProvider", found)
                .await
        };
        if !warnings.is_empty() {
            crate::response::paging::mark_partial(&mut row, "definitionHopFailed", &warnings);
        }
        Ok(row)
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
            self.response_window,
        )
        .await
    }

    pub(super) async fn workspace_symbol(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let name = lsp_search_query::symbol_query(query)
            .map(str::to_owned)
            .ok_or_else(|| LspFailure::invalid_query("workspaceSymbol requires symbolQuery"))?;
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
                crate::response::result::attach_related(
                    &mut row,
                    crate::response::result::Continuation::related(
                        ToolId::LspSearch,
                        json!({
                            "operation": "workspaceSymbol",
                            "symbolQuery": name,
                            "path": file
                        }),
                    )
                    .build(),
                );
            }
            let hint = if others.is_empty() {
                match super::super::session::root_representative(self.path, &self.scope.root) {
                    Some(file) => format!(
                        "workspaceRoot-only search: the {searched} server searched the project of {}; for another project, pass path there.",
                        std::path::Path::new(&file)
                            .strip_prefix(&self.scope.root)
                            .map_or(file.as_str().into(), std::path::Path::to_string_lossy)
                    ),
                    None => format!(
                        "workspaceRoot-only search found no {searched} source file to load a project; pass path for a file in the symbol's."
                    ),
                }
            } else {
                format!(
                    "workspaceRoot-only search used the {searched} server; this root also holds {} unsearched projects.",
                    others.join(", ")
                )
            };
            row["hints"] = json!([hint]);
        }
        super::super::server_coverage::workspace_symbols(
            &mut row,
            &name,
            self.scope,
            document_anchor.as_deref(),
            capped_at,
        );
        Ok(row)
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
        let (items, truncated) = diagnostic_items(report);
        let mut row = items_payload(query, "diagnostics", dedupe_diagnostics(items), self.budget);
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
