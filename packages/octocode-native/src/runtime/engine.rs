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
use crate::tools::local_fetch::{CancellationCheck, LocalFetchRegex};

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
    /// Override the per-request execution timeout in seconds (default: 60).
    /// CLI sets 120 to accommodate LSP cold-start initialization.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Box<Value>>,
    #[serde(skip)]
    pub validation_issues: Option<Vec<contracts::ValidationIssue>>,
}
impl RuntimeError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            payload: None,
            validation_issues: None,
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
    github_cache: super::github_cache::GitHubContentCache,
    github_services: Arc<
        std::sync::OnceLock<
            Result<super::github::GitHubServices, crate::providers::github::ProviderError>,
        >,
    >,
    lsp_pool: Arc<octocode_engine::lsp::pool::LspClientPool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    NotFound,
    Authentication,
    Permission,
    RateLimited,
    Execution,
}

#[derive(Debug)]
pub struct ToolOutcome {
    pub structured_content: Value,
    pub content: Vec<TextContent>,
    pub source_digest: Option<String>,
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
        super::maintenance::run_if_due(&runtime.inspect_config().home);
        if let Some(secs) = options.timeout_secs {
            // Replace the default 60-second runtime with the caller-specified timeout.
            // The CLI uses 120 s so LSP cold-start initialisation (which can take ~60 s)
            // completes before the execution context deadline fires.
            runtime.requests = RequestRuntime::new(RuntimeLimits {
                timeout: std::time::Duration::from_secs(secs),
                ..RuntimeLimits::default()
            })
            .map_err(|e| RuntimeError::new("runtime", format!("{e:?}")))?;
        }
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
        let octocode_home = config::octocode_home(&input.env, &input.cwd, &input.os_home);
        let mut additional_roots: Vec<PathBuf> =
            local.allowed_paths.iter().map(PathBuf::from).collect();
        additional_roots.push(octocode_home.clone());
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: local.workspace_root.as_ref().map(PathBuf::from),
            additional_roots,
            include_home: true,
            home_dir: Some(input.os_home.clone()),
        })
        .map_err(|error| RuntimeError::new("policy", error.message))?;
        let mut registry = SecurityRegistry::default();
        registry.freeze();
        let security = ContentSecurity::new(Arc::new(registry));
        let requests = RequestRuntime::new(RuntimeLimits::default())
            .map_err(|e| RuntimeError::new("runtime", format!("{e:?}")))?;
        let github_cache = super::github_cache::GitHubContentCache::new(
            crate::cache::CacheConfig::default(),
            config.revision,
            config::is_persistent_storage_enabled(&config.resolved)
                .then_some(octocode_home.join("tmp").join("response")),
        );
        Ok(Self {
            requests,
            input,
            config,
            paths: Arc::new(paths),
            security: Arc::new(security),
            regex: None,
            github_cache,
            github_services: Arc::new(std::sync::OnceLock::new()),
            lsp_pool: Arc::new(octocode_engine::lsp::pool::LspClientPool::default()),
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
        self.lsp_pool.clear_all().await;
        self.github_cache.clear();
    }
    pub fn inspect_config(&self) -> config::ConfigInspectorData {
        config::inspector_data(&self.input, &self.config)
    }

    pub fn clear_github_cache(&self) {
        self.github_cache.clear();
    }

    fn cursor_scope(&self) -> Result<String, RuntimeError> {
        super::cursor::scope_digest(&json!({"cwd":self.input.cwd,"home":self.inspect_config().home,"osHome":self.input.os_home,"config":self.config.resolved}))
            .map_err(|error| RuntimeError::new("invalidCursor", format!("{error:?}")))
    }

    pub fn continuation_token(
        &self,
        call: &Value,
        source_sha256: Option<&str>,
    ) -> Result<String, RuntimeError> {
        let tool = call["tool"].as_str().unwrap_or("");
        let query = call
            .get("query")
            .filter(|v| v.is_object())
            .ok_or_else(|| RuntimeError::new("invalidCursor", "Missing continuation query"))?;
        if !self.is_available(tool) {
            return Err(RuntimeError::new(
                "invalidCursor",
                "Continuation names an unavailable tool",
            ));
        }
        if tool == "localSearch" {
            return super::cursor::ReadCursor::create_search(query.clone(), self.cursor_scope()?)
                .map_err(|error| RuntimeError::new("invalidCursor", format!("{error:?}")));
        }
        if tool == "localFetch" {
            let path = query["path"]
                .as_str()
                .ok_or_else(|| RuntimeError::new("invalidCursor", "Missing continuation path"))?;
            let validated = self
                .paths
                .validate_read(path)
                .map_err(|error| RuntimeError::new("invalidCursor", error.message))?;
            return super::cursor::ReadCursor::create(
                query.clone(),
                self.cursor_scope()?,
                &validated.canonical,
                source_sha256,
            )
            .map_err(|error| RuntimeError::new("invalidCursor", format!("{error:?}")));
        }
        super::cursor::UniversalCursor::create(tool, query.clone(), self.cursor_scope()?)
            .map_err(|error| RuntimeError::new("invalidCursor", format!("{error:?}")))
    }

    /// Returns `(tool, query, source_digest)` where `source_digest` is `Some`
    /// only for local-file tools (localFetch/localSearch). The returned query
    /// remains untrusted and must re-enter normal contract and security validation.
    pub fn resume_token(
        &self,
        token: &str,
    ) -> Result<(String, Value, Option<String>), RuntimeError> {
        let scope = self.cursor_scope()?;
        if let Ok(cursor) = super::cursor::ReadCursor::decode(token, &scope) {
            let path = cursor.query["path"]
                .as_str()
                .ok_or_else(|| RuntimeError::new("invalidCursor", "Missing continuation path"))?;
            let validated = if cursor.tool == "localFetch" {
                self.paths.validate_read(path)
            } else {
                self.paths.validate(path)
            }
            .map_err(|error| RuntimeError::new("invalidCursor", error.message))?;
            if cursor.tool == "localFetch" {
                cursor
                    .verify_source(&validated.canonical)
                    .map_err(|error| RuntimeError::new("staleCursor", format!("{error:?}")))?;
            }
            return Ok((cursor.tool, cursor.query, Some(cursor.source_sha256)));
        }
        let cursor = super::cursor::UniversalCursor::decode(token, &scope)
            .map_err(|error| RuntimeError::new("invalidCursor", format!("{error:?}")))?;
        Ok((cursor.tool, cursor.query, None))
    }

    pub fn is_available(&self, tool: &str) -> bool {
        let local = self.config.resolved.local.enabled;
        let clone = self.config.resolved.local.enable_clone
            && self.config.resolved.storage.mode == "persistent";
        let github = matches!(
            tool,
            "ghGetFileContent" | "ghGetHistoryItem" | "ghSearch" | "ghSearchHistory"
        );
        let local_tools = local
            && matches!(
                tool,
                "localFetch" | "localSearch" | "astSearch" | "astRewrite" | "lspSearch"
            );
        (github || local_tools || (tool == "ghCloneRepo" && clone) || tool == "artifactSearch")
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
        let contract = contracts::parsed_contract()
            .map_err(|_| RuntimeError::new("contract", "Embedded contract is invalid"))?;
        let instructions = contracts::mcp_instructions(contract, |name| self.is_available(name))
            .map_err(|message| RuntimeError::new("contract", message))?;
        let fields = contract
            .as_object()
            .ok_or_else(|| RuntimeError::new("contract", "Embedded catalog is invalid"))?;
        let mut catalog = Value::Object(
            fields
                .iter()
                .filter(|(key, _)| key.as_str() != "mcpInstructionTable")
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        );
        catalog["mcpInstructions"] = Value::String(instructions);
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
        let admission = self.admit(request_id)?;
        self.execute_admitted(admission, tool, input).await
    }

    pub fn admit(&self, request_id: String) -> Result<super::RequestAdmission, RuntimeError> {
        self.requests
            .admit(request_id)
            .map_err(runtime_execution_error)
    }

    pub async fn execute_admitted(
        &self,
        admission: super::RequestAdmission,
        tool: String,
        input: Value,
    ) -> Result<ToolOutcome, RuntimeError> {
        self.execute_channel(admission, tool, input, false).await
    }

    pub async fn execute_mcp(
        &self,
        request_id: String,
        tool: String,
        input: Value,
    ) -> Result<Value, RuntimeError> {
        let admission = self.admit(request_id)?;
        self.execute_mcp_admitted(admission, tool, input).await
    }

    pub async fn execute_mcp_admitted(
        &self,
        admission: super::RequestAdmission,
        tool: String,
        input: Value,
    ) -> Result<Value, RuntimeError> {
        if let Some(result) = super::error::mcp_envelope_error(&tool, &input) {
            return Ok(result);
        }
        let result = match self
            .execute_channel(admission, tool.clone(), input.clone(), true)
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
        admission: super::RequestAdmission,
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
        // Parse response-paging options from the raw input before contract
        // validation; they are envelope-level fields, not query-schema fields.
        let options: ResponsePageOptions =
            serde_json::from_value(input.clone()).unwrap_or_default();
        let query = contracts::prepare_and_validate(&tool, input, PrepareOptions::default())
            .map_err(|error| RuntimeError {
                code: "invalidInput".into(),
                message: error.to_string(),
                payload: Some(Box::new(contracts::format_input_error(&tool, &error))),
                validation_issues: Some(error.issues),
            })?;
        let checked = self.security.validate_input_parameters(&query);
        if !checked.is_valid {
            return Err(RuntimeError::new(
                "securityValidationFailed",
                format!(
                    "Security validation failed: {}",
                    checked.warnings.join("; ")
                ),
            ));
        }
        let query = Value::Object(checked.sanitized_params);
        let paths = self.paths.clone();
        let security = self.security.clone();
        let regex = LocalFetchRegex::new(self.regex.clone());
        let github_services = self.github_services.clone();
        let github_cache = self.github_cache.clone();
        let config = self.config.clone();
        let home = self.inspect_config().home;
        let handle = tokio::runtime::Handle::current();
        let allow_ast_rewrite_apply = self.config.resolved.local.enable_ast_rewrite_apply;
        let lsp_pool = self.lsp_pool.clone();
        let lsp_execution_config = crate::tools::lsp_search::LspExecutionConfig {
            config_path: self.config.resolved.lsp.config_path.clone(),
            trust_project_config: self.input.trusted_project,
        };
        let output_tool = tool.clone();
        let outcome = self
            .requests
            .execute_blocking_admitted(admission, move |context| {
                // Execute the single query.
                let result = if matches!(
                    tool.as_str(),
                    "ghGetFileContent"
                        | "ghGetHistoryItem"
                        | "ghSearch"
                        | "ghSearchHistory"
                        | "ghCloneRepo"
                ) {
                    let _enter = handle.enter();
                    match github_services.get_or_init(|| {
                        super::github::GitHubServices::new(
                            config.clone(),
                            home.clone(),
                            github_cache.clone(),
                        )
                    }) {
                        Ok(services) => services.execute_query(
                            &tool, &query, &context, &security, &regex, &handle, &paths,
                        )?,
                        Err(error) => super::github::provider_error(error.clone()),
                    }
                } else if tool == "artifactSearch" {
                    let _enter = handle.enter();
                    context.check()?;
                    handle.block_on(async {
                        match crate::tools::artifact_search::execute(
                            &query,
                            context.deadline,
                            context.cancellation.clone(),
                        )
                        .await
                        {
                            Ok(data) => super::dispatch::value_result(data),
                            Err(error) => super::dispatch::provider_failure(
                                error.message,
                                error.code,
                                error.hints,
                            ),
                        }
                    })
                } else if tool == "lspSearch" {
                    let _enter = handle.enter();
                    context.check()?;
                    handle.block_on(async {
                        match crate::tools::lsp_search::execute(
                            query.clone(),
                            &context,
                            &lsp_pool,
                            &paths,
                            &lsp_execution_config,
                        )
                        .await
                        {
                            Ok(data) => super::dispatch::value_result(data),
                            Err(message) => super::dispatch::provider_failure(
                                message,
                                "lspUnavailable".into(),
                                vec!["Use localSearch or astSearch, then localFetch.".into()],
                            ),
                        }
                    })
                } else {
                    let _enter = handle.enter();
                    context.check()?;
                    handle.block_on(async {
                        let tool = tool.clone();
                        let paths = paths.clone();
                        let security = security.clone();
                        let regex = regex.clone();
                        let context = context.clone();
                        let query = query.clone();
                        tokio::task::spawn_blocking(move || {
                            super::dispatch::execute_local(
                                &tool,
                                &query,
                                &paths,
                                &security,
                                &context,
                                &regex,
                                allow_ast_rewrite_apply,
                            )
                        })
                        .await
                        .map_err(|_| ExecutionError::WorkerFailed)?
                    })?
                };
                context.check()?;
                let source_digest = result.source_digest;
                let failure = result.failure;
                let mut row = response::result_row(&tool, 0, &query, result.data, result.status);
                response::attach_diagnostics(&mut row, result.diagnostics);
                if result.cache {
                    row["cache"] = json!(1);
                }
                response::apply_hint_policy(&mut row, &tool, &query);
                let all_failed = row["status"] == "error";
                let mut structured = response::envelope(vec![row]);
                response::sanitize_fields(&mut structured, &security, &context)?;
                context.check()?;
                let render = options.render_text.unwrap_or(mcp)
                    || failure.is_some()
                    || options.response_char_length.is_some()
                    || options.response_char_offset.is_some()
                    || options.response_snapshot.is_some();
                let rendered_text =
                    render.then(|| super::render::render_tool(&tool, &structured, &query));
                context.check()?;
                let prepared = ResponsePager::new(ResponsePagerConfig::default())
                    .prepare(
                        ResponseInput {
                            tool,
                            query,
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
                    source_digest,
                    failure,
                    all_failed,
                })
            })
            .await
            .map_err(runtime_execution_error)?;
        contracts::validate_output(&output_tool, &outcome.structured_content).map_err(|error| {
            let details = error
                .issues
                .iter()
                .map(|issue| format!("{}: {}", issue.path.join("."), issue.message))
                .collect::<Vec<_>>()
                .join("; ");
            RuntimeError {
                code: "outputContractViolation".into(),
                message: format!(
                    "{output_tool} produced a response that violates its canonical output contract: {details}"
                ),
                payload: None,
                validation_issues: Some(error.issues),
            }
        })?;
        Ok(outcome)
    }
}

fn runtime_execution_error(error: ExecutionError) -> RuntimeError {
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
}

impl CancellationCheck for ExecutionContext {
    fn check(&self) -> Result<(), String> {
        ExecutionContext::check(self).map_err(|error| format!("{error:?}"))
    }
}
