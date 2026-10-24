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
use octocode_contract_types::LspSearchQuery;
use octocode_engine::lsp::client::{LocationRequest, NativeLspClient, SnippetReadPolicy};
use octocode_engine::lsp::types::LspCodeSnippet;
use serde_json::{Value, json};
use tokio::sync::Mutex;

mod edges;

/// One window of implementation candidates, bound to the whole ordered set.
pub(super) struct CandidateWindow {
    pub(super) items: Vec<LspCodeSnippet>,
    pub(super) digest: String,
    pub(super) has_more: bool,
}

impl CandidateWindow {
    pub(super) fn cut(query: &LspSearchQuery, mut candidates: Vec<LspCodeSnippet>) -> Option<Self> {
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
            for field in ["page", "candidatePage", "snapshot"] {
                fields.remove(field);
            }
        }
        let identity = octocode_engine::digest::canonical_json(
            json!({
                "query": scope,
                "candidates": candidates,
            }),
            octocode_engine::digest::Nulls::Drop,
        );
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
mod tests;
