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
        super::maintenance::run_if_due(&runtime.inspect_config().home);
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

    /// Resolve the classification credential: the generic
    /// `OCTOCODE_CLASSIFICATION_API` first, then the selected vendor's native
    /// key env (e.g. `OCTOCODE_JEV_KEY` for the `jev` vendor).
    fn classification_key(&self) -> Option<&str> {
        let vendor = self.config.resolved.classification.r#type.as_str();
        let provider = crate::providers::classification::provider_for(vendor);
        self.config
            .env_value("OCTOCODE_CLASSIFICATION_API")
            .filter(|value| !value.trim().is_empty())
            .or_else(|| self.config.env_value(provider.key_env()))
    }

    pub fn is_available(&self, tool: &str) -> bool {
        let local = self.config.resolved.local.enabled;
        let clone = self.config.resolved.local.enable_clone
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

    /// Runtime truth only: tool names, availability, and the enforcement
    /// contract fingerprint. Presentation and MCP instructions are delivered
    /// to agents by the JS layers directly from `@octocodeai/octocode-core`.
    pub fn catalog(&self) -> Result<Value, RuntimeError> {
        let contract = contracts::parsed_contract()
            .map_err(|_| RuntimeError::new("contract", "Embedded contract is invalid"))?;
        let tools = contract["tools"]
            .as_array()
            .ok_or_else(|| RuntimeError::new("contract", "Embedded catalog is invalid"))?
            .iter()
            .map(|tool| {
                let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
                json!({
                    "name": name,
                    "shortDescription": tool["shortDescription"],
                    "available": self.is_available(name),
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "contractFormatVersion": contract["contractFormatVersion"],
            "fingerprint": contract["fingerprint"],
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
        let stats_enabled = config::is_stats_enabled(&self.config.resolved);
        let redact_emails = self.config.resolved.output.redact_emails;
        let output_tool = tool.clone();
        let cursor_scope = scope;
        let mut outcome = self
            .requests
            .execute_blocking_admitted(admission, move |context| {
                let mut rows = Vec::with_capacity(queries.len());
                let mut source_digest = None;
                let mut source_digests = Vec::with_capacity(queries.len());
                let mut failure = None;
                let mut classification_rows = if tool == "clasify" {
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
                    let evaluated = super::clasify_batch::execute(
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
                    let result = match classification_rows.as_mut() {
                        Some(rows) => rows.next().ok_or(ExecutionError::WorkerFailed)?,
                        None => dispatcher.execute(&tool, query, &context)?,
                    };
                    context.check()?;
                    let row_source_digest = result.source_digest;
                    if queries.len() == 1 {
                        source_digest = row_source_digest.clone();
                    }
                    source_digests.push(row_source_digest);
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
                let all_failed = rows.iter().all(|row| {
                    row.get("status").and_then(serde_json::Value::as_str) == Some("error")
                });
                // Jev receipts and caller-authored rubric values are opaque JSON:
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
                context.check()?;
                let render = options.render_text.unwrap_or(mcp)
                    || failure.is_some()
                    || options.response_char_length.is_some()
                    || options.response_char_offset.is_some()
                    || options.response_snapshot.is_some();
                let rendered_text =
                    render.then(|| super::render::render_tool(&tool, &structured, &response_query));
                context.check()?;
                let is_clasify_output = tool == "clasify";
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
                if !is_clasify_output {
                    inject_cursors(&mut structured_content, &cursor_scope, &source_digests);
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

fn inject_cursors(value: &mut Value, scope: &str, source_digests: &[Option<String>]) {
    inject_cursors_inner(value, scope, false, source_digests, None);
}

fn inject_cursors_inner<'a>(
    value: &mut Value,
    scope: &str,
    inside_next: bool,
    source_digests: &'a [Option<String>],
    row_source_digest: Option<&'a str>,
) {
    match value {
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                inject_cursors_inner(child, scope, inside_next, source_digests, row_source_digest);
            }
        }
        Value::Object(map) => {
            let row_source_digest = map
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|index| source_digests.get(index as usize))
                .and_then(Option::as_deref)
                .or(row_source_digest);
            if let (true, Some(tool_str), Some(query_val)) = (
                inside_next && !map.contains_key("cursor"),
                map.get("tool").and_then(Value::as_str),
                map.get("query").filter(|v| v.is_object()),
            ) {
                let tool = tool_str.to_owned();
                let query = query_val.clone();
                let token = if matches!(tool.as_str(), "localFetch" | "localSearch") {
                    row_source_digest.and_then(|digest| {
                        super::cursor::ReadCursor::create(
                            &tool,
                            query.clone(),
                            digest.to_owned(),
                            scope.to_owned(),
                        )
                        .ok()
                    })
                } else {
                    super::cursor::UniversalCursor::create(&tool, query, scope.to_owned()).ok()
                };
                if let Some(token) = token {
                    map.insert("cursor".into(), Value::String(token));
                }
            }
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if let Some(child) = map.get_mut(&key) {
                    inject_cursors_inner(
                        child,
                        scope,
                        inside_next || key == "next",
                        source_digests,
                        row_source_digest,
                    );
                }
            }
        }
        _ => {}
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
mod cursor_tests {
    use super::*;
    use crate::policy::path::PathPolicyConfig;
    use sha2::{Digest, Sha256};

    #[test]
    fn local_fetch_continuations_are_bound_to_their_result_row_digest() {
        let mut structured = json!({
            "results":[
                {"index":0,"data":{"next":{"continue":{"tool":"localFetch","query":{"path":"/workspace/a.rs","offset":1}}}}},
                {"index":1,"data":{"next":{"continue":{"tool":"localFetch","query":{"path":"/workspace/b.rs","offset":1}}}}}
            ]
        });
        inject_cursors(
            &mut structured,
            "scope",
            &[Some("digest-a".into()), Some("digest-b".into())],
        );

        let first = structured["results"][0]["data"]["next"]["continue"]["cursor"]
            .as_str()
            .expect("first cursor");
        let second = structured["results"][1]["data"]["next"]["continue"]["cursor"]
            .as_str()
            .expect("second cursor");
        let first = super::super::cursor::ReadCursor::decode(first, "scope")
            .expect("decode first read cursor");
        let second = super::super::cursor::ReadCursor::decode(second, "scope")
            .expect("decode second read cursor");
        assert_eq!(first.source_sha256, "digest-a");
        assert_eq!(second.source_sha256, "digest-b");
        assert_ne!(first.source_sha256, second.source_sha256);
    }

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
