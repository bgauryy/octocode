use crate::json_rpc::{ClientRequestContext, JsonRpcConnection};
use crate::types::{JsCodeSnippet, JsExactPosition, JsLanguageServerConfig, JsRange};
use crate::uri::{path_to_uri, uri_to_path};
use napi::{Error, Result, Status};
use napi_derive::napi;
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::Mutex as StdMutex;
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;

const REQUEST_TIMEOUT_MS: u32 = 30_000;
const CONTENT_MODIFIED_RETRIES: u8 = 3;
const CONTENT_MODIFIED_RETRY_DELAY_MS: u64 = 500;

#[napi]
pub struct NativeLspClient {
    config: JsLanguageServerConfig,
    child: Mutex<Option<Child>>,
    connection: Mutex<Option<JsonRpcConnection<ChildStdin>>>,
    capabilities: StdMutex<Option<Value>>,
}

#[napi]
impl NativeLspClient {
    #[napi(constructor)]
    pub fn new(config: JsLanguageServerConfig) -> Self {
        Self {
            config,
            child: Mutex::new(None),
            connection: Mutex::new(None),
            capabilities: StdMutex::new(None),
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

        let mut command = tokio::process::Command::new(&self.config.command);
        command
            .args(self.config.args.clone().unwrap_or_default())
            .current_dir(&self.config.workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|err| {
            Error::new(
                Status::GenericFailure,
                format!("Failed to start language server: {err}"),
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            Error::new(
                Status::GenericFailure,
                "Language server stdout pipe missing",
            )
        })?;
        let stdin = child.stdin.take().ok_or_else(|| {
            Error::new(Status::GenericFailure, "Language server stdin pipe missing")
        })?;

        let root_uri = path_to_uri(&self.config.workspace_root)?;
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
        );
        let initialize_result = initialize(&connection, &self.config).await?;
        if let Ok(mut capabilities) = self.capabilities.lock() {
            *capabilities = initialize_result.get("capabilities").cloned();
        }
        connection.notify("initialized", json!({})).await?;

        *self.connection.lock().await = Some(connection);
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
        Ok(())
    }

    #[napi]
    pub async fn wait_for_ready(&self, timeout_ms: Option<u32>) -> Result<()> {
        let delay_ms = timeout_ms.unwrap_or(1_000).min(1_000);
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(u64::from(delay_ms))).await;
        }
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
    match value {
        Value::Null => Ok(snippets),
        Value::Array(items) => {
            for item in items {
                if let Some(snippet) = snippet_from_location_like(&item).await? {
                    snippets.push(snippet);
                }
            }
            Ok(snippets)
        }
        object @ Value::Object(_) => {
            if let Some(snippet) = snippet_from_location_like(&object).await? {
                snippets.push(snippet);
            }
            Ok(snippets)
        }
        _ => Ok(snippets),
    }
}

async fn snippet_from_location_like(value: &Value) -> Result<Option<JsCodeSnippet>> {
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
    let content = read_range_content(&file_path, &range)
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

fn parse_position(value: &Value) -> Result<JsExactPosition> {
    Ok(JsExactPosition {
        line: value.get("line").and_then(Value::as_u64).unwrap_or(0) as u32,
        character: value.get("character").and_then(Value::as_u64).unwrap_or(0) as u32,
    })
}

async fn read_range_content(file_path: &str, range: &JsRange) -> Result<String> {
    let content = tokio::fs::read_to_string(file_path)
        .await
        .map_err(|err| Error::new(Status::GenericFailure, err.to_string()))?;
    let lines: Vec<&str> = content.lines().collect();
    let start = range.start.line as usize;
    let end = range.end.line as usize;
    if start >= lines.len() {
        return Ok(String::new());
    }
    let end_inclusive = end.min(lines.len().saturating_sub(1));
    Ok(lines[start..=end_inclusive].join("\n"))
}
