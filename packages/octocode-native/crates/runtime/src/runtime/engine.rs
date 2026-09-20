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
use crate::tools::id::{ToolFamily, ToolId};
use crate::tools::local_fetch::{CancellationCheck, LocalFetchRegex};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
        // Default sandbox: the configured workspace, or the process cwd when
        // unconfigured — never all of $HOME. octocode_home stays an additional
        // root for cache. Broad $HOME access is opt-in only, via an explicit
        // workspaceRoot or an allowedPaths entry.
        let workspace_root = local
            .workspace_root
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| input.cwd.clone());
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace_root),
            additional_roots,
            include_home: false,
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
        self.github_cache.clear_memory();
    }
    pub fn inspect_config(&self) -> config::ConfigInspectorData {
        config::inspector_data(&self.input, &self.config)
    }

    pub fn clear_github_cache(&self) {
        self.github_cache.clear();
    }

    /// Scope digest partitioned by tool family so that toggling local-only
    /// config (e.g. `enable_clone`) does not invalidate GitHub/remote cursors.
    fn cursor_scope_for(&self, tool: &str) -> Result<String, RuntimeError> {
        let home = self.inspect_config().home;
        let os_home = &self.input.os_home;
        // An empty tool string is treated as a local scope (cursor resume paths
        // derive scope before the tool is decoded); unknown tools fall back to
        // the narrowest remote scope.
        let family = match ToolId::from_name(tool) {
            Some(id) => id.family(),
            None if tool.is_empty() => ToolFamily::Local,
            None => ToolFamily::Remote,
        };
        let scope_value = match family {
            ToolFamily::Local => {
                // Local tools need cwd, allowed paths, and LSP config.
                json!({
                    "cwd": self.input.cwd,
                    "home": home,
                    "osHome": os_home,
                    "local": self.config.resolved.local,
                    "lsp": self.config.resolved.lsp,
                })
            }
            ToolFamily::GitHub => {
                // GitHub tools: scoped to API endpoint + home; not local config.
                json!({
                    "home": home,
                    "osHome": os_home,
                    "github": self.config.resolved.github,
                })
            }
            ToolFamily::Remote => {
                // Artifact and other remote tools: home only.
                json!({"home": home, "osHome": os_home})
            }
        };
        super::cursor::scope_digest(&scope_value)
            .map_err(|error| RuntimeError::new("invalidCursor", format!("{error:?}")))
    }

    pub fn is_available(&self, tool: &str) -> bool {
        let local = self.config.resolved.local.enabled;
        let clone = self.config.resolved.local.enable_clone
            && self.config.resolved.storage.mode == "persistent";
        let id = ToolId::from_name(tool);
        // GitHub read tools are always enabled; cloning has an extra gate below.
        let github = matches!(id, Some(t) if t.is_github() && t != ToolId::GhCloneRepo);
        let local_tools = local && matches!(id, Some(t) if t.is_local());
        let jev = matches!(id, Some(t) if t.is_jev())
            && self
                .config
                .env_value("OCTOCODE_JEV_KEY")
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        (github
            || local_tools
            || (id == Some(ToolId::GhCloneRepo) && clone)
            || id == Some(ToolId::ArtifactSearch)
            || jev)
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
        // Cursor shortcut: { cursor: "<token>" } resumes any previous page.
        // Decoded queries skip prepare_and_validate (already validated when the
        // token was minted; null placeholders like artifactSearch registry would
        // fail re-validation). Skipping is safe only because the token is
        // HMAC-authenticated under a per-process key (see cursor.rs), so a caller
        // cannot forge a query that bypasses contract validation; the security
        // input gate (validate_input_parameters) still runs on the decoded query
        // below. Scope is computed before decoding; for cursor inputs the tool is
        // unknown until decoded, so we derive scope from the original tool arg.
        let scope = self.cursor_scope_for(&tool)?;
        let (tool, input, from_cursor) = match input
            .as_object()
            .filter(|m| m.len() == 1 && m.contains_key("cursor"))
            .and_then(|m| m["cursor"].as_str())
        {
            Some(tok) => {
                let (dt, dq) = super::cursor::UniversalCursor::decode(tok, &scope)
                    .map(|c| (c.tool, c.query))
                    .or_else(|_| {
                        super::cursor::ReadCursor::decode(tok, &scope).map(|c| (c.tool, c.query))
                    })
                    .map_err(|e| RuntimeError::new("invalidCursor", format!("{e:?}")))?;
                if !self.is_available(&dt) {
                    return Err(RuntimeError::new(
                        "toolUnavailable",
                        format!("Tool {dt} is not available"),
                    ));
                }
                (dt, dq, true)
            }
            None => (tool, input, false),
        };
        // Jev is nondeterministic and billed per evaluation. Query replay cannot
        // serve a page of the original judgment, including authenticated cursors.
        if tool == "jev"
            && [
                "responseCharOffset",
                "responseCharLength",
                "responseSnapshot",
            ]
            .iter()
            .any(|field| input.get(field).is_some())
        {
            return Err(RuntimeError::new(
                "unsupportedResponsePagination",
                "Jev response pagination is unsupported: replay would repeat context execution and inference. Remove response paging options and use the complete result; batch independent context/question pairs.",
            ));
        }
        // Parse response-paging options before contract validation.
        let options: ResponsePageOptions =
            serde_json::from_value(input.clone()).unwrap_or_default();
        let prepared_queries = if from_cursor {
            vec![
                input
                    .as_object()
                    .cloned()
                    .map(Value::Object)
                    .unwrap_or(input),
            ]
        } else {
            contracts::prepare_many_and_validate(&tool, input, PrepareOptions::default()).map_err(
                |error| RuntimeError {
                    code: "invalidInput".into(),
                    message: error.to_string(),
                    payload: Some(Box::new(contracts::format_input_error(&tool, &error))),
                    validation_issues: Some(error.issues),
                },
            )?
        };
        let mut queries = Vec::with_capacity(prepared_queries.len());
        for query in prepared_queries {
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
            queries.push(Value::Object(checked.sanitized_params));
        }
        let response_query = if queries.len() == 1 {
            queries[0].clone()
        } else {
            json!({"queries": queries.clone()})
        };
        let paths = self.paths.clone();
        let security = self.security.clone();
        let regex = LocalFetchRegex::new(self.regex.clone());
        let github_services = self.github_services.clone();
        let github_cache = self.github_cache.clone();
        let config = self.config.clone();
        let home = self.inspect_config().home;
        let handle = tokio::runtime::Handle::current();
        let dispatcher = super::domain_dispatch::DomainDispatcher {
            paths,
            security: security.clone(),
            regex,
            github_services,
            github_cache,
            config: config.clone(),
            home: home.clone(),
            handle: handle.clone(),
            lsp_pool: self.lsp_pool.clone(),
            lsp_execution_config: crate::tools::lsp_search::LspExecutionConfig {
                config_path: self.config.resolved.lsp.config_path.clone(),
                trust_project_config: self.input.trusted_project,
            },
            available_tools: ToolId::ALL
                .iter()
                .filter(|id| self.is_available(id.as_str()))
                .map(|id| id.as_str())
                .collect(),
        };
        let jev_key = self
            .config
            .env_value("OCTOCODE_JEV_KEY")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| SecretString::from(value.to_owned()));
        let jev_base_url = self
            .config
            .env_value("OCTOCODE_JEV_BASE_URL")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("https://api.typesafe.ai")
            .to_owned();
        let jev_model = self
            .config
            .env_value("OCTOCODE_JEV_MODEL")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("jev-latest")
            .to_owned();
        let jev_timeout = Duration::from_millis(self.config.resolved.network.timeout as u64);
        let jev_retries = self.config.resolved.network.max_retries as u32;
        let stats_enabled = config::is_stats_enabled(&self.config.resolved);
        let redact_emails = self.config.resolved.output.redact_emails;
        let output_tool = tool.clone();
        let cursor_scope = scope;
        let mut outcome = self
            .requests
            .execute_blocking_admitted(admission, move |context| {
                let mut rows = Vec::with_capacity(queries.len());
                let mut source_digest = None;
                let mut failure = None;
                let mut jev_rows = if tool == "jev" {
                    let _enter = handle.enter();
                    let Some(key) = jev_key.as_ref() else {
                        return Err(ExecutionError::WorkerFailed);
                    };
                    let evaluation_context = ExecutionContext {
                        deadline: context.deadline.min(Instant::now() + jev_timeout),
                        ..context.clone()
                    };
                    let evaluated = super::jev_batch::execute(
                        &queries,
                        &dispatcher,
                        &evaluation_context,
                        super::jev_batch::ProviderConfig {
                            key,
                            base_url: &jev_base_url,
                            model: &jev_model,
                            retries: jev_retries,
                        },
                        |usage| {
                            super::session_stats::record_jev(
                                &home,
                                stats_enabled,
                                &json!({"usage":usage}),
                            );
                        },
                    )?;
                    Some(evaluated.into_iter())
                } else {
                    None
                };
                for (index, query) in queries.iter().enumerate() {
                    context.check()?;
                    let result = match jev_rows.as_mut() {
                        Some(rows) => rows.next().ok_or(ExecutionError::WorkerFailed)?,
                        None => dispatcher.execute(&tool, query, &context)?,
                    };
                    context.check()?;
                    if queries.len() == 1 {
                        source_digest = result.source_digest;
                    }
                    failure = failure.or(result.failure);
                    let mut row =
                        response::result_row(&tool, index, query, result.data, result.status);
                    response::attach_diagnostics(&mut row, result.diagnostics);
                    if result.cache {
                        row["cache"] = json!(1);
                    }
                    response::apply_hint_policy(&mut row, &tool, query);
                    rows.push(row);
                }
                let all_failed = rows.iter().all(|row| {
                    row.get("status").and_then(serde_json::Value::as_str) == Some("error")
                });
                // Jev receipts and caller-authored rubric values are opaque JSON:
                // path compaction would mutate their identity and meaning.
                let mut structured = if tool == "jev" {
                    json!({"results": rows})
                } else {
                    response::envelope(rows)
                };
                let shared_path = queries
                    .first()
                    .and_then(|query| query.get("path"))
                    .filter(|path| queries.iter().all(|query| query.get("path") == Some(*path)));
                if (queries.len() == 1 || shared_path.is_some())
                    && let Some(query) = queries.first()
                {
                    response::attach_query_base(&mut structured, &tool, query);
                }
                response::sanitize_fields(&mut structured, &security, &context)?;
                if redact_emails && tool.starts_with("gh") {
                    response::redact_email_fields(&mut structured, &security, &context)?;
                }
                context.check()?;
                let render = options.render_text.unwrap_or(mcp)
                    || failure.is_some()
                    || options.response_char_length.is_some()
                    || options.response_char_offset.is_some()
                    || options.response_snapshot.is_some();
                let rendered_text =
                    render.then(|| super::render::render_tool(&tool, &structured, &response_query));
                context.check()?;
                let jev_output = tool == "jev";
                let prepared = ResponsePager::new(ResponsePagerConfig::default())
                    .prepare(
                        ResponseInput {
                            tool,
                            query: response_query,
                            structured,
                            rendered_text,
                            is_error: all_failed,
                            options,
                        },
                        &std::sync::atomic::AtomicBool::new(context.cancellation.is_cancelled()),
                    )
                    .map_err(|_| ExecutionError::WorkerFailed)?;
                // Stamp cursor tokens on the final envelope, which now includes
                // responsePagination.next added by the pager.
                let mut structured_content = prepared.structured_content;
                // Jev receipts retain executable tool/query pairs. Their tool
                // scopes differ from the outer Jev request's cursor scope.
                if !jev_output {
                    inject_cursors(&mut structured_content, &cursor_scope);
                }
                context.check()?;
                Ok(ToolOutcome {
                    structured_content,
                    content: prepared.content,
                    source_digest,
                    failure,
                    all_failed,
                })
            })
            .await
            .map_err(runtime_execution_error)?;
        if let Err(error) = contracts::validate_output(&output_tool, &outcome.structured_content) {
            // Row-scoped violations degrade to row-level errors so one
            // drifting emitter cannot discard the batch's healthy rows; the
            // rendered text keeps its pre-patch form. Envelope-level
            // violations still fail the whole call.
            match contracts::isolate_row_violations(
                &output_tool,
                &outcome.structured_content,
                &error,
            ) {
                Some(patched) => outcome.structured_content = patched,
                None => {
                    let details = error
                        .issues
                        .iter()
                        .map(|issue| format!("{}: {}", issue.path.join("."), issue.message))
                        .collect::<Vec<_>>()
                        .join("; ");
                    return Err(RuntimeError {
                        code: "outputContractViolation".into(),
                        message: format!(
                            "{output_tool} produced a response that violates its canonical output contract: {details}"
                        ),
                        payload: None,
                        validation_issues: Some(error.issues),
                    });
                }
            }
        }
        Ok(outcome)
    }
}

fn inject_cursors(value: &mut Value, scope: &str) {
    inject_cursors_inner(value, scope, false);
}

fn inject_cursors_inner(value: &mut Value, scope: &str, inside_next: bool) {
    match value {
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                inject_cursors_inner(child, scope, inside_next);
            }
        }
        Value::Object(map) => {
            if let (true, Some(tool_str), Some(query_val)) = (
                inside_next && !map.contains_key("cursor"),
                map.get("tool").and_then(Value::as_str),
                map.get("query").filter(|v| v.is_object()),
            ) {
                let tool = tool_str.to_owned();
                let query = query_val.clone();
                if let Ok(token) =
                    super::cursor::UniversalCursor::create(&tool, query, scope.to_owned())
                {
                    map.insert("cursor".into(), Value::String(token));
                }
            }
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(child) = map.get_mut(&key) {
                    inject_cursors_inner(child, scope, inside_next || key == "next");
                }
            }
        }
        _ => {}
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
