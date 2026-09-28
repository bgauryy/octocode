//! Per-operation language-server requests and their row shaping, run after
//! `execute` has admitted the query, acquired a leased client, synchronized
//! the document, and resolved the anchor.

use super::LspSearchQuery;
use super::cancellable;
use super::failure::{LspFailure, empty, empty_hint};
use super::importers::{self, Importers};
use super::locations::{
    RECOVERED_ALIAS, items_payload, locations, public_range, public_workspace_symbol,
};
use super::recovery::{get_locations, recover_aliases, resolve_definition_chain, snippet_identity};
use super::render::as_array;
use super::render::uri_to_path;
use super::source::{SourceCache, filter_authorized_items};
use super::walk::hierarchy;
use crate::tools::local_fetch::CancellationCheck;
use octocode_engine::lsp::client::{LocationRequest, NativeLspClient, SnippetReadPolicy};
use octocode_engine::lsp::config::{representative_source_for, workspace_root_languages};
use serde_json::{Value, json};
use std::collections::HashSet;

/// Label for references found from a verified importer anchor.
const RECOVERED_IMPORTER: &str = "recoveredImporter";

pub(super) const PUSH_DIAGNOSTICS_WAIT_MS: u32 = 1_500;

/// Everything one operation needs, borrowed from `execute`.
pub(super) struct Operation<'a, 'p> {
    pub(super) client: &'a NativeLspClient,
    pub(super) query: &'a LspSearchQuery,
    pub(super) sources: &'a mut SourceCache<'p>,
    pub(super) snippet_policy: &'a SnippetReadPolicy,
    pub(super) cancel: &'a dyn CancellationCheck,
    /// Canonical file (or workspace-root directory) path of the request.
    pub(super) path: &'a str,
    pub(super) workspace_root: &'a str,
    /// A `workspaceRoot`-only request (no source file).
    pub(super) root_only: bool,
    /// Zero-based LSP anchor (0,0 for document-wide operations).
    pub(super) line: u32,
    pub(super) character: u32,
    /// Language of the anchor file, for server-specific recovery.
    pub(super) language_id: Option<&'a str>,
}

impl Operation<'_, '_> {
    pub(super) async fn run(mut self) -> Result<Value, LspFailure> {
        let query = self.query;
        match query.operation().as_str() {
            "definition" => {
                let found = resolve_definition_chain(
                    self.client,
                    self.sources,
                    self.snippet_policy,
                    self.cancel,
                    self.path,
                    self.line,
                    self.character,
                )
                .await?;
                Ok(self
                    .locations("definition", "definitionProvider", found)
                    .await)
            }
            "references" => {
                let include_declaration = query.include_declaration().unwrap_or(true);
                let mut found = self
                    .location_request(LocationRequest::References {
                        include_declaration,
                    })
                    .await?;
                let recovered = recover_aliases(
                    self.client,
                    self.sources,
                    self.snippet_policy,
                    self.cancel,
                    query.symbol_name(),
                    include_declaration,
                    self.path,
                    self.line,
                    self.character,
                    &found,
                )
                .await?;
                let mut seen = found
                    .iter()
                    .chain(&recovered)
                    .map(snippet_identity)
                    .collect::<HashSet<_>>();
                let known_files = found
                    .iter()
                    .chain(&recovered)
                    .map(|snippet| canonical_path(&uri_to_path(&snippet.uri)))
                    .collect::<HashSet<_>>();
                let importers = self.importers(&known_files).await?;
                let mut from_importers = Vec::new();
                if let Some(importers) = &importers {
                    for anchor in importers.per_file() {
                        let Ok(extra) = get_locations(
                            self.client,
                            self.snippet_policy,
                            self.cancel,
                            LocationRequest::References {
                                include_declaration,
                            },
                            &anchor.path,
                            anchor.line,
                            anchor.character,
                        )
                        .await
                        else {
                            self.cancel.check().map_err(LspFailure::cancelled)?;
                            continue;
                        };
                        from_importers.extend(
                            extra
                                .into_iter()
                                .filter(|snippet| seen.insert(snippet_identity(snippet))),
                        );
                    }
                }
                // Recovered references are not reported from the anchor:
                // label each so callers can weigh them as such.
                let labelled = |label: &'static str| {
                    move |snippet| {
                        let mut value = serde_json::to_value(snippet).unwrap_or(Value::Null);
                        value["source"] = json!(label);
                        value
                    }
                };
                let found = found
                    .drain(..)
                    .map(|snippet| serde_json::to_value(snippet).unwrap_or(Value::Null))
                    .chain(recovered.into_iter().map(labelled(RECOVERED_ALIAS)))
                    .chain(from_importers.into_iter().map(labelled(RECOVERED_IMPORTER)))
                    .collect::<Vec<_>>();
                let mut row = locations(
                    self.query,
                    self.sources,
                    "references",
                    "referencesProvider",
                    found,
                )
                .await;
                if let Some(importers) = &importers {
                    importers.annotate(&mut row);
                }
                Ok(row)
            }
            "typeDefinition" => {
                let found = self
                    .location_request(LocationRequest::TypeDefinition)
                    .await?;
                Ok(self
                    .locations("typeDefinition", "typeDefinitionProvider", found)
                    .await)
            }
            "implementation" => {
                let found = self
                    .location_request(LocationRequest::Implementation)
                    .await?;
                Ok(self
                    .locations("implementation", "implementationProvider", found)
                    .await)
            }
            "hover" => {
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
                        "type": query.operation(),
                        "uri": query.uri(),
                        "lsp": { "serverAvailable": true, "provider": "hoverProvider" },
                        "payload": { "kind": "hover", "hover": hover }
                    }),
                })
            }
            "documentSymbols" => {
                let symbols = cancellable(
                    self.cancel,
                    self.client.get_document_symbols(self.path.to_owned()),
                )
                .await??;
                Ok(items_payload(query, "documentSymbols", symbols))
            }
            "workspaceSymbol" => self.workspace_symbol().await,
            "diagnostic" => self.diagnostic().await,
            "callers" | "callees" | "callHierarchy" | "supertypes" | "subtypes" => {
                let importers = self.importers(&HashSet::new()).await?;
                let extra_roots = importers
                    .as_ref()
                    .map(|importers| {
                        importers
                            .call_sites()
                            .into_iter()
                            .map(|anchor| (anchor.path.clone(), anchor.line, anchor.character))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let derived_callers = match &importers {
                    Some(importers) if query.operation() != "callees" => {
                        importers::callers_from_references(
                            self.client,
                            self.sources,
                            self.snippet_policy,
                            self.cancel,
                            importers,
                        )
                        .await?
                    }
                    _ => Vec::new(),
                };
                let mut row = hierarchy(
                    self.client,
                    query,
                    self.sources.policy(),
                    self.path,
                    self.line,
                    self.character,
                    &extra_roots,
                    derived_callers,
                    self.cancel,
                )
                .await?;
                if let Some(importers) = &importers {
                    importers.annotate(&mut row);
                }
                Ok(row)
            }
            other => Ok(empty(
                query,
                "unsupportedOperation",
                &format!("Unsupported lspSearch operation: {other}"),
                true,
            )),
        }
    }

    /// Verified importer anchors when the server may have missed importers
    /// (TS/JS incoming operations); `None` when recovery does not apply.
    async fn importers(
        &mut self,
        known_files: &HashSet<String>,
    ) -> Result<Option<Importers>, LspFailure> {
        let operation = self.query.operation();
        if !importers::applies(self.language_id, &operation) || self.root_only {
            return Ok(None);
        }
        let symbol =
            match self.query.symbol_name() {
                Some(name) if !name.trim().is_empty() => Some(name.to_owned()),
                _ => self.sources.get(self.path).await.and_then(|source| {
                    importers::word_at(&source.content, self.line, self.character)
                }),
            };
        let Some(symbol) = symbol else {
            return Ok(None);
        };
        importers::verified_anchors(
            self.client,
            self.sources,
            self.snippet_policy,
            self.cancel,
            &symbol,
            self.workspace_root,
            self.path,
            self.line,
            self.character,
            known_files,
        )
        .await
        .map(Some)
    }

    async fn location_request(
        &self,
        request: LocationRequest,
    ) -> Result<Vec<octocode_engine::lsp::types::JsCodeSnippet>, LspFailure> {
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
        found: Vec<octocode_engine::lsp::types::JsCodeSnippet>,
    ) -> Value {
        locations(self.query, self.sources, kind, provider, found).await
    }

    async fn workspace_symbol(self) -> Result<Value, LspFailure> {
        let query = self.query;
        let name = query
            .symbol_name()
            .map(str::to_owned)
            .ok_or_else(|| LspFailure::invalid_query("workspaceSymbol requires symbolName"))?;
        let symbols =
            cancellable(self.cancel, self.client.workspace_symbol(name.clone())).await??;
        // workspace/symbol URIs are server-controlled and span the whole
        // project; drop any that fall outside the read policy before emitting.
        let mut symbols = as_array(&filter_authorized_items(symbols, self.sources.policy()))
            .iter()
            .map(public_workspace_symbol)
            .collect::<Vec<_>>();
        // Servers return fuzzy matches in their own order (rust-analyzer:
        // alphabetical), which can bury the exact name past the first page.
        // Stable sort keeps server order within each tier.
        symbols.sort_by_key(|symbol| {
            match_tier(
                symbol.get("name").and_then(Value::as_str).unwrap_or(""),
                &name,
            )
        });
        let mut row = items_payload(query, "symbols", json!(symbols));
        if self.root_only && row.pointer("/payload/kind").and_then(Value::as_str) == Some("empty") {
            let languages = workspace_root_languages(self.path);
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
                row["next"][format!("search{}", capitalized(language))] = json!({
                    "tool": "lspSearch",
                    "confidence": "medium",
                    "query": {
                        "operation": "workspaceSymbol",
                        "symbolName": name,
                        "uri": file,
                        "goal": query.goal(),
                        "reasoning": format!("Search the {language} project that shares this workspace root.")
                    }
                });
            }
            let hint = if others.is_empty() {
                format!(
                    "workspaceRoot-only search covered the {searched} project of one representative file; pass uri for a source file in the project that should contain the symbol."
                )
            } else {
                format!(
                    "workspaceRoot-only search used the {searched} language server; this root also holds {} projects it did not search. Follow next.* to search them.",
                    others.join(", ")
                )
            };
            row["hints"] = json!([hint]);
        }
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
        let mut row = items_payload(query, "diagnostics", items);
        if !published && row.pointer("/payload/kind").and_then(Value::as_str) == Some("empty") {
            row["payload"]["category"] = json!("diagnosticsNotPublished");
            row["payload"]["reason"] = json!(format!(
                "The language server published no diagnostics for this document within {PUSH_DIAGNOSTICS_WAIT_MS} ms."
            ));
            row["hints"] = json!([empty_hint("diagnosticsNotPublished")]);
        }
        if truncated {
            row["isPartial"] = json!(true);
            row["terminalLimit"] = json!(true);
            row["partialReasons"] = json!(["diagnosticsTruncated"]);
        }
        // Success rows drop `hints` (response hint policy), so this note is a
        // warning: it explains results, it is not a recovery step.
        if disabled_macro_diagnostics(&row["payload"]["items"]) {
            let note = json!(RUST_MACRO_NOTE);
            match row["warnings"].as_array_mut() {
                Some(warnings) => warnings.push(note),
                None => row["warnings"] = json!([note]),
            }
        }
        Ok(row)
    }
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
    as_array(items).iter().any(|item| {
        item.get("code")
            .and_then(Value::as_str)
            .is_some_and(|code| DISABLED_MACRO_CODES.contains(&code))
    })
}

/// Replace the zero-based LSP `range` on a diagnostic (and on each related
/// location) with the one-based public `displayRange` every other result uses.
fn publish_diagnostic_ranges(mut item: Value) -> Value {
    fn swap(object: &mut Value) {
        if let Some(display) = object.get("range").and_then(public_range)
            && let Some(map) = object.as_object_mut()
        {
            map.remove("range");
            map.insert("displayRange".into(), display);
        }
    }
    swap(&mut item);
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

fn language_name(extension: &str) -> &'static str {
    match extension {
        ".rs" => "rust",
        ".go" => "go",
        ".py" => "python",
        _ => "typescript",
    }
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn canonical_path(path: &str) -> String {
    std::fs::canonicalize(path)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::match_tier;

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
