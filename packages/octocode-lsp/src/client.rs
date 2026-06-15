use crate::json_rpc::{ClientRequestContext, JsonRpcConnection, ProgressTracker};
use crate::types::{JsCodeSnippet, JsExactPosition, JsLanguageServerConfig, JsRange};
use crate::uri::{path_to_uri, uri_to_path};
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
    connection: Mutex<Option<JsonRpcConnection<ChildStdin>>>,
    stderr_task: Mutex<Option<JoinHandle<()>>>,
    stderr_lines: Arc<StdMutex<VecDeque<String>>>,
    capabilities: StdMutex<Option<Value>>,
    progress: Arc<ProgressTracker>,
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
            progress: ProgressTracker::new(),
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
        let connection = JsonRpcConnection::new(
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
        );
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
        Ok(())
    }

    #[napi]
    pub async fn wait_for_ready(&self, timeout_ms: Option<u32>) -> Result<()> {
        let timeout_ms = u64::from(timeout_ms.unwrap_or(45_000));
        self.progress.wait_until_idle(timeout_ms).await;
        Ok(())
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

    #[napi(js_name = "getRecentStderr")]
    pub fn get_recent_stderr(&self) -> Vec<String> {
        self.stderr_lines
            .lock()
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }

    #[napi]
    pub async fn open_document(&self, file_path: String, content: String) -> Result<()> {
        let language_id = crate::config::detect_language_id(file_path.clone())
            .or_else(|| self.config.language_id.clone())
            .unwrap_or_else(|| "plaintext".to_owned());
        let params = json!({
            "textDocument": {
                "uri": path_to_uri(&file_path)?,
                "languageId": language_id,
                "version": 1,
                "text": content
            }
        });
        let guard = self.connection.lock().await;
        let connection = guard
            .as_ref()
            .ok_or_else(|| Error::new(Status::GenericFailure, "LSP client not initialized"))?;
        connection.notify("textDocument/didOpen", params).await
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
    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let guard = self.connection.lock().await;
        let connection = guard
            .as_ref()
            .ok_or_else(|| Error::new(Status::GenericFailure, "LSP client not initialized"))?;
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

fn is_content_modified_error(error: &Error) -> bool {
    error.reason.contains("-32801") || error.reason.contains("content modified")
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
        "clientInfo": { "name": "octocode-lsp", "version": env!("CARGO_PKG_VERSION") },
        "locale": "en",
        "rootUri": root_uri,
        "workspaceFolders": [{ "uri": root_uri, "name": "workspace" }],
        "capabilities": {
            "textDocument": {
                "definition": { "dynamicRegistration": false, "linkSupport": false },
                "references": { "dynamicRegistration": false },
                "hover": { "dynamicRegistration": false, "contentFormat": ["markdown", "plaintext"] },
                "typeDefinition": { "dynamicRegistration": false, "linkSupport": false },
                "implementation": { "dynamicRegistration": false, "linkSupport": false },
                "documentSymbol": { "dynamicRegistration": false, "hierarchicalDocumentSymbolSupport": true },
                "callHierarchy": { "dynamicRegistration": false },
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
    let content = content_cache
        .read_range_content(&file_path, &range)
        .await
        .unwrap_or_default();
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
    Ok(JsExactPosition {
        line: value.get("line").and_then(Value::as_u64).unwrap_or(0) as u32,
        character: value.get("character").and_then(Value::as_u64).unwrap_or(0) as u32,
    })
}

fn slice_range_content(content: &str, range: &JsRange) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start = range.start.line as usize;
    let end = range.end.line as usize;
    if start >= lines.len() {
        return String::new();
    }
    let end_inclusive = end.min(lines.len().saturating_sub(1));
    lines[start..=end_inclusive].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn stderr_ring_keeps_only_recent_lines() {
        let lines = Arc::new(StdMutex::new(VecDeque::new()));

        for index in 0..(STDERR_RING_CAPACITY + 5) {
            push_stderr_line(&lines, format!("line-{index}"));
        }

        let lines = lines.lock().expect("stderr ring lock");
        assert_eq!(lines.len(), STDERR_RING_CAPACITY);
        assert_eq!(lines.front().map(String::as_str), Some("line-5"));
        assert_eq!(
            lines.back().map(String::as_str),
            Some(format!("line-{}", STDERR_RING_CAPACITY + 4).as_str())
        );
    }

    #[test]
    fn stderr_ring_truncates_very_long_lines() {
        let lines = Arc::new(StdMutex::new(VecDeque::new()));

        push_stderr_line(&lines, "x".repeat(STDERR_LINE_MAX_CHARS + 10));

        let line = lines
            .lock()
            .expect("stderr ring lock")
            .front()
            .cloned()
            .expect("stderr line");
        assert_eq!(line.chars().count(), STDERR_LINE_MAX_CHARS + 3);
        assert!(line.ends_with("..."));
    }

    #[test]
    fn snippet_content_cache_reuses_file_content_for_later_ranges() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let file_path = temp_file("octocode-lsp-snippet-cache");
            std::fs::write(&file_path, "alpha\nbeta\ngamma\n").expect("write fixture");
            let file_path = file_path.to_string_lossy().into_owned();
            let mut cache = SnippetContentCache::default();

            let first = cache
                .read_range_content(&file_path, &range(0, 0))
                .await
                .expect("first range");
            std::fs::remove_file(&file_path).expect("remove fixture");
            let second = cache
                .read_range_content(&file_path, &range(1, 2))
                .await
                .expect("second range");

            assert_eq!(first, "alpha");
            assert_eq!(second, "beta\ngamma");
            assert_eq!(cache.files.len(), 1);
        });
    }

    fn range(start_line: u32, end_line: u32) -> JsRange {
        JsRange {
            start: JsExactPosition {
                line: start_line,
                character: 0,
            },
            end: JsExactPosition {
                line: end_line,
                character: 0,
            },
        }
    }

    fn temp_file(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("{name}-{}-{nanos}", std::process::id()))
    }
}
