//! Resolved direct heritage edges for the shared bounded hierarchy walker.

use super::anchor::Position;
use super::cancellable;
use super::failure::LspFailure;
use super::heritage;
use super::recovery::{get_locations, resolve_definition_chain};
use super::render::uri_to_path;
use super::source::SourceCache;
use super::walk::{Expansion, HierarchySource, HierarchyWalk, MAX_HIERARCHY_NODES, NodeKeys, Walk};
use crate::execution::cancel::CancellationCheck;
use crate::tools::lsp_search::lsp_search_query;
use octocode_engine::lsp::client::{LocationRequest, NativeLspClient, SnippetReadPolicy};
use octocode_engine::lsp::types::LspCodeSnippet;
use serde_json::{Value, json};
use tokio::sync::Mutex;

/// One window of implementation candidates, bound to the whole ordered set.
pub(super) struct CandidateWindow {
    pub(super) items: Vec<LspCodeSnippet>,
    pub(super) digest: String,
    pub(super) has_more: bool,
}

impl CandidateWindow {
    pub(super) fn cut(
        query: &super::LspSearchQuery,
        mut candidates: Vec<LspCodeSnippet>,
    ) -> Option<Self> {
        let position = |item: &LspCodeSnippet| {
            (
                item.range.start.line,
                item.range.start.character,
                item.range.end.line,
                item.range.end.character,
            )
        };
        candidates.sort_by(|a, b| {
            (&a.uri, position(a), &a.content).cmp(&(&b.uri, position(b), &b.content))
        });
        candidates.dedup_by(|a, b| a.uri == b.uri && position(a) == position(b));
        let mut scope = lsp_search_query::to_row(query);
        if let Some(fields) = scope.as_object_mut() {
            for field in crate::response::result::INTENT_FIELDS.iter().chain(&[
                "page",
                "candidatePage",
                "snapshot",
            ]) {
                fields.remove(*field);
            }
        }
        let identity = octocode_engine::digest::canonicalize_dropping_nulls(json!({
            "query": scope,
            "candidates": candidates,
        }));
        let digest = format!("lsp-candidates:{}", crate::digest::json_sha256(&identity));
        let page = crate::num::usize_of(lsp_search_query::candidate_page(query));
        let start = (page - 1).saturating_mul(MAX_HIERARCHY_NODES);
        // A window's first result page carries the candidate digest. Later
        // result pages carry a result snapshot salted with this same digest.
        if page > 1
            && (start >= candidates.len()
                || (lsp_search_query::page(query).unwrap_or(1) == 1
                    && lsp_search_query::snapshot(query) != Some(digest.as_str())))
        {
            return None;
        }
        let has_more = candidates.len() > start.saturating_add(MAX_HIERARCHY_NODES);
        Some(Self {
            items: candidates
                .into_iter()
                .skip(start)
                .take(MAX_HIERARCHY_NODES)
                .collect(),
            digest,
            has_more,
        })
    }
}

pub(super) struct Fallback<'a, 'p> {
    client: &'a NativeLspClient,
    sources: Mutex<&'a mut SourceCache<'p>>,
    snippet_policy: &'a SnippetReadPolicy,
    cancel: &'a dyn CancellationCheck,
    failures: Mutex<Vec<LspFailure>>,
    keys: Mutex<NodeKeys<'p>>,
    bases: Mutex<std::collections::HashMap<String, Vec<Value>>>,
    documents: Mutex<
        std::collections::HashMap<
            String,
            std::sync::Arc<octocode_engine::graph::GraphFactsDocument>,
        >,
    >,
    initial: Mutex<Option<(Value, Vec<LspCodeSnippet>)>>,
    /// A node other than the anchor had more candidates than one window.
    candidate_limit: std::sync::atomic::AtomicBool,
}

impl<'a, 'p> Fallback<'a, 'p> {
    pub(super) fn new(
        client: &'a NativeLspClient,
        sources: &'a mut SourceCache<'p>,
        snippet_policy: &'a SnippetReadPolicy,
        cancel: &'a dyn CancellationCheck,
    ) -> Self {
        let keys = NodeKeys::new(sources.policy());
        Self {
            client,
            sources: Mutex::new(sources),
            snippet_policy,
            cancel,
            failures: Mutex::new(Vec::new()),
            keys: Mutex::new(keys),
            bases: Mutex::new(std::collections::HashMap::new()),
            documents: Mutex::new(std::collections::HashMap::new()),
            initial: Mutex::new(None),
            candidate_limit: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub(super) fn candidate_limited(&self) -> bool {
        self.candidate_limit
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// A relation the source and the server's answers did not establish.
    async fn fail(&self, message: String) {
        self.fail_from(LspFailure::unestablished(message)).await;
    }

    /// A request or parse that failed, kept with its kind: a transient one
    /// makes the row retryable.
    async fn fail_from(&self, failure: LspFailure) {
        self.failures.lock().await.push(failure);
    }

    async fn source(
        &self,
        path: &str,
    ) -> Result<std::sync::Arc<super::source::Source>, LspFailure> {
        let source = self.sources.lock().await.get(path).await.ok_or_else(|| {
            LspFailure::invalid_query(format!("Authorized type source unavailable: {path}"))
        })?;
        cancellable(
            self.cancel,
            self.client.open_document(path.to_owned(), &source.content),
        )
        .await??;
        Ok(source)
    }

    async fn document(
        &self,
        path: &str,
    ) -> Result<std::sync::Arc<octocode_engine::graph::GraphFactsDocument>, LspFailure> {
        let mut documents = self.documents.lock().await;
        if let Some(document) = documents.get(path).cloned() {
            return Ok(document);
        }
        let source = self.source(path).await?;
        let owned_path = path.to_owned();
        let document = super::blocking_cancellable(self.cancel, move |_| {
            heritage::type_document(&source.content, &owned_path)
        })
        .await?
        .ok_or_else(|| LspFailure::invalid_query("Type parsing worker failed"))?
        .map_err(LspFailure::invalid_query)?;
        let document = std::sync::Arc::new(document);
        documents.insert(path.to_owned(), document.clone());
        Ok(document)
    }

    async fn node(&self, path: &str, line: u32, character: u32) -> Result<Value, LspFailure> {
        let document = self.document(path).await?;
        heritage::type_node(&document, path, line, character).map_err(LspFailure::invalid_query)
    }

    /// The trait implementation located at `line:character`, if any: the
    /// subtype a Rust server's implementation answer names.
    async fn impl_node(
        &self,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Option<Value>, LspFailure> {
        let document = self.document(path).await?;
        Ok(heritage::impl_node(&document, path, line, character))
    }

    async fn resolved_nodes(
        &self,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<Value>, LspFailure> {
        match self.node(path, line, character).await {
            Ok(node) => Ok(vec![node]),
            Err(failure) if failure.is_cancelled() => Err(failure),
            Err(_) => self.definitions(path, line, character).await,
        }
    }

    async fn definitions(
        &self,
        path: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<Value>, LspFailure> {
        let mut found = get_locations(
            self.client,
            self.snippet_policy,
            self.cancel,
            LocationRequest::Definition,
            path,
            line,
            character,
        )
        .await?;
        let mut nodes = Vec::new();
        let mut unresolved = false;
        for snippet in &found {
            let target = uri_to_path(&snippet.uri);
            match self
                .node(
                    &target,
                    snippet.range.start.line,
                    snippet.range.start.character,
                )
                .await
            {
                Ok(node) => nodes.push(node),
                Err(failure) if failure.is_cancelled() => return Err(failure),
                Err(_) => unresolved = true,
            }
        }
        if !unresolved && !nodes.is_empty() {
            return Ok(nodes);
        }
        if unresolved {
            let (resolved, warnings) = {
                let mut sources = self.sources.lock().await;
                resolve_definition_chain(
                    self.client,
                    &mut sources,
                    self.snippet_policy,
                    self.cancel,
                    path,
                    line,
                    character,
                )
                .await?
            };
            found = resolved;
            for warning in warnings {
                self.fail(warning).await;
            }
        }
        if found.is_empty() {
            self.fail(format!(
                "No definition establishes the heritage relation at {path}:{}:{}",
                line + 1,
                character + 1
            ))
            .await;
        }
        nodes.clear();
        for found in found {
            self.cancel.check().map_err(LspFailure::cancelled)?;
            let target = uri_to_path(&found.uri);
            match self
                .node(&target, found.range.start.line, found.range.start.character)
                .await
            {
                Ok(node) => nodes.push(node),
                Err(failure) if failure.is_cancelled() => return Err(failure),
                Err(failure) => self.fail_from(failure).await,
            }
        }
        Ok(nodes)
    }

    async fn bases(&self, node: &Value) -> Result<Vec<Value>, LspFailure> {
        let key = self.keys.lock().await.key(node);
        if let Some(bases) = self.bases.lock().await.get(&key).cloned() {
            return Ok(bases);
        }
        let path = node_path(node)?;
        let source = self.source(&path).await?;
        let document = self.document(&path).await?;
        let bases = heritage::direct_bases(&document, &source.content, &path, node)
            .map_err(LspFailure::invalid_query)?;
        let mut resolved = Vec::new();
        for base in bases {
            match self.definitions(&path, base.line, base.character).await {
                Ok(nodes) => resolved.extend(nodes),
                Err(failure) if failure.is_cancelled() => return Err(failure),
                Err(failure) => self.fail_from(failure).await,
            }
        }
        self.bases.lock().await.insert(key, resolved.clone());
        Ok(resolved)
    }

    async fn subtypes(&self, parent: &Value) -> Result<Vec<Value>, LspFailure> {
        // Nothing implements an impl block; asking the server at its self
        // type would list that type's own impls.
        if parent.get("kind").and_then(Value::as_str) == Some("impl") {
            return Ok(Vec::new());
        }
        let path = node_path(parent)?;
        let (line, character) = node_position(parent)?;
        let parent_key = self.keys.lock().await.key(parent);
        let seed = self.initial.lock().await.take();
        let (candidates, anchor) = match seed {
            Some((root, candidates)) if self.keys.lock().await.key(&root) == parent_key => {
                (candidates, true)
            }
            _ => (
                get_locations(
                    self.client,
                    self.snippet_policy,
                    self.cancel,
                    LocationRequest::Implementation,
                    &path,
                    line,
                    character,
                )
                .await?,
                false,
            ),
        };
        // The anchor already owns a checked CandidateWindow. Deeper types
        // still have a terminal per-node candidate limit.
        if !anchor && candidates.len() > MAX_HIERARCHY_NODES {
            self.candidate_limit
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let mut direct = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for candidate in candidates.into_iter().take(MAX_HIERARCHY_NODES) {
            self.cancel.check().map_err(LspFailure::cancelled)?;
            let path = uri_to_path(&candidate.uri);
            let (line, character) = (candidate.range.start.line, candidate.range.start.character);
            // A trait implementation is the subtype itself, not the type its
            // self-type position resolves to (which declares no heritage).
            let resolved = match self.impl_node(&path, line, character).await {
                Ok(Some(node)) => Ok(vec![node]),
                Err(failure) if failure.is_cancelled() => return Err(failure),
                _ => self.resolved_nodes(&path, line, character).await,
            };
            let resolved = match resolved {
                Ok(resolved) => resolved,
                Err(failure) if failure.is_cancelled() => return Err(failure),
                Err(failure) => {
                    self.fail_from(failure).await;
                    continue;
                }
            };
            let mut distinct = {
                let mut keys = self.keys.lock().await;
                let mut identities = std::collections::HashSet::new();
                resolved
                    .into_iter()
                    .filter(|node| identities.insert(keys.key(node)))
                    .collect::<Vec<_>>()
            };
            if distinct.len() > 1 {
                self.fail(format!(
                    "Implementation candidate at {path}:{}:{} resolves to {} distinct type declarations; its immediate subtype identity is ambiguous",
                    line + 1,
                    character + 1,
                    distinct.len()
                )).await;
                continue;
            }
            let Some(candidate) = distinct.pop() else {
                continue;
            };
            let identity = self.keys.lock().await.key(&candidate);
            if identity == parent_key || !seen.insert(identity) {
                continue;
            }
            match self.bases(&candidate).await {
                Ok(bases) => {
                    let mut keys = self.keys.lock().await;
                    if bases.iter().any(|base| keys.key(base) == parent_key) {
                        direct.push(candidate);
                    } else if bases.is_empty() {
                        self.fail(format!("No resolved immediate heritage establishes the implementation candidate {}", candidate["name"])).await;
                    }
                }
                Err(failure) if failure.is_cancelled() => return Err(failure),
                Err(failure) => self.fail_from(failure).await,
            }
        }
        Ok(direct)
    }

    pub(super) async fn run(
        &self,
        anchor: Position<'_>,
        expansion: Expansion,
        depth: u32,
        initial: Option<Vec<LspCodeSnippet>>,
    ) -> Result<HierarchyWalk, LspFailure> {
        let roots = self
            .resolved_nodes(anchor.path, anchor.line, anchor.character)
            .await?;
        if let Some(initial) = initial {
            if roots.len() != 1 {
                return Err(LspFailure::invalid_query(
                    "Cannot assign implementation candidates to an ambiguous type anchor",
                ));
            }
            *self.initial.lock().await = Some((roots[0].clone(), initial));
        }
        let paths = self.sources.lock().await.policy();
        let mut walk = Walk::new(paths, &roots, expansion, depth)
            .finish(self, self.cancel)
            .await?;
        walk.failures.extend(self.failures.lock().await.drain(..));
        Ok(walk)
    }
}

impl HierarchySource for Fallback<'_, '_> {
    async fn expand(&self, expansion: Expansion, item: Value) -> Result<Value, LspFailure> {
        let result = match expansion {
            Expansion::Supertypes => self.bases(&item).await,
            Expansion::Subtypes => self.subtypes(&item).await,
            _ => Err(LspFailure::invalid_query(
                "The heritage source expands only type hierarchy edges",
            )),
        };
        result.map(|nodes| json!(nodes))
    }
}

fn node_path(node: &Value) -> Result<String, LspFailure> {
    node.get("uri")
        .and_then(Value::as_str)
        .map(uri_to_path)
        .ok_or_else(|| LspFailure::invalid_query("Type declaration has no local URI"))
}

fn node_position(node: &Value) -> Result<(u32, u32), LspFailure> {
    let point = |field: &str| {
        node.pointer(&format!("/selectionRange/start/{field}"))
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
    };
    match (point("line"), point("character")) {
        (Some(line), Some(character)) => Ok((line, character)),
        _ => Err(LspFailure::invalid_query(
            "Type declaration has no exact anchor",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LspSearchQuery, locations};
    use super::*;

    fn candidates() -> Vec<LspCodeSnippet> {
        (0..MAX_HIERARCHY_NODES + 3)
            .map(|line| {
                serde_json::from_value(json!({
            "uri": "file:///workspace/types.ts",
            "range": {"start":{"line":line,"character":13},"end":{"line":line,"character":21}},
            "content": format!("export class Child{line} extends Base {{}}"),
        })).expect("snippet")
            })
            .collect()
    }

    fn query() -> LspSearchQuery {
        serde_json::from_value(json!({"operation":"subtypes","path":"/workspace/types.ts","line":1,"symbolName":"Base","pageSize":1})).expect("query")
    }

    #[test]
    fn candidate_windows_reject_mutation_and_preserve_stable_order() {
        let query = query();
        let first = CandidateWindow::cut(&query, candidates()).expect("first window");
        assert_eq!(first.items.len(), MAX_HIERARCHY_NODES);
        assert!(first.has_more);
        let mut next = lsp_search_query::to_row(&query);
        next["candidatePage"] = json!(2);
        next["snapshot"] = json!(first.digest);
        let next: LspSearchQuery = serde_json::from_value(next).expect("next window");
        let mut reordered = candidates();
        reordered.reverse();
        // Duplicate provider locations must not shift the candidate boundary.
        reordered.push(reordered[0].clone());
        let second = CandidateWindow::cut(&next, reordered).expect("same candidates");
        assert_eq!(second.items.len(), 3);
        assert_eq!(
            second.items[0].range.start.line as usize,
            MAX_HIERARCHY_NODES
        );
        assert!(!second.has_more);
        let mut removed = candidates();
        removed.remove(0);
        assert!(CandidateWindow::cut(&next, removed).is_none());
        let mut changed = candidates();
        changed[0].content.push_str(" // changed");
        assert!(CandidateWindow::cut(&next, changed).is_none());
        let mut missing = lsp_search_query::to_row(&next);
        missing
            .as_object_mut()
            .expect("query object")
            .remove("snapshot");
        assert!(
            CandidateWindow::cut(
                &serde_json::from_value(missing).expect("query"),
                candidates()
            )
            .is_none()
        );
    }

    #[test]
    fn candidate_mutation_stales_later_result_pages_even_when_rows_disappear() {
        let query = query();
        let window = CandidateWindow::cut(&query, candidates()).expect("window");
        let items = json!([{"name":"first"},{"name":"second"}]);
        let first = super::super::failure::with_next(
            &query,
            locations::salted_items_payload(
                &query,
                "subtypes",
                items.clone(),
                Some(&window.digest),
                16000,
                super::super::render::plain_sizes,
            ),
        );
        let next: LspSearchQuery =
            serde_json::from_value(first["next"]["nextPage"]["query"]["queries"][0].clone())
                .expect("next result page");
        let stable = locations::salted_items_payload(
            &next,
            "subtypes",
            items.clone(),
            Some(&window.digest),
            16000,
            super::super::render::plain_sizes,
        );
        assert!(stable.get("errorCode").is_none(), "{stable}");
        let mut changed = candidates();
        changed.remove(0);
        let changed =
            CandidateWindow::cut(&next, changed).expect("digest checked by result pagination");
        for rows in [items, json!([])] {
            let stale = locations::salted_items_payload(
                &next,
                "subtypes",
                rows,
                Some(&changed.digest),
                16000,
                super::super::render::plain_sizes,
            );
            assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
            assert!(stale.pointer("/next/restart").is_some(), "{stale}");
        }
    }
}
