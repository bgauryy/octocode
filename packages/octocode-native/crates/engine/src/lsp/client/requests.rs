//! Query requests: the typed LSP methods, selected provider evidence, and the
//! content-modified retry loop.

mod methods;

pub use methods::LocationRequest;
pub(super) use methods::is_retryable_error;
use methods::{Reuse, result_id, supports_partial_results};

use super::snippets::snippets_from_locations;
use super::{
    CONTENT_MODIFIED_RETRIES, CONTENT_MODIFIED_RETRY_DELAY_MS, NativeLspClient, REQUEST_TIMEOUT_MS,
    RESPONSE_SCOPE, SnippetReadPolicy,
};
use crate::error::{Error, Result};
use crate::lsp::transport::{JsonRpcConnection, WaitBudget};
use crate::lsp::types::LspCodeSnippet;
use crate::lsp::uri::path_to_uri;
use serde_json::{Value, json};
use std::sync::Arc;
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

    /// `textDocument/documentSymbol`, shared with selected provider evidence (a
    /// reused answer is not copied).
    pub async fn get_document_symbols(&self, file_path: String) -> Result<Arc<Value>> {
        let uri = path_to_uri(&file_path)?;
        self.provider_request(
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
    /// Returns the full `DocumentDiagnosticReport` (`kind: "full"`, `items`).
    /// The request names the last full report's `resultId` for an open
    /// document, so a server whose diagnostics did not change answers
    /// `unchanged`, and that previous report is returned.
    /// Prefer pull diagnostics over push (`publishDiagnostics`) for agent/CLI use: you control
    /// *when* to request them and avoid a notification firehose.
    pub async fn get_diagnostics(&self, file_path: String) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        let previous = self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .diagnostic_report(&uri);
        let mut params = json!({ "textDocument": { "uri": uri } });
        if let Some(result_id) = previous.as_deref().and_then(result_id) {
            params["previousResultId"] = json!(result_id);
        }
        let report = self
            .provider_request("textDocument/diagnostic", params, None)
            .await?;
        let full = match (report.get("kind").and_then(Value::as_str), previous) {
            (Some("unchanged"), Some(previous)) => {
                let mut full = Arc::unwrap_or_clone(previous);
                if let Some(result_id) = report.get("resultId") {
                    full["resultId"] = result_id.clone();
                }
                Arc::new(full)
            }
            _ => report,
        };
        if full.get("kind").and_then(Value::as_str) == Some("full") && result_id(&full).is_some() {
            self.inner
                .open_docs
                .lock()
                .map_err(|_| Error::new("open_docs lock poisoned"))?
                .remember_diagnostic_report(&uri, Arc::clone(&full));
        }
        Ok(Arc::unwrap_or_clone(full))
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
            .provider_request(request.method(), params, settle)
            .await?;
        snippets_from_locations(&result, policy).await
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.provider_request(method, params, None)
            .await
            .map(Arc::unwrap_or_clone)
    }

    /// The outline of a document [`Self::open_document`] synced, asked first
    /// without waiting for a project load: an outline is usually the file's
    /// own syntax. A server may answer an empty outline until its project
    /// loads, so when the answer is empty, `content` has text, and the
    /// document still owes its open wait, this pays only that owed wait
    /// ([`Self::open_document_and_wait`]) and asks once more, recording the
    /// new answer in place of the empty one. Returns the outline and the
    /// readiness of a wait it paid.
    pub async fn get_document_symbols_settled(
        &self,
        file_path: String,
        content: &str,
    ) -> Result<(Arc<Value>, Option<String>)> {
        let symbols = self.get_document_symbols(file_path.clone()).await?;
        let empty = symbols.is_null() || symbols.as_array().is_some_and(Vec::is_empty);
        let uri = path_to_uri(&file_path)?;
        let owed = self
            .inner
            .open_docs
            .lock()
            .map_err(|_| Error::new("open_docs lock poisoned"))?
            .wait_owed(&uri);
        if !empty || content.trim().is_empty() || !owed {
            return Ok((symbols, None));
        }
        let readiness = self
            .open_document_and_wait(file_path, content, None, None)
            .await?;
        let symbols = self
            .provider_answer(
                "textDocument/documentSymbol",
                json!({ "textDocument": { "uri": uri } }),
                None,
                Reuse::Fresh,
            )
            .await?;
        Ok((symbols, readiness))
    }

    /// Read one method's answer from this operation's selected evidence, or
    /// ask the server and retain it for this operation's later pages.
    async fn provider_request(
        &self,
        method: &str,
        params: Value,
        settle: Option<Duration>,
    ) -> Result<Arc<Value>> {
        self.provider_answer(method, params, settle, Reuse::Recorded)
            .await
    }

    /// [`Self::provider_request`] with `reuse` choosing whether a recorded
    /// answer may stand in for asking the server.
    async fn provider_answer(
        &self,
        method: &str,
        params: Value,
        settle: Option<Duration>,
        reuse: Reuse,
    ) -> Result<Arc<Value>> {
        let (call, answers) = RESPONSE_SCOPE
            .try_with(|scope| {
                let scope = scope.borrow();
                (scope.call, scope.answers.clone())
            })
            .unwrap_or_default();
        self.sync_open_documents_for(call).await?;
        let settle = settle.map(|settle| self.settle(settle));
        // The digest identifies the request without retaining its server
        // configuration (environment, initialization options) or parameters
        // in the answers a continuation persists.
        let key = crate::digest::sha256(
            json!([
                &self.inner.config,
                method,
                &params,
                settle.map(|duration| duration.as_nanos())
            ])
            .to_string()
            .as_bytes(),
        );
        if reuse == Reuse::Recorded
            && let Some(value) = answers.as_ref().and_then(|answers| {
                answers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .0
                    .get(&key)
                    .cloned()
            })
        {
            return Ok(value);
        }
        if let Some(settle) = settle {
            tokio::time::sleep(settle).await;
        }
        let value = Arc::new(self.send_request(method, params).await?);
        if let Some(answers) = answers {
            answers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0
                .insert(key, Arc::clone(&value));
        }
        Ok(value)
    }

    async fn send_request(&self, method: &str, params: Value) -> Result<Value> {
        // Bound the async frame depth shared by every semantic request path.
        let request: std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Value>> + Send + '_>,
        > = Box::pin(self.send_request_inner(method, params));
        request.await
    }

    async fn send_request_inner(&self, method: &str, params: Value) -> Result<Value> {
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
