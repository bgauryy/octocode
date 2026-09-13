use super::{ExecutionContext, ExecutionError, RequestRuntime, RuntimeLimits, response};
use crate::config::{self, ConfigInput, ConfigOutput, RuntimeSurface};
use crate::contracts::{self, PrepareOptions};
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::regex::{IsolatedRegexEngine, IsolatedRegexLimits};
use crate::response::{
    PreparedResponse, ResponseInput, ResponsePageOptions, ResponsePager, ResponsePagerConfig,
    TextContent,
};
use crate::security::{ContentSecurity, SecurityRegistry};
use crate::tools::local_fetch::{
    CancellationCheck, LocalFetchRegex, LocalFetchRequest, execute_local_fetch_with_regex,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostOptions {
    pub cwd: Option<PathBuf>,
    pub regex_worker_path: Option<PathBuf>,
    #[serde(default)]
    pub trusted_project: bool,
    #[serde(default)]
    pub surface: RuntimeSurface,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}
impl RuntimeError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            payload: None,
        }
    }
}

pub struct ToolRuntime {
    pub requests: RequestRuntime,
    input: ConfigInput,
    config: Arc<ConfigOutput>,
    paths: Arc<PathPolicy>,
    security: Arc<ContentSecurity>,
    regex: Option<Arc<IsolatedRegexEngine>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    NotFound,
    Execution,
}

#[derive(Debug)]
pub struct ToolOutcome {
    pub structured_content: Value,
    pub content: Vec<TextContent>,
    pub source_digests: Vec<Option<String>>,
    pub failure: Option<FailureKind>,
    pub all_failed: bool,
}

impl ToolRuntime {
    pub fn from_host(options: HostOptions) -> Result<Self, RuntimeError> {
        let cwd = options
            .cwd
            .or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| RuntimeError::new("config", "Cannot resolve working directory"))?;
        let home = std::env::home_dir()
            .ok_or_else(|| RuntimeError::new("config", "Cannot resolve home directory"))?;
        let input = config::acquire_config_input(
            std::env::vars().collect(),
            cwd,
            home,
            options.trusted_project,
            options.surface,
            1,
        );
        let mut runtime = Self::new(input)?;
        let worker_path = options.regex_worker_path.or_else(|| {
            (options.surface == RuntimeSurface::Cli)
                .then(|| std::env::current_exe().ok())
                .flatten()
                .and_then(|path| {
                    path.parent().map(|parent| {
                        parent.join(if cfg!(windows) {
                            "octocode-regex-worker.exe"
                        } else {
                            "octocode-regex-worker"
                        })
                    })
                })
        });
        if let Some(path) = worker_path {
            runtime.regex = Some(Arc::new(IsolatedRegexEngine::new(
                path,
                IsolatedRegexLimits::default(),
            )));
        }
        Ok(runtime)
    }

    pub fn new(input: ConfigInput) -> Result<Self, RuntimeError> {
        let config = Arc::new(config::resolve_config(&input));
        let local = &config.resolved.local;
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: local.workspace_root.as_ref().map(PathBuf::from),
            additional_roots: local.allowed_paths.iter().map(PathBuf::from).collect(),
            include_home: true,
            home_dir: Some(input.os_home.clone()),
        })
        .map_err(|error| RuntimeError::new("policy", error.message))?;
        let mut registry = SecurityRegistry::default();
        registry.freeze();
        let security = ContentSecurity::new(Arc::new(registry));
        let requests = RequestRuntime::new(RuntimeLimits::default())
            .map_err(|e| RuntimeError::new("runtime", format!("{e:?}")))?;
        Ok(Self {
            requests,
            input,
            config,
            paths: Arc::new(paths),
            security: Arc::new(security),
            regex: None,
        })
    }

    pub fn config(&self) -> &ConfigOutput {
        &self.config
    }
    pub fn begin_close(&self) {
        if let Some(regex) = &self.regex {
            regex.shutdown();
        }
        self.requests.begin_close();
    }
    pub async fn close(&self) {
        self.begin_close();
        self.requests.close().await;
    }
    pub fn inspect_config(&self) -> config::ConfigInspectorData {
        config::inspector_data(&self.input, &self.config)
    }

    fn cursor_scope(&self) -> Result<String, RuntimeError> {
        super::cursor::scope_digest(&json!({"cwd":self.input.cwd,"home":self.inspect_config().home,"osHome":self.input.os_home,"config":self.config.resolved}))
            .map_err(|error| RuntimeError::new("invalidCursor", format!("{error:?}")))
    }

    pub fn continuation_token(
        &self,
        call: &Value,
        source_sha256: &str,
    ) -> Result<String, RuntimeError> {
        if call["tool"] != "localFetch" {
            return Err(RuntimeError::new(
                "invalidCursor",
                "Unsupported continuation tool",
            ));
        }
        let query = call
            .get("query")
            .filter(|v| v.is_object())
            .ok_or_else(|| RuntimeError::new("invalidCursor", "Missing continuation query"))?;
        let path = query["path"]
            .as_str()
            .ok_or_else(|| RuntimeError::new("invalidCursor", "Missing continuation path"))?;
        let validated = self
            .paths
            .validate_read(path)
            .map_err(|error| RuntimeError::new("invalidCursor", error.message))?;
        super::cursor::ReadCursor::create(
            query.clone(),
            self.cursor_scope()?,
            &validated.canonical,
            source_sha256,
        )
        .map_err(|error| RuntimeError::new("invalidCursor", format!("{error:?}")))
    }

    pub fn resume_token(&self, token: &str) -> Result<(String, Value, String), RuntimeError> {
        let cursor = super::cursor::ReadCursor::decode(token, &self.cursor_scope()?)
            .map_err(|error| RuntimeError::new("invalidCursor", format!("{error:?}")))?;
        let path = cursor.query["path"]
            .as_str()
            .ok_or_else(|| RuntimeError::new("invalidCursor", "Missing continuation path"))?;
        let validated = self
            .paths
            .validate_read(path)
            .map_err(|error| RuntimeError::new("invalidCursor", error.message))?;
        cursor
            .verify_source(&validated.canonical)
            .map_err(|error| RuntimeError::new("staleCursor", format!("{error:?}")))?;
        Ok((cursor.tool, cursor.query, cursor.source_sha256))
    }

    pub fn is_available(&self, tool: &str) -> bool {
        tool == "localFetch"
            && self.config.resolved.local.enabled
            && self
                .config
                .resolved
                .tools
                .enabled
                .as_ref()
                .is_none_or(|names| names.iter().any(|name| name == tool))
            && !self
                .config
                .resolved
                .tools
                .disabled
                .as_ref()
                .is_some_and(|names| names.iter().any(|name| name == tool))
    }

    pub fn catalog(&self) -> Result<Value, RuntimeError> {
        let mut catalog: Value = serde_json::from_str(contracts::contract_json())
            .map_err(|_| RuntimeError::new("contract", "Embedded contract is invalid"))?;
        if let Some(tools) = catalog.get_mut("tools").and_then(Value::as_array_mut) {
            for tool in tools {
                let available = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| self.is_available(name));
                tool["available"] = json!(available);
            }
        }
        Ok(catalog)
    }

    pub async fn execute(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> Result<ToolOutcome, RuntimeError> {
        self.execute_channel(request_id, tool, input, false).await
    }

    pub async fn execute_mcp(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> Result<Value, RuntimeError> {
        let result = match self
            .execute_channel(request_id, tool.clone(), input.clone(), true)
            .await
        {
            Ok(result) => result,
            Err(error) => return super::error::mcp_input_error(&tool, &input, &error).ok_or(error),
        };
        serde_json::to_value(PreparedResponse {
            content: result.content,
            structured_content: result.structured_content,
            is_error: result.all_failed,
        })
        .map_err(|_| RuntimeError::new("response", "Cannot serialize response"))
    }

    async fn execute_channel(
        &self,
        request_id: String,
        tool: String,
        input: Value,
        mcp: bool,
    ) -> Result<ToolOutcome, RuntimeError> {
        if !self.is_available(&tool) {
            return Err(RuntimeError::new(
                "toolUnavailable",
                format!("Tool {tool} is not available in this native runtime"),
            ));
        }
        let prepared = contracts::prepare_and_validate(&tool, input, PrepareOptions::default())
            .map_err(|error| RuntimeError {
                code: "invalidInput".into(),
                message: error.to_string(),
                payload: Some(contracts::format_input_error(&tool, &error)),
            })?;
        let checked = self.security.validate_input_parameters(&prepared);
        if !checked.is_valid {
            return Err(RuntimeError::new(
                "securityValidationFailed",
                format!(
                    "Security validation failed: {}",
                    checked.warnings.join("; ")
                ),
            ));
        }
        let prepared = Value::Object(checked.sanitized_params);
        let options: ResponsePageOptions = serde_json::from_value(prepared.clone())
            .map_err(|_| RuntimeError::new("invalidInput", "Invalid response options"))?;
        let queries = prepared
            .get("queries")
            .and_then(Value::as_array)
            .ok_or_else(|| RuntimeError::new("invalidInput", "Expected queries"))?
            .clone();
        let paths = self.paths.clone();
        let security = self.security.clone();
        let regex = LocalFetchRegex::new(self.regex.clone());
        self.requests
            .execute_blocking(request_id, move |context| {
                let mut rows = Vec::with_capacity(queries.len());
                let mut source_digests = Vec::with_capacity(queries.len());
                let mut failure = None;
                for (index, query) in queries.iter().enumerate() {
                    context.check()?;
                    let mut domain_query = query.clone();
                    if let Some(object) = domain_query.as_object_mut() {
                        object.remove("goal");
                        object.remove("reasoning");
                    }
                    let request: LocalFetchRequest = serde_json::from_value(domain_query)
                        .map_err(|_| ExecutionError::WorkerFailed)?;
                    let result = execute_local_fetch_with_regex(
                        &request,
                        paths.as_ref(),
                        security.as_ref(),
                        &context,
                        &regex,
                    );
                    let status = match result.status.as_str() {
                        "error" => Some("error"),
                        "empty" => Some("empty"),
                        _ => None,
                    };
                    source_digests.push(result.source_sha256.clone());
                    if status == Some("error") {
                        failure = Some(
                            if result.resource_missing && failure != Some(FailureKind::Execution) {
                                FailureKind::NotFound
                            } else {
                                FailureKind::Execution
                            },
                        );
                    }
                    let mut data =
                        serde_json::to_value(&result).map_err(|_| ExecutionError::WorkerFailed)?;
                    if status != Some("error")
                        && let Some(kind) = crate::content::classify_file_type(&result.path)
                    {
                        use crate::content::FileType;
                        data["fileType"] = json!(match kind {
                            FileType::Code => "code",
                            FileType::Config => "config",
                            FileType::Lock => "lock",
                            FileType::Doc => "doc",
                        });
                    }
                    rows.push(response::result_row(&tool, index, query, data, status));
                }
                let all_failed =
                    !rows.is_empty() && rows.iter().all(|row| row["status"] == "error");
                let structured = response::envelope(rows);
                context.check()?;
                let render = options.render_text.unwrap_or(mcp)
                    || failure.is_some()
                    || options.response_char_length.is_some()
                    || options.response_char_offset.is_some()
                    || options.response_snapshot.is_some();
                let rendered_text = render.then(|| super::render::render_local_fetch(&structured));
                context.check()?;
                let prepared = ResponsePager::new(ResponsePagerConfig::default())
                    .prepare(
                        ResponseInput {
                            tool,
                            queries,
                            structured,
                            rendered_text,
                            is_error: all_failed,
                            options,
                        },
                        &std::sync::atomic::AtomicBool::new(context.cancellation.is_cancelled()),
                    )
                    .map_err(|_| ExecutionError::WorkerFailed)?;
                context.check()?;
                Ok(ToolOutcome {
                    structured_content: prepared.structured_content,
                    content: prepared.content,
                    source_digests,
                    failure,
                    all_failed,
                })
            })
            .await
            .map_err(|error| {
                RuntimeError::new(
                    match error {
                        ExecutionError::Closed => "closed",
                        ExecutionError::Busy => "busy",
                        ExecutionError::DuplicateRequest => "duplicateRequest",
                        ExecutionError::Cancelled => "cancelled",
                        ExecutionError::Timeout => "timeout",
                        _ => "executionFailed",
                    },
                    format!("{error:?}"),
                )
            })
    }
}

impl CancellationCheck for ExecutionContext {
    fn check(&self) -> Result<(), String> {
        ExecutionContext::check(self).map_err(|error| format!("{error:?}"))
    }
}
