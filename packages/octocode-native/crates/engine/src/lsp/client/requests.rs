//! Query requests: the typed LSP methods, the response cache, and the
//! content-modified retry loop.

use super::snippets::snippets_from_locations;
use super::{
    CONTENT_MODIFIED_RETRIES, CONTENT_MODIFIED_RETRY_DELAY_MS, CachedResponse, NativeLspClient,
    REQUEST_TIMEOUT_MS, RESPONSE_SCOPE, SnippetReadPolicy,
};
use crate::error::{Error, Result};
use crate::lsp::transport::{JsonRpcConnection, WaitBudget};
use crate::lsp::types::LspCodeSnippet;
use crate::lsp::uri::path_to_uri;
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};
use tokio::time::Duration;

impl NativeLspClient {
    pub async fn get_hover(&self, file_path: String, line: u32, character: u32) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
        .await
    }

    /// `textDocument/documentSymbol`, shared with the response cache (a
    /// reused answer is not copied).
    pub async fn get_document_symbols(&self, file_path: String) -> Result<Arc<Value>> {
        let uri = path_to_uri(&file_path)?;
        self.cached_request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
            None,
        )
        .await
    }

    pub async fn prepare_call_hierarchy(
        &self,
        file_path: String,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/prepareCallHierarchy",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
        .await
    }

    pub async fn incoming_calls(&self, item: Value) -> Result<Value> {
        self.request("callHierarchy/incomingCalls", json!({ "item": item }))
            .await
    }

    pub async fn outgoing_calls(&self, item: Value) -> Result<Value> {
        self.request("callHierarchy/outgoingCalls", json!({ "item": item }))
            .await
    }

    /// Project-wide fuzzy symbol search — `workspace/symbol`.
    /// Returns `WorkspaceSymbol[] | SymbolInformation[]` (raw JSON).
    /// `query` is the fuzzy name string; empty string returns all symbols.
    pub async fn workspace_symbol(&self, query: String) -> Result<Value> {
        self.request("workspace/symbol", json!({ "query": query }))
            .await
    }

    /// Prepare a type-hierarchy item at a given position — `textDocument/prepareTypeHierarchy`.
    /// Returns `TypeHierarchyItem[] | null` (raw JSON).
    pub async fn prepare_type_hierarchy(
        &self,
        file_path: String,
        line: u32,
        character: u32,
    ) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/prepareTypeHierarchy",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character }
            }),
        )
        .await
    }

    /// Retrieve supertypes (base classes / implemented interfaces) — `typeHierarchy/supertypes`.
    /// `item` is a `TypeHierarchyItem` previously returned by `prepareTypeHierarchy`.
    pub async fn type_hierarchy_supertypes(&self, item: Value) -> Result<Value> {
        self.request("typeHierarchy/supertypes", json!({ "item": item }))
            .await
    }

    /// Retrieve subtypes (subclasses / implementors) — `typeHierarchy/subtypes`.
    /// `item` is a `TypeHierarchyItem` previously returned by `prepareTypeHierarchy`.
    pub async fn type_hierarchy_subtypes(&self, item: Value) -> Result<Value> {
        self.request("typeHierarchy/subtypes", json!({ "item": item }))
            .await
    }

    /// Pull diagnostics for a single file — `textDocument/diagnostic` (LSP 3.17+).
    /// Returns `DocumentDiagnosticReport` with `kind: "full"|"unchanged"` and `items: Diagnostic[]`.
    /// Prefer pull diagnostics over push (`publishDiagnostics`) for agent/CLI use: you control
    /// *when* to request them and avoid a notification firehose.
    pub async fn get_diagnostics(&self, file_path: String) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/diagnostic",
            json!({ "textDocument": { "uri": uri } }),
        )
        .await
    }

    /// Return the latest bounded `textDocument/publishDiagnostics` payload for
    /// a file, waiting briefly when the server has not published one yet.
    pub async fn get_push_diagnostics(
        &self,
        file_path: String,
        timeout_ms: Option<u32>,
    ) -> Result<Option<Value>> {
        let uri = path_to_uri(&file_path)?;
        let _activity = self.lease();
        let connection = self.connection_handle().await?;
        let min_version = self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .version(&uri)
            .map(i64::from);
        Ok(connection
            .wait_for_push_diagnostics(&uri, timeout_ms.unwrap_or(1_500).min(10_000), min_version)
            .await)
    }

    /// Clones the connection handle out from under the lock, releasing the
    /// guard before the caller awaits any request. This keeps the
    /// `connection` mutex uncontended (held only for the clone) so concurrent
    /// LSP requests are NOT serialized and cannot head-of-line block one
    /// another. Returns an error if the client has not been started.
    pub(super) async fn connection_handle(&self) -> Result<Arc<JsonRpcConnection>> {
        self.inner
            .connection
            .lock()
            .await
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| Error::new("LSP client not initialized"))
    }

    /// Definition, references, type-definition, or implementation locations
    /// with snippet content. Every server-supplied target path passes
    /// `policy` before any of its bytes are read; a refused path keeps its
    /// location with [`super::SNIPPET_CONTENT_WITHHELD`] as content. Reads are
    /// bounded to regular files of at most 1 MB.
    ///
    /// With `settle`, the answer is one asked at least `settle` after the
    /// call: an answer the server gave earlier (before a project load
    /// settled) is never returned, and a settled answer the response scope
    /// already holds is returned without waiting.
    pub async fn get_locations(
        &self,
        request: LocationRequest,
        file_path: String,
        line: u32,
        character: u32,
        policy: &SnippetReadPolicy,
        settle: Option<Duration>,
    ) -> Result<Vec<LspCodeSnippet>> {
        let uri = path_to_uri(&file_path)?;
        let mut params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character }
        });
        if let LocationRequest::References {
            include_declaration,
        } = request
        {
            params["context"] = json!({ "includeDeclaration": include_declaration });
        }
        let result = self
            .cached_request(request.method(), params, settle)
            .await?;
        snippets_from_locations(&result, policy).await
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.cached_request(method, params, None)
            .await
            .map(Arc::unwrap_or_clone)
    }

    /// `method` under the current [`RESPONSE_SCOPE`], once every open
    /// document matches its file: a reused answer is shared, not copied,
    /// and only one given since the open documents' text last changed is
    /// reused. A `settle` request keys its answer apart from the plain one
    /// and waits `settle` only when it reaches the server.
    async fn cached_request(
        &self,
        method: &str,
        params: Value,
        settle: Option<Duration>,
    ) -> Result<Arc<Value>> {
        self.sync_open_documents().await?;
        let settle = settle.map(|settle| self.settle(settle));
        let (reuse, generation) = RESPONSE_SCOPE
            .try_with(|scope| {
                let scope = scope.borrow();
                (scope.reuse, scope.generation.clone())
            })
            .unwrap_or_default();
        if generation.is_empty() {
            if let Some(settle) = settle {
                tokio::time::sleep(settle).await;
            }
            return self.send_request(method, params).await.map(Arc::new);
        }
        let variant = if settle.is_some() { "settled" } else { "" };
        let epoch = self.inner.content_epoch.load(Ordering::Acquire);
        let key = format!("{generation}\u{0}{epoch}\u{0}{variant}\u{0}{method}\u{0}{params}");
        if reuse && let Some(hit) = self.inner.responses.get(&key) {
            return Ok(hit.value);
        }
        if let Some(settle) = settle {
            tokio::time::sleep(settle).await;
        }
        let value = Arc::new(self.send_request(method, params).await?);
        self.inner
            .responses
            .insert(key, CachedResponse::new(Arc::clone(&value)));
        Ok(value)
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        let _activity = self.lease();
        // Acquire a cloned handle and DROP the guard before awaiting, so the
        // request + content-modified retry loop never holds the connection
        // mutex across `.await`.
        let connection = self.connection_handle().await?;
        // One budget for the request and its content-modified retries: the
        // starvation extension (see `transport::patience`) is granted once.
        let budget = WaitBudget::new(REQUEST_TIMEOUT_MS);
        let mut attempts = 0;
        loop {
            let response = if supports_partial_results(method) {
                connection
                    .request_with_partials(method, params.clone(), budget)
                    .await
            } else {
                connection.request(method, params.clone(), budget).await
            };
            match response {
                Ok(value) => return Ok(value),
                Err(error)
                    if is_retryable_error(&error)
                        && attempts < CONTENT_MODIFIED_RETRIES
                        && !budget.is_spent() =>
                {
                    attempts += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(
                        CONTENT_MODIFIED_RETRY_DELAY_MS,
                    ))
                    .await;
                }
                Err(error) => return Err(error),
            }
        }
    }
}

/// Which location request [`NativeLspClient::get_locations`] sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationRequest {
    Definition,
    References { include_declaration: bool },
    TypeDefinition,
    Implementation,
}

impl LocationRequest {
    fn method(self) -> &'static str {
        match self {
            Self::Definition => "textDocument/definition",
            Self::References { .. } => "textDocument/references",
            Self::TypeDefinition => "textDocument/typeDefinition",
            Self::Implementation => "textDocument/implementation",
        }
    }
}

fn supports_partial_results(method: &str) -> bool {
    matches!(
        method,
        "textDocument/definition"
            | "textDocument/references"
            | "textDocument/typeDefinition"
            | "textDocument/implementation"
            | "textDocument/documentSymbol"
            | "workspace/symbol"
            | "callHierarchy/incomingCalls"
            | "callHierarchy/outgoingCalls"
            | "typeHierarchy/supertypes"
            | "typeHierarchy/subtypes"
            | "textDocument/diagnostic"
    )
}

/// A request failure worth re-sending: `ContentModified`, or
/// `ServerCancelled` when the server asked for a retrigger. Matches the typed
/// RPC error, never the rendered message text.
pub(super) fn is_retryable_error(error: &Error) -> bool {
    error
        .rpc_error()
        .is_some_and(crate::error::RpcError::is_retryable)
}
