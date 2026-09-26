use super::{ExecutionContext, ExecutionError, RequestRuntime, RuntimeLimits, response};
use crate::config::{self, ConfigInput, ConfigOutput, RuntimeSurface};
use crate::contracts::{self, PrepareOptions};
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::regex::{IsolatedRegexEngine, IsolatedRegexLimits};
use crate::response::{PreparedResponse, ResponsePageOptions, TextContent};
use crate::security::ContentSecurity;
use crate::tools::id::{ToolFamily, ToolId};
use crate::tools::local_fetch::{CancellationCheck, LocalFetchRegex};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostOptions {
    pub cwd: Option<PathBuf>,
    pub regex_worker_path: Option<PathBuf>,
    /// Explicit host environment. Embedders use this to keep configuration
    /// injection deterministic; standalone surfaces fall back to process env.
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub trusted_project: bool,
    #[serde(default)]
    pub surface: RuntimeSurface,
    /// Override the per-request execution timeout in seconds (default: 60).
    /// Interactive surfaces set 300 to exceed cold start plus one LSP request.
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

/// An invalid row of an isolated batch, shaped like any other error row.
fn rejected_row(
    tool: &str,
    index: usize,
    raw: Value,
    error: &contracts::ContractValidationError,
) -> (usize, Value) {
    let formatted = contracts::format_input_error(tool, error);
    let data = json!({
        "error": formatted["error"],
        "errorCode": "invalidInput",
        "hints": formatted["details"],
    });
    (
        index,
        super::response::result_row(tool, index, &raw, data, Some("error")),
    )
}

/// Put rejected rows back at their input positions and renumber `index`, so
/// rows stay aligned with the caller's queries.
fn merge_rejected_rows(rows: &mut Vec<Value>, rejected: Vec<(usize, Value)>) {
    if rejected.is_empty() {
        return;
    }
    for (index, row) in rejected {
        let position = index.min(rows.len());
        rows.insert(position, row);
    }
    for (index, row) in rows.iter_mut().enumerate() {
        row["index"] = json!(index);
    }
}

fn output_contract_error(tool: &str, error: contracts::ContractValidationError) -> RuntimeError {
    let details = error
        .issues
        .iter()
        .map(|issue| format!("{}: {}", issue.path.join("."), issue.message))
        .collect::<Vec<_>>()
        .join("; ");
    RuntimeError {
        code: "outputContractViolation".into(),
        message: format!(
            "{tool} produced a response that violates its canonical output contract: {details}"
        ),
        payload: None,
        validation_issues: Some(error.issues),
    }
}

fn mcp_result(result: ToolOutcome) -> Result<Value, RuntimeError> {
    serde_json::to_value(PreparedResponse {
        content: result.content,
        structured_content: result.structured_content,
        is_error: result.all_failed,
    })
    .map_err(|_| RuntimeError::new("response", "Cannot serialize response"))
}

fn execute_ordinary_queries(
    tool: &str,
    queries: &[Value],
    dispatcher: &super::domain_dispatch::DomainDispatcher,
    context: &ExecutionContext,
) -> Result<Vec<super::dispatch::DomainResult>, ExecutionError> {
    let concurrent = queries.len() > 1
        && ToolId::from_name(tool).is_some_and(ToolId::supports_concurrent_queries);
    if !concurrent {
        return queries
            .iter()
            .map(|query| dispatcher.execute(tool, query, context))
            .collect();
    }

    std::thread::scope(|scope| {
        let tasks = queries
            .iter()
            .map(|query| scope.spawn(move || dispatcher.execute(tool, query, context)))
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| task.join().map_err(|_| ExecutionError::WorkerFailed)?)
            .collect()
    })
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
            options.env.unwrap_or_else(|| std::env::vars().collect()),
            cwd,
            home,
            options.trusted_project,
            options.surface,
            1,
        );
        let mut runtime = Self::new(input)?;
        // Misconfiguration never fails startup; say what was ignored and
        // where. stderr only: stdout carries CLI results and MCP JSON-RPC.
        for diagnostic in &runtime.config.diagnostics {
            eprintln!("{diagnostic}");
        }
        // storage.mode=memory must not touch the disk cache at all.
        if config::is_persistent_storage_enabled(&runtime.config.resolved) {
            super::maintenance::run_if_due(&runtime.inspect_config().home);
        }
        if let Some(secs) = options.timeout_secs {
            // Replace the default 60-second runtime with the caller-specified timeout.
            // Interactive surfaces use 300 s so initialize, Java readiness,
            // request retries, and cleanup fit below the outer deadline.
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
        let security = ContentSecurity::new();
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
    /// config (e.g. `allowed_paths`) does not invalidate GitHub/remote cursors.
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

    /// Resolve the classification credential: the generic
    /// `OCTOCODE_CLASSIFICATION_API` first, then the selected vendor's native
    /// key env (e.g. `OCTOCODE_JEV_KEY` for the `jev` vendor).
    fn classification_key(&self) -> Option<&str> {
        let vendor = self.config.resolved.classification.r#type.as_str();
        let provider = crate::providers::classification::provider_for(vendor);
        // Present but blank is an explicit opt-out: no vendor-key fallback.
        match self.config.env_value("OCTOCODE_CLASSIFICATION_API") {
            Some(value) if value.trim().is_empty() => None,
            Some(value) => Some(value),
            None => self.config.env_value(provider.key_env()),
        }
    }

    /// True when `tools.enabled`/`tools.disabled` (not a feature gate)
    /// excludes the tool, so the remedy is the tool list.
    fn excluded_by_tool_list(&self, tool: &str) -> bool {
        let tools = &self.config.resolved.tools;
        tools
            .enabled
            .as_ref()
            .is_some_and(|names| !names.iter().any(|name| name == tool))
            || tools
                .disabled
                .as_ref()
                .is_some_and(|names| names.iter().any(|name| name == tool))
    }

    pub fn is_available(&self, tool: &str) -> bool {
        let local = self.config.resolved.local.enabled;
        let clone = self.input.runtime_surface == RuntimeSurface::Cli
            && self.config.resolved.storage.mode == "persistent";
        let id = ToolId::from_name(tool);
        // GitHub read tools are always enabled; cloning has an extra gate below.
        let github = matches!(id, Some(t) if t.is_github() && t != ToolId::GhCloneRepo);
        let local_tools = local
            && matches!(id, Some(t) if t.is_local())
            && (id.is_none_or(|tool| !tool.is_beta()) || self.config.resolved.local.beta);
        let classification = matches!(id, Some(t) if t.is_clasify())
            && self
                .classification_key()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty());
        (github
            || local_tools
            || (id == Some(ToolId::GhCloneRepo) && clone)
            || id == Some(ToolId::ArtifactSearch)
            || classification)
            && !self.excluded_by_tool_list(tool)
    }

    /// Runtime truth only: tool names, availability, grammar capabilities, and
    /// the enforcement contract fingerprint. Presentation and MCP instructions
    /// are delivered to agents by the JS layers from `@octocodeai/octocode-core`.
    pub fn catalog(&self) -> Result<Value, RuntimeError> {
        let contract = contracts::parsed_contract()
            .map_err(|_| RuntimeError::new("contract", "Embedded contract is invalid"))?;
        let tools = contract["tools"]
            .as_array()
            .ok_or_else(|| RuntimeError::new("contract", "Embedded catalog is invalid"))?
            .iter()
            .map(|tool| {
                let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
                let mut entry = json!({
                    "name": name,
                    "shortDescription": tool["shortDescription"],
                    "available": self.is_available(name),
                });
                if self.excluded_by_tool_list(name) {
                    entry["unavailableReason"] = json!("toolsList");
                }
                entry
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "contractFormatVersion": contract["contractFormatVersion"],
            "fingerprint": contract["fingerprint"],
            "grammarCapabilities": octocode_engine::portable::grammar_capabilities(),
            "tools": tools,
        }))
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
        mcp_result(result)
    }

    async fn execute_channel(
        &self,
        admission: super::RequestAdmission,
        tool: String,
        input: Value,
        mcp: bool,
    ) -> Result<ToolOutcome, RuntimeError> {
        // The CLI may clone into its persistent cache. MCP never exposes the
        // mutating clone operation, even if an embedder used a CLI host surface.
        if mcp && tool == "ghCloneRepo" {
            return Err(RuntimeError::new(
                "toolUnavailable",
                "Tool ghCloneRepo is not available through MCP",
            ));
        }
        if !self.is_available(&tool) {
            if !mcp
                && ToolId::from_name(&tool).is_some_and(ToolId::is_beta)
                && !self.config.resolved.local.beta
            {
                return Err(RuntimeError::new(
                    "missingConfiguration",
                    format!(
                        "{tool} is a beta feature, disabled by default. Set OCTOCODE_BETA=true or local.beta:true before retrying."
                    ),
                ));
            }
            if !mcp
                && tool == "clasify"
                && self
                    .classification_key()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
            {
                return Err(RuntimeError::new("missingConfiguration", {
                    let vendor = self.config.resolved.classification.r#type.as_str();
                    let p = crate::providers::classification::provider_for(vendor);
                    format!(
                        "clasify requires OCTOCODE_CLASSIFICATION_API (or the {vendor} \
                             vendor's {}). Create a classification provider API key ({}) \
                             and set OCTOCODE_CLASSIFICATION_API before retrying.",
                        p.key_env(),
                        p.docs_url(),
                    )
                }));
            }
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
                let (dt, dq) = match super::cursor::UniversalCursor::decode(tok, &scope) {
                    Ok(cursor) => (cursor.tool, cursor.query),
                    Err(_) => {
                        let cursor = super::cursor::ReadCursor::decode(tok, &scope)
                            .map_err(cursor_runtime_error)?;
                        cursor
                            .verify_source(&self.paths)
                            .map_err(cursor_runtime_error)?;
                        (cursor.tool, cursor.query)
                    }
                };
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
        // Semantic assessment is nondeterministic and billed per evaluation. Query replay cannot
        // serve a page of the original judgment, including authenticated cursors.
        if tool == "clasify"
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
                "clasify response pagination is unsupported: replay would repeat context execution and inference. Use the page-level results and next.clasify continuation instead.",
            ));
        }
        // Parse response-paging options before contract validation.
        let options: ResponsePageOptions =
            serde_json::from_value(input.clone()).unwrap_or_default();
        let mut rejected_rows: Vec<(usize, Value)> = Vec::new();
        let prepared_queries = if from_cursor {
            vec![
                input
                    .as_object()
                    .cloned()
                    .map(Value::Object)
                    .unwrap_or(input),
            ]
        } else {
            match contracts::prepare_many_and_validate(
                &tool,
                input.clone(),
                PrepareOptions::default(),
            ) {
                Ok(prepared) => prepared,
                // Row isolation: when only some rows are invalid, execute the
                // valid rows and return the rest as indexed error rows.
                Err(error) => {
                    let Some(rows) =
                        contracts::prepare_rows(&tool, &input, PrepareOptions::default())
                    else {
                        return Err(RuntimeError {
                            code: "invalidInput".into(),
                            message: error.to_string(),
                            payload: Some(Box::new(contracts::format_input_error(&tool, &error))),
                            validation_issues: Some(error.issues),
                        });
                    };
                    let raw_rows = input
                        .get("queries")
                        .or(Some(&input))
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let mut prepared = Vec::with_capacity(rows.len());
                    for (index, row) in rows.into_iter().enumerate() {
                        match row {
                            Ok(query) => prepared.push(query),
                            Err(error) => rejected_rows.push(rejected_row(
                                &tool,
                                index,
                                raw_rows.get(index).cloned().unwrap_or(Value::Null),
                                &error,
                            )),
                        }
                    }
                    prepared
                }
            }
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
            json!({"queries":queries})
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
        let classification_provider = crate::providers::classification::provider_for(
            self.config.resolved.classification.r#type.as_str(),
        );
        let classification_key_secret = self
            .classification_key()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| SecretString::from(value.to_owned()));
        let classification_base_url = self
            .config
            .env_value("OCTOCODE_CLASSIFICATION_API_HOST")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| classification_provider.default_host().to_owned());
        let classification_endpoint_path = classification_provider.endpoint_path().to_owned();
        let classification_model = classification_provider.default_model().to_owned();
        let classification_timeout =
            Duration::from_millis(self.config.resolved.network.timeout as u64);
        let classification_retries = self.config.resolved.network.max_retries as u32;
        let classification_max_concurrency =
            self.config.resolved.classification.max_concurrency as usize;
        let stats_enabled = config::is_stats_enabled(&self.config.resolved);
        let redact_emails = self.config.resolved.output.redact_emails;
        let auto_page_chars = self.config.resolved.output.pagination.default_char_length as usize;
        let text_format =
            super::render::TextFormat::from_config(&self.config.resolved.output.format);
        let output_tool = tool.clone();
        let outcome = self
            .requests
            .execute_blocking_admitted(admission, move |context| {
                let mut rows = Vec::with_capacity(queries.len());
                let mut source_digest = None;
                let mut failure = None;
                let evaluated = if tool == "clasify" {
                    let _enter = handle.enter();
                    let Some(key) = classification_key_secret.as_ref() else {
                        return Err(ExecutionError::WorkerFailed);
                    };
                    let evaluation_context = ExecutionContext {
                        deadline: context
                            .deadline
                            .min(Instant::now() + classification_timeout),
                        ..context.clone()
                    };
                    super::clasify_batch::execute(
                        &queries,
                        &dispatcher,
                        &evaluation_context,
                        super::clasify_batch::ProviderConfig {
                            key,
                            base_url: &classification_base_url,
                            endpoint_path: &classification_endpoint_path,
                            model: &classification_model,
                            provider: classification_provider,
                            retries: classification_retries,
                            max_concurrency: classification_max_concurrency,
                        },
                        |usage| {
                            super::session_stats::record_classification(
                                &home,
                                stats_enabled,
                                usage,
                            );
                        },
                    )?
                } else {
                    execute_ordinary_queries(&tool, &queries, &dispatcher, &context)?
                };
                let mut evaluated = evaluated.into_iter();
                for (index, query) in queries.iter().enumerate() {
                    context.check()?;
                    let result = evaluated.next().ok_or(ExecutionError::WorkerFailed)?;
                    context.check()?;
                    if queries.len() == 1 {
                        source_digest = result.source_digest;
                    }
                    failure = failure.or(result.failure);
                    if tool == "clasify" {
                        rows.push(result.data);
                        continue;
                    }
                    let mut row =
                        response::result_row(&tool, index, query, result.data, result.status);
                    response::attach_diagnostics(&mut row, result.diagnostics);
                    if result.cache {
                        row["cache"] = json!(1);
                    }
                    response::apply_hint_policy(&mut row, &tool, query);
                    rows.push(row);
                }
                merge_rejected_rows(&mut rows, rejected_rows);
                // Clasify receipts and caller-authored rubric values are opaque JSON:
                // path compaction would mutate their identity and meaning.
                let mut structured = if tool == "clasify" {
                    json!({"queries": rows})
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
                response::finalize_output_fields(
                    &mut structured,
                    &tool,
                    &security,
                    &context,
                    redact_emails,
                )?;
                if tool != "clasify" {
                    super::continuations::filter_unavailable_cross_tool_next(
                        &mut structured,
                        &tool,
                        |target| {
                            (!mcp || target != "ghCloneRepo")
                                && dispatcher.available_tools.contains(&target)
                        },
                    );
                }
                super::response_stage::finish(
                    super::response_stage::StageInput {
                        tool,
                        structured,
                        response_query,
                        options,
                        mcp,
                        failure,
                        auto_page_chars,
                        text_format,
                        allow_auto_paging: true,
                        source_digest,
                    },
                    &context,
                )
            })
            .await
            .map_err(runtime_execution_error)?
            .map_err(|error| output_contract_error(&output_tool, error))?;
        Ok(outcome)
    }
}

fn cursor_runtime_error(error: super::cursor::CursorError) -> RuntimeError {
    let code = if matches!(
        error,
        super::cursor::CursorError::ChangedSource | super::cursor::CursorError::SourceUnavailable
    ) {
        "staleCursor"
    } else {
        "invalidCursor"
    };
    RuntimeError::new(code, format!("{error:?}"))
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

#[cfg(test)]
mod output_recovery_tests {
    use super::super::response_stage::{isolate_output_rows, response_all_failed};
    use super::*;

    fn mcp_from_rows(mut structured: Value) -> Value {
        assert!(isolate_output_rows("ghCloneRepo", &mut structured).expect("row isolation"));
        let all_failed = response_all_failed(&structured);
        let text = super::super::render::render_tool(
            "ghCloneRepo",
            &structured,
            &json!({"owner":"a","repo":"b"}),
            super::super::render::TextFormat::Yaml,
        );
        mcp_result(ToolOutcome {
            structured_content: structured,
            content: vec![TextContent {
                r#type: "text".into(),
                text,
            }],
            source_digest: None,
            failure: Some(FailureKind::Execution),
            all_failed,
        })
        .expect("MCP response")
    }

    #[test]
    fn malformed_single_row_is_an_error_in_both_mcp_channels() {
        let result = mcp_from_rows(json!({"results":[
            {"index":0,"data":{"owner":"a"}}
        ]}));
        assert_eq!(result["isError"], true);
        assert_eq!(
            result["structuredContent"]["results"][0]["data"]["errorCode"],
            "outputContractViolation"
        );
        assert!(
            result["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("outputContractViolation")),
            "{result}"
        );
    }

    #[test]
    fn malformed_row_in_mixed_batch_keeps_healthy_row_and_mcp_success_flag() {
        let result = mcp_from_rows(json!({"results":[
            {"index":0,"data":{"owner":"a"}},
            {"index":1,"data":{
                "owner":"a","repo":"b","totalSize":0,
                "location":{
                    "kind":"repo","localPath":"/tmp/repo","source":"clone",
                    "cached":false,"commitSha":"abc","verified":true,
                    "complete":true,"resolvedBranch":"main"
                }
            }}
        ]}));
        assert_eq!(result["isError"], false);
        assert_eq!(
            result["structuredContent"]["results"][0]["data"]["errorCode"],
            "outputContractViolation"
        );
        assert_eq!(
            result["structuredContent"]["results"][1]["data"]["location"]["commitSha"],
            "abc"
        );
        assert!(
            result["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.contains("outputContractViolation")),
            "{result}"
        );
    }
}

#[cfg(test)]
mod cursor_tests {
    //! Legacy `{cursor}` resume: responses no longer stamp cursors (localFetch
    //! continuations carry `snapshot`), but previously issued read cursors
    //! still verify their source before resuming.
    use super::*;
    use crate::policy::path::PathPolicyConfig;
    use sha2::{Digest, Sha256};

    #[test]
    fn changing_one_local_fetch_source_stales_only_that_rows_cursor() {
        let root = tempfile::tempdir().expect("temp workspace");
        let first_path = root.path().join("a.rs");
        let second_path = root.path().join("b.rs");
        std::fs::write(&first_path, b"first-v1").expect("write first fixture");
        std::fs::write(&second_path, b"second-v1").expect("write second fixture");
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            ..Default::default()
        })
        .expect("path policy");
        let digest = |bytes: &[u8]| hex::encode(Sha256::digest(bytes));
        let first = super::super::cursor::ReadCursor::create(
            "localFetch",
            json!({"path":first_path}),
            digest(b"first-v1"),
            "scope".into(),
        )
        .and_then(|token| super::super::cursor::ReadCursor::decode(&token, "scope"))
        .expect("first read cursor");
        let second = super::super::cursor::ReadCursor::create(
            "localFetch",
            json!({"path":second_path}),
            digest(b"second-v1"),
            "scope".into(),
        )
        .and_then(|token| super::super::cursor::ReadCursor::decode(&token, "scope"))
        .expect("second read cursor");

        first.verify_source(&paths).expect("first initially fresh");
        second
            .verify_source(&paths)
            .expect("second initially fresh");
        std::fs::write(&first_path, b"first-v2").expect("mutate first fixture");

        let first_error = first
            .verify_source(&paths)
            .expect_err("mutated first source must be stale");
        assert_eq!(
            first_error,
            super::super::cursor::CursorError::ChangedSource
        );
        assert_eq!(cursor_runtime_error(first_error).code, "staleCursor");
        second
            .verify_source(&paths)
            .expect("unchanged second source remains resumable");
    }
}
