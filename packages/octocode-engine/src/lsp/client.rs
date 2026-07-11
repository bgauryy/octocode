use crate::lsp::json_rpc::{ClientRequestContext, JsonRpcConnection, ProgressTracker};
use crate::lsp::types::{JsCodeSnippet, JsExactPosition, JsLanguageServerConfig, JsRange};
use crate::lsp::uri::{path_to_uri, uri_to_path};
use napi::{Error, Result, Status};
use napi_derive::napi;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

const REQUEST_TIMEOUT_MS: u32 = 30_000;
const CONTENT_MODIFIED_RETRIES: u8 = 3;
const CONTENT_MODIFIED_RETRY_DELAY_MS: u64 = 500;
const STDERR_RING_CAPACITY: usize = 100;
const STDERR_LINE_MAX_CHARS: usize = 2_000;

#[napi]
pub struct NativeLspClient {
    config: JsLanguageServerConfig,
    child: Mutex<Option<Child>>,
    // Stored behind an `Arc` so callers can clone a handle out from under the
    // lock and then release the guard BEFORE awaiting the (potentially
    // multi-second) request, instead of serializing all LSP traffic on this
    // mutex. `JsonRpcConnection` is internally `Send + Sync` and supports
    // concurrent `request`/`notify` (its writer + pending map are each
    // `Arc<Mutex<..>>`), so cloned handles are safe to use in parallel.
    connection: Mutex<Option<Arc<JsonRpcConnection<ChildStdin>>>>,
    stderr_task: Mutex<Option<JoinHandle<()>>>,
    stderr_lines: Arc<StdMutex<VecDeque<String>>>,
    capabilities: StdMutex<Option<Value>>,
    /// The `positionEncoding` the server selected in its `InitializeResult`
    /// (LSP 3.17). We advertise UTF-16 only, so this should be `utf-16` or absent
    /// (absent ⇒ utf-16 by spec). Any other value means the server ignored our
    /// capability and our offsets may be misaligned on non-ASCII lines — surfaced
    /// as a stderr warning at start time.
    position_encoding: StdMutex<Option<String>>,
    progress: Arc<ProgressTracker>,
    /// Open-document lifecycle state: `uri -> last sent version`. Drives the
    /// LSP `didOpen` (once) → `didChange` (incrementing version) → `didClose`
    /// protocol so servers never see a second `didOpen` for the same document.
    open_docs: StdMutex<HashMap<String, i32>>,
}

#[napi]
impl NativeLspClient {
    #[napi(constructor)]
    pub fn new(config: JsLanguageServerConfig) -> Self {
        Self {
            config,
            child: Mutex::new(None),
            connection: Mutex::new(None),
            stderr_task: Mutex::new(None),
            stderr_lines: Arc::new(StdMutex::new(VecDeque::new())),
            capabilities: StdMutex::new(None),
            position_encoding: StdMutex::new(None),
            progress: ProgressTracker::new(),
            open_docs: StdMutex::new(HashMap::new()),
        }
    }

    #[napi]
    pub async fn start(&self) -> Result<()> {
        let mut child_guard = self.child.lock().await;
        if child_guard.is_some() {
            return Err(Error::new(
                Status::GenericFailure,
                "LSP client already started",
            ));
        }
        if let Ok(mut stderr_lines) = self.stderr_lines.lock() {
            stderr_lines.clear();
        }
        if let Ok(mut capabilities) = self.capabilities.lock() {
            *capabilities = None;
        }
        if let Ok(mut encoding) = self.position_encoding.lock() {
            *encoding = None;
        }
        if let Ok(mut open_docs) = self.open_docs.lock() {
            open_docs.clear();
        }

        let mut command = tokio::process::Command::new(&self.config.command);
        command
            .args(self.config.args.clone().unwrap_or_default())
            .current_dir(&self.config.workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(env) = &self.config.env {
            for (key, value) in env {
                command.env(key, value);
            }
        }

        let mut child = command.spawn().map_err(|err| {
            Error::new(
                Status::GenericFailure,
                format!("Failed to start language server: {err}"),
            )
        })?;
        let stderr_task = child
            .stderr
            .take()
            .map(|stderr| spawn_stderr_reader(stderr, Arc::clone(&self.stderr_lines)));
        let Some(stdout) = child.stdout.take() else {
            cleanup_failed_start(&mut child, stderr_task).await;
            return Err(Error::new(
                Status::GenericFailure,
                "Language server stdout pipe missing",
            ));
        };
        let Some(stdin) = child.stdin.take() else {
            cleanup_failed_start(&mut child, stderr_task).await;
            return Err(Error::new(
                Status::GenericFailure,
                "Language server stdin pipe missing",
            ));
        };

        let root_uri = match path_to_uri(&self.config.workspace_root) {
            Ok(uri) => uri,
            Err(error) => {
                cleanup_failed_start(&mut child, stderr_task).await;
                return Err(error);
            }
        };
        let connection = Arc::new(JsonRpcConnection::new(
            stdout,
            stdin,
            ClientRequestContext {
                configuration: self
                    .config
                    .initialization_options
                    .clone()
                    .unwrap_or_else(|| json!({})),
                workspace_folders: json!([{ "uri": root_uri, "name": "workspace" }]),
            },
            Arc::clone(&self.progress),
        ));
        let initialize_result = match initialize(&connection, &self.config).await {
            Ok(value) => value,
            Err(error) => {
                cleanup_failed_start(&mut child, stderr_task).await;
                return Err(error);
            }
        };
        if let Ok(mut capabilities) = self.capabilities.lock() {
            *capabilities = initialize_result.get("capabilities").cloned();
        }
        // Reconcile the negotiated position encoding (LSP 3.17). We only emit
        // UTF-16, so anything else means the server ignored our advertised
        // capability and our offsets may be wrong on non-ASCII lines — make that
        // observable instead of silently returning mis-positioned results.
        let negotiated_encoding = extract_position_encoding(&initialize_result);
        if let Some(encoding) = negotiated_encoding.as_deref() {
            if encoding != "utf-16" {
                push_stderr_line(
                    &self.stderr_lines,
                    format!(
                        "[octocode] WARNING: language server negotiated positionEncoding \
                         '{encoding}' but octocode only emits utf-16; positions on lines with \
                         non-ASCII characters may be misaligned"
                    ),
                );
            }
        }
        if let Ok(mut encoding) = self.position_encoding.lock() {
            *encoding = negotiated_encoding;
        }
        if let Err(error) = connection.notify("initialized", json!({})).await {
            cleanup_failed_start(&mut child, stderr_task).await;
            return Err(error);
        }

        *self.connection.lock().await = Some(connection);
        *self.stderr_task.lock().await = stderr_task;
        *child_guard = Some(child);
        Ok(())
    }

    #[napi]
    pub async fn stop(&self) -> Result<()> {
        let connection = self.connection.lock().await.take();
        if let Some(connection) = connection {
            let _ = connection.request("shutdown", Value::Null, 1_000).await;
            let _ = connection.notify("exit", Value::Null).await;
        }
        if let Some(mut child) = self.child.lock().await.take() {
            let _ = child.kill().await;
        }
        if let Some(task) = self.stderr_task.lock().await.take() {
            task.abort();
        }
        if let Ok(mut capabilities) = self.capabilities.lock() {
            *capabilities = None;
        }
        if let Ok(mut encoding) = self.position_encoding.lock() {
            *encoding = None;
        }
        if let Ok(mut open_docs) = self.open_docs.lock() {
            open_docs.clear();
        }
        Ok(())
    }

    /// Wait for the server to finish any post-`initialized` indexing, returning
    /// a readiness descriptor so JS can tell a confirmed-idle server apart from
    /// one that never reported progress or is still busy. The returned string
    /// is one of `"progressIdle"`, `"settledFallback"`, or `"timeout"`.
    #[napi]
    pub async fn wait_for_ready(&self, timeout_ms: Option<u32>) -> Result<String> {
        let timeout_ms = u64::from(timeout_ms.unwrap_or(45_000));
        Ok(self
            .progress
            .wait_until_idle(timeout_ms)
            .await
            .as_str()
            .to_owned())
    }

    #[napi]
    pub fn has_capability(&self, capability: String) -> bool {
        let Ok(capabilities) = self.capabilities.lock() else {
            return false;
        };
        capabilities
            .as_ref()
            .map(|value| capability_supported(value, &capability))
            .unwrap_or(false)
    }

    /// The `positionEncoding` the server selected at initialize time, if any.
    /// `None` means the server omitted it (implying the spec default, utf-16) or
    /// the client has not started yet. octocode advertises utf-16 only, so a
    /// value other than `Some("utf-16")` indicates a non-conformant server.
    #[napi]
    pub fn position_encoding(&self) -> Option<String> {
        self.position_encoding
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
    }

    #[napi(js_name = "getRecentStderr")]
    pub fn get_recent_stderr(&self) -> Vec<String> {
        self.stderr_lines
            .lock()
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Sync a document's in-memory content to the server, honoring the LSP
    /// document lifecycle: the FIRST sync of a URI sends `textDocument/didOpen`
    /// (version 1); every subsequent sync sends `textDocument/didChange` with an
    /// incremented version and a full-document content change. Re-sending
    /// `didOpen` (as before) is ignored or rejected by many servers and can make
    /// changed content resolve against the stale original.
    #[napi]
    pub async fn open_document(&self, file_path: String, content: String) -> Result<()> {
        let uri = path_to_uri(&file_path)?;

        // Acquire the connection FIRST: if the client isn't started this fails
        // without mutating `open_docs`, so a doc is never marked open when its
        // didOpen/didChange was never actually sent.
        let connection = self.connection_handle().await?;

        // Decide didOpen-vs-didChange and reserve the version under the lock,
        // then release it before awaiting the notify (never hold a std mutex
        // across an await).
        let next_version = {
            let mut open_docs = self
                .open_docs
                .lock()
                .map_err(|_| Error::new(Status::GenericFailure, "open_docs lock poisoned"))?;
            let version = open_docs.get(&uri).copied().unwrap_or(0) + 1;
            open_docs.insert(uri.clone(), version);
            version
        };

        if next_version == 1 {
            let language_id = crate::lsp::config::detect_language_id(file_path.clone())
                .or_else(|| self.config.language_id.clone())
                .unwrap_or_else(|| "plaintext".to_owned());
            let params = json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": next_version,
                    "text": content
                }
            });
            connection.notify("textDocument/didOpen", params).await
        } else {
            let params = json!({
                "textDocument": { "uri": uri, "version": next_version },
                "contentChanges": [{ "text": content }]
            });
            connection.notify("textDocument/didChange", params).await
        }
    }

    /// Close a previously opened document (`textDocument/didClose`) and forget
    /// its version, so a later `open_document` starts a fresh `didOpen`.
    #[napi]
    pub async fn close_document(&self, file_path: String) -> Result<()> {
        let uri = path_to_uri(&file_path)?;
        let was_open = {
            let mut open_docs = self
                .open_docs
                .lock()
                .map_err(|_| Error::new(Status::GenericFailure, "open_docs lock poisoned"))?;
            open_docs.remove(&uri).is_some()
        };
        if !was_open {
            return Ok(());
        }
        let connection = self.connection_handle().await?;
        connection
            .notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": uri } }),
            )
            .await
    }

    #[napi]
    pub async fn get_definition(
        &self,
        file_path: String,
        line: u32,
        character: u32,
    ) -> Result<Vec<JsCodeSnippet>> {
        self.location_request("textDocument/definition", file_path, line, character)
            .await
    }

    #[napi]
    pub async fn get_references(
        &self,
        file_path: String,
        line: u32,
        character: u32,
        include_declaration: Option<bool>,
    ) -> Result<Vec<JsCodeSnippet>> {
        let uri = path_to_uri(&file_path)?;
        let params = json!({
            "textDocument": { "uri": uri },
            "position": { "line": line, "character": character },
            "context": { "includeDeclaration": include_declaration.unwrap_or(true) }
        });
        let result = self.request("textDocument/references", params).await?;
        snippets_from_locations(result).await
    }

    #[napi]
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

    #[napi]
    pub async fn get_type_definition(
        &self,
        file_path: String,
        line: u32,
        character: u32,
    ) -> Result<Vec<JsCodeSnippet>> {
        self.location_request("textDocument/typeDefinition", file_path, line, character)
            .await
    }

    #[napi]
    pub async fn get_implementation(
        &self,
        file_path: String,
        line: u32,
        character: u32,
    ) -> Result<Vec<JsCodeSnippet>> {
        self.location_request("textDocument/implementation", file_path, line, character)
            .await
    }

    #[napi]
    pub async fn get_document_symbols(&self, file_path: String) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )
        .await
    }

    #[napi]
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

    #[napi]
    pub async fn incoming_calls(&self, item: Value) -> Result<Value> {
        self.request("callHierarchy/incomingCalls", json!({ "item": item }))
            .await
    }

    #[napi]
    pub async fn outgoing_calls(&self, item: Value) -> Result<Value> {
        self.request("callHierarchy/outgoingCalls", json!({ "item": item }))
            .await
    }

    /// Project-wide fuzzy symbol search — `workspace/symbol`.
    /// Returns `WorkspaceSymbol[] | SymbolInformation[]` (raw JSON).
    /// `query` is the fuzzy name string; empty string returns all symbols.
    #[napi]
    pub async fn workspace_symbol(&self, query: String) -> Result<Value> {
        self.request("workspace/symbol", json!({ "query": query }))
            .await
    }

    /// Prepare a type-hierarchy item at a given position — `textDocument/prepareTypeHierarchy`.
    /// Returns `TypeHierarchyItem[] | null` (raw JSON).
    #[napi]
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
    #[napi]
    pub async fn type_hierarchy_supertypes(&self, item: Value) -> Result<Value> {
        self.request("typeHierarchy/supertypes", json!({ "item": item }))
            .await
    }

    /// Retrieve subtypes (subclasses / implementors) — `typeHierarchy/subtypes`.
    /// `item` is a `TypeHierarchyItem` previously returned by `prepareTypeHierarchy`.
    #[napi]
    pub async fn type_hierarchy_subtypes(&self, item: Value) -> Result<Value> {
        self.request("typeHierarchy/subtypes", json!({ "item": item }))
            .await
    }

    /// Pull diagnostics for a single file — `textDocument/diagnostic` (LSP 3.17+).
    /// Returns `DocumentDiagnosticReport` with `kind: "full"|"unchanged"` and `items: Diagnostic[]`.
    /// Prefer pull diagnostics over push (`publishDiagnostics`) for agent/CLI use: you control
    /// *when* to request them and avoid a notification firehose.
    #[napi]
    pub async fn get_diagnostics(&self, file_path: String) -> Result<Value> {
        let uri = path_to_uri(&file_path)?;
        self.request(
            "textDocument/diagnostic",
            json!({ "textDocument": { "uri": uri } }),
        )
        .await
    }
}

impl Drop for NativeLspClient {
    fn drop(&mut self) {
        self.connection.get_mut().take();
        if let Some(task) = self.stderr_task.get_mut().take() {
            task.abort();
        }
        if let Some(mut child) = self.child.get_mut().take() {
            let _ = child.start_kill();
        }
        if let Ok(mut capabilities) = self.capabilities.lock() {
            *capabilities = None;
        }
    }
}

impl NativeLspClient {
    /// Clones the connection handle out from under the lock, releasing the
    /// guard before the caller awaits any request. This keeps the
    /// `connection` mutex uncontended (held only for the clone) so concurrent
    /// LSP requests are NOT serialized and cannot head-of-line block one
    /// another. Returns an error if the client has not been started.
    async fn connection_handle(&self) -> Result<Arc<JsonRpcConnection<ChildStdin>>> {
        self.connection
            .lock()
            .await
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| Error::new(Status::GenericFailure, "LSP client not initialized"))
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        // Acquire a cloned handle and DROP the guard before awaiting, so the
        // request + content-modified retry loop never holds the connection
        // mutex across `.await`.
        let connection = self.connection_handle().await?;
        let mut attempts = 0;
        loop {
            match connection
                .request(method, params.clone(), REQUEST_TIMEOUT_MS)
                .await
            {
                Ok(value) => return Ok(value),
                Err(error)
                    if is_content_modified_error(&error) && attempts < CONTENT_MODIFIED_RETRIES =>
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

    async fn location_request(
        &self,
        method: &str,
        file_path: String,
        line: u32,
        character: u32,
    ) -> Result<Vec<JsCodeSnippet>> {
        let uri = path_to_uri(&file_path)?;
        let result = self
            .request(
                method,
                json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": line, "character": character }
                }),
            )
            .await?;
        snippets_from_locations(result).await
    }
}

/// Detects the LSP `ContentModified` (-32801) error so the request can be retried.
///
/// The reason string is the JSON error object rendered by [`read_loop`], e.g.
/// `LSP error: {"code":-32801,"message":"content modified"}`. We match on the
/// numeric error CODE rather than a free-text `"content modified"` substring,
/// which would false-positive on hover/diagnostic payloads that merely mention
/// the phrase (e.g. a doc-comment) and trigger spurious retries.
fn is_content_modified_error(error: &Error) -> bool {
    reason_has_error_code(&error.reason, -32801)
}

/// Returns true if the rendered JSON error object carries `"code": <code>`,
/// tolerating arbitrary whitespace between the key, colon, and value.
fn reason_has_error_code(reason: &str, code: i64) -> bool {
    let mut search = reason;
    while let Some(idx) = search.find("\"code\"") {
        let after = &search[idx + "\"code\"".len()..];
        let after = after.trim_start();
        if let Some(rest) = after.strip_prefix(':') {
            let rest = rest.trim_start();
            // Parse the leading signed integer literal.
            let end = rest
                .find(|c: char| c != '-' && !c.is_ascii_digit())
                .unwrap_or(rest.len());
            if rest[..end].parse::<i64>() == Ok(code) {
                return true;
            }
        }
        search = &search[idx + "\"code\"".len()..];
    }
    false
}

/// Extracts the server-selected `positionEncoding` from an `InitializeResult`.
/// Returns `None` when the server omits it (which the LSP spec defines as the
/// utf-16 default).
fn extract_position_encoding(initialize_result: &Value) -> Option<String> {
    initialize_result
        .get("capabilities")
        .and_then(|capabilities| capabilities.get("positionEncoding"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn capability_supported(capabilities: &Value, capability: &str) -> bool {
    let Some(value) = capabilities.get(capability) else {
        return false;
    };
    match value {
        Value::Bool(enabled) => *enabled,
        Value::Null => false,
        Value::Object(_) => true,
        Value::Array(items) => !items.is_empty(),
        _ => false,
    }
}

async fn initialize(
    connection: &JsonRpcConnection<ChildStdin>,
    config: &JsLanguageServerConfig,
) -> Result<Value> {
    let root_uri = path_to_uri(&config.workspace_root)?;
    let params = json!({
        "processId": std::process::id(),
        "clientInfo": { "name": "octocode-engine", "version": env!("CARGO_PKG_VERSION") },
        "locale": "en",
        "rootUri": root_uri,
        "workspaceFolders": [{ "uri": root_uri, "name": "workspace" }],
        "capabilities": {
            "general": {
                // Advertise UTF-16 ONLY. Every position octocode sends is computed
                // in UTF-16 code units (resolver::byte_offset_to_utf16) and the
                // column/snippet layers are UTF-16 too, so a server that selected
                // utf-8 would misread our offsets on any line with non-ASCII text.
                // UTF-16 is the mandatory baseline encoding, so it is always
                // supported. The server's choice is read back and asserted in
                // `start()` via `extract_position_encoding`.
                "positionEncodings": ["utf-16"]
            },
            "textDocument": {
                "definition": { "dynamicRegistration": false, "linkSupport": false },
                "references": { "dynamicRegistration": false },
                "hover": { "dynamicRegistration": false, "contentFormat": ["markdown", "plaintext"] },
                "typeDefinition": { "dynamicRegistration": false, "linkSupport": false },
                "implementation": { "dynamicRegistration": false, "linkSupport": false },
                "documentSymbol": { "dynamicRegistration": false, "hierarchicalDocumentSymbolSupport": true },
                "callHierarchy": { "dynamicRegistration": false },
                // LSP 3.17: type hierarchy — navigate supertypes (base classes/interfaces)
                // and subtypes (subclasses/implementors) without opening every file.
                "typeHierarchy": { "dynamicRegistration": false },
                // LSP 3.17: pull diagnostics — agent/CLI requests errors on demand instead
                // of receiving an unprompted push stream after every didChange.
                "diagnostic": { "dynamicRegistration": false, "relatedDocumentSupport": false },
                "synchronization": { "didSave": true, "willSave": false, "willSaveWaitUntil": false }
            },
            "workspace": {
                "configuration": true,
                "workspaceFolders": true,
                "symbol": { "dynamicRegistration": false }
            },
            "window": {
                "workDoneProgress": true
            }
        },
        "initializationOptions": config.initialization_options.clone().unwrap_or(Value::Null)
    });
    connection
        .request("initialize", params, REQUEST_TIMEOUT_MS)
        .await
}

async fn snippets_from_locations(value: Value) -> Result<Vec<JsCodeSnippet>> {
    let mut snippets = Vec::new();
    let mut content_cache = SnippetContentCache::default();
    match value {
        Value::Null => Ok(snippets),
        Value::Array(items) => {
            for item in items {
                if let Some(snippet) = snippet_from_location_like(&item, &mut content_cache).await?
                {
                    snippets.push(snippet);
                }
            }
            Ok(snippets)
        }
        object @ Value::Object(_) => {
            if let Some(snippet) = snippet_from_location_like(&object, &mut content_cache).await? {
                snippets.push(snippet);
            }
            Ok(snippets)
        }
        _ => Ok(snippets),
    }
}

#[derive(Default)]
struct SnippetContentCache {
    files: HashMap<String, String>,
}

impl SnippetContentCache {
    async fn read_range_content(&mut self, file_path: &str, range: &JsRange) -> Result<String> {
        if !self.files.contains_key(file_path) {
            let content = tokio::fs::read_to_string(file_path)
                .await
                .map_err(|err| Error::new(Status::GenericFailure, err.to_string()))?;
            self.files.insert(file_path.to_owned(), content);
        }
        Ok(slice_range_content(
            self.files
                .get(file_path)
                .map(String::as_str)
                .unwrap_or_default(),
            range,
        ))
    }
}

async fn snippet_from_location_like(
    value: &Value,
    content_cache: &mut SnippetContentCache,
) -> Result<Option<JsCodeSnippet>> {
    let uri = value
        .get("uri")
        .or_else(|| value.get("targetUri"))
        .and_then(Value::as_str);
    let range_value = value.get("range").or_else(|| value.get("targetRange"));
    let (Some(uri), Some(range_value)) = (uri, range_value) else {
        return Ok(None);
    };
    let range = parse_range(range_value)?;
    let file_path = uri_to_path(uri)?;
    // A read failure here is real evidence ("target file is missing/unreadable/
    // generated"), not "no useful definition". Surface it as explicit content
    // instead of an empty string so callers don't misread it — and keep the
    // request resilient (one bad target must not drop the other locations).
    let content = match content_cache.read_range_content(&file_path, &range).await {
        Ok(text) => text,
        Err(err) => format!("[content unavailable — could not read {uri}: {err}]"),
    };
    Ok(Some(JsCodeSnippet {
        uri: uri.to_owned(),
        range,
        content,
        symbol_kind: None,
        display_range: None,
    }))
}

fn parse_range(value: &Value) -> Result<JsRange> {
    let start = value
        .get("start")
        .ok_or_else(|| Error::new(Status::InvalidArg, "LSP range missing start"))?;
    let end = value
        .get("end")
        .ok_or_else(|| Error::new(Status::InvalidArg, "LSP range missing end"))?;
    Ok(JsRange {
        start: parse_position(start)?,
        end: parse_position(end)?,
    })
}

fn spawn_stderr_reader(
    stderr: ChildStderr,
    stderr_lines: Arc<StdMutex<VecDeque<String>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            push_stderr_line(&stderr_lines, line);
        }
    })
}

fn push_stderr_line(stderr_lines: &Arc<StdMutex<VecDeque<String>>>, line: String) {
    let Ok(mut lines) = stderr_lines.lock() else {
        return;
    };
    while lines.len() >= STDERR_RING_CAPACITY {
        lines.pop_front();
    }
    lines.push_back(truncate_stderr_line(line));
}

fn truncate_stderr_line(line: String) -> String {
    if line.chars().count() <= STDERR_LINE_MAX_CHARS {
        return line;
    }
    let mut truncated = line.chars().take(STDERR_LINE_MAX_CHARS).collect::<String>();
    truncated.push_str("...");
    truncated
}

async fn cleanup_failed_start(child: &mut Child, stderr_task: Option<JoinHandle<()>>) {
    let _ = child.kill().await;
    if let Some(task) = stderr_task {
        task.abort();
    }
}

fn parse_position(value: &Value) -> Result<JsExactPosition> {
    // Required, numeric fields. Coercing a missing/malformed line or character
    // to 0 silently mis-positions snippets and masks protocol/server corruption,
    // so validate and surface InvalidArg instead.
    let line = value
        .get("line")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::new(Status::InvalidArg, "LSP position missing numeric 'line'"))?;
    let character = value
        .get("character")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            Error::new(
                Status::InvalidArg,
                "LSP position missing numeric 'character'",
            )
        })?;
    Ok(JsExactPosition {
        line: line as u32,
        character: character as u32,
    })
}

/// Slice `content` to an LSP range, returning whole lines for snippet context.
/// LSP ranges are **end-exclusive**: a range ending at `{line: N, character: 0}`
/// stops at the end of line `N-1` and must not include line `N`. When
/// `end.character > 0` the end line is partially covered, so it is included
/// (whole-line — column truncation would shrink a single-line definition
/// snippet down to the bare identifier).
fn slice_range_content(content: &str, range: &JsRange) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start = range.start.line as usize;
    if start >= lines.len() {
        return String::new();
    }
    let end = range.end.line as usize;
    let last_inclusive = if range.end.character == 0 {
        match end.checked_sub(1) {
            Some(v) => v,
            None => return String::new(),
        }
    } else {
        end
    };
    let last_inclusive = last_inclusive.min(lines.len().saturating_sub(1));
    if last_inclusive < start {
        return String::new();
    }
    lines[start..=last_inclusive].join("\n")
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
