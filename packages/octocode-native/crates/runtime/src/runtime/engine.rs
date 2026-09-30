use super::{ExecutionContext, ExecutionError, RequestRuntime, RuntimeLimits, response};
use crate::config::{self, ConfigInput, ConfigOutput, RuntimeSurface};
use crate::contracts::{self, PrepareOptions};
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::regex::{IsolatedRegexEngine, IsolatedRegexLimits};
use crate::response::{PreparedResponse, ResponsePageOptions, TextContent};
use crate::security::{ContentSecurity, ValidationResult};
use crate::tools::ast_graph::store as graph_store;
use crate::tools::cancel::CancellationCheck;
use crate::tools::id::{ToolFamily, ToolId};
use crate::tools::local_fetch::LocalFetchRegex;

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const LOCAL_DISABLED: &str =
    "Local tools are disabled (ENABLE_LOCAL=false); graph commands read local files.";

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
    /// Sanitized full views of recently paged local files (this runtime's
    /// security policy only), so each localFetch page skips a full rescan.
    local_views: Arc<crate::security::scan::SanitizedViewMemo>,
    /// Pre-paging envelopes of recent paged responses (this runtime's config
    /// and workspace only), so a later page skips re-executing the batch.
    page_replays: Arc<PageReplayMemo>,
}

/// One paged response's envelope before paging, replayed only for a request
/// that carries the page's `responseSnapshot`.
#[derive(Clone)]
struct PageReplay {
    key: [u8; 32],
    snapshot: String,
    stored: std::time::Instant,
    bytes: usize,
    structured: Value,
    response_query: Value,
    failure: Option<FailureKind>,
    source_digest: Option<String>,
}

/// Bounded LRU of [`PageReplay`]s. A page served from it is byte-identical
/// to the one a same-snapshot re-execution would produce, without the
/// dispatch; a mismatched or missing snapshot always re-executes.
#[derive(Default)]
struct PageReplayMemo {
    entries: std::sync::Mutex<std::collections::VecDeque<PageReplay>>,
}

impl PageReplayMemo {
    const MAX_ENTRIES: usize = 8;
    const MAX_BYTES: usize = 32 * 1024 * 1024;
    const TTL: Duration = Duration::from_secs(120);

    fn get(&self, key: &[u8; 32], snapshot: &str) -> Option<PageReplay> {
        let mut entries = self.entries.lock().ok()?;
        entries.retain(|entry| entry.stored.elapsed() < Self::TTL);
        let position = entries
            .iter()
            .position(|entry| &entry.key == key && entry.snapshot == snapshot)?;
        let entry = entries.remove(position)?;
        entries.push_back(entry.clone());
        Some(entry)
    }

    fn insert(&self, entry: PageReplay) {
        if entry.bytes > Self::MAX_BYTES {
            return;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        entries.retain(|stored| stored.key != entry.key && stored.stored.elapsed() < Self::TTL);
        entries.push_back(entry);
        let mut bytes: usize = entries.iter().map(|stored| stored.bytes).sum();
        while entries.len() > Self::MAX_ENTRIES || bytes > Self::MAX_BYTES {
            let Some(evicted) = entries.pop_front() else {
                break;
            };
            bytes -= evicted.bytes;
        }
    }
}

/// Identity of an executed batch: the validated (defaulted, sanitized) rows
/// and rejected rows, so a compacted `next.query` replay maps to its origin.
fn page_replay_key(
    tool: &str,
    mcp: bool,
    revision: u64,
    queries: &[Value],
    rejected: &[(usize, Value)],
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let identity = json!({
        "tool": tool,
        "mcp": mcp,
        "revision": revision,
        "queries": queries,
        "rejected": rejected,
    });
    Sha256::digest(identity.to_string().as_bytes()).into()
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
    tool: ToolId,
    index: usize,
    raw: Value,
    error: &contracts::ContractValidationError,
    mcp: bool,
) -> (usize, Value) {
    let formatted = contracts::format_input_error(tool.as_str(), error, mcp);
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

fn invalid_input_error(
    tool: &str,
    error: contracts::ContractValidationError,
    mcp: bool,
) -> RuntimeError {
    RuntimeError {
        code: "invalidInput".into(),
        message: error.to_string(),
        payload: Some(Box::new(contracts::format_input_error(tool, &error, mcp))),
        validation_issues: Some(error.issues),
    }
}

/// Security-gate failures of one row as field issues, so they isolate and
/// render like any other invalid field. Only clasify evidence may be
/// redacted in place: any other rewritten value — including a clasify
/// context read's `query` — would silently change what a tool runs and leak
/// the placeholder into continuations.
fn security_issues(
    checked: &ValidationResult,
    prefix: &[String],
    clasify: bool,
) -> Vec<contracts::ValidationIssue> {
    let issue = |rule_id: &str, field: Option<&str>, message: String| {
        let mut path = prefix.to_vec();
        path.extend(field.map(str::to_owned));
        contracts::ValidationIssue {
            rule_id: rule_id.into(),
            path,
            message,
            schema: None,
            received: None,
        }
    };
    let mut issues = Vec::new();
    if !checked.is_valid {
        issues.extend(
            checked
                .warnings
                .iter()
                .filter(|warning| !warning.starts_with("Secrets detected"))
                .map(|warning| {
                    let hint = if warning.contains("exceeds maximum length") {
                        "; send a shorter, distinctive fragment"
                    } else {
                        ""
                    };
                    issue("security.input", None, format!("{warning}{hint}"))
                }),
        );
    }
    let redactable = |field: &&String| clasify && !field.contains("context.query.");
    issues.extend(
        checked
            .secret_fields
            .iter()
            .filter(|field| !redactable(field))
            .map(|field| {
            issue(
                "security.credential",
                Some(field),
                "Value looks like a credential and was not run (its redacted form would match unrelated text); search a non-secret fragment instead, such as its prefix or the identifier that holds it".into(),
            )
        }),
    );
    issues
}

/// Put rejected rows back at their input positions and renumber `index`, so
/// rows stay aligned with the caller's queries.
pub(super) fn merge_rejected_rows(rows: &mut Vec<Value>, rejected: Vec<(usize, Value)>) {
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

/// Clasify's resolved provider settings. Clasify is its own product: it
/// runs through [`ClasifySettings::execute`] and never enters the ordinary
/// results loop.
struct ClasifySettings {
    key: Option<SecretString>,
    base_url: String,
    endpoint_path: String,
    model: String,
    provider: &'static dyn crate::providers::classification::ClassificationProvider,
    timeout: Duration,
    retries: u32,
    max_concurrency: usize,
}

impl ClasifySettings {
    fn execute(
        &self,
        queries: &[Value],
        rejected_rows: Vec<(usize, Value)>,
        dispatcher: &super::domain_dispatch::DomainDispatcher,
        context: &ExecutionContext,
        record_usage: impl FnOnce(super::session_stats::ClassificationUsage),
    ) -> Result<super::clasify_batch::Receipts, ExecutionError> {
        let Some(key) = self.key.as_ref() else {
            return Err(ExecutionError::WorkerFailed);
        };
        super::clasify_batch::execute(
            queries,
            rejected_rows,
            dispatcher,
            context,
            self.timeout,
            super::clasify_batch::ProviderConfig {
                key,
                base_url: &self.base_url,
                endpoint_path: &self.endpoint_path,
                model: &self.model,
                provider: self.provider,
                retries: self.retries,
                max_concurrency: self.max_concurrency,
            },
            record_usage,
        )
    }
}

/// Semantic assessment is nondeterministic and billed per evaluation. Query
/// replay cannot serve a page of the original judgment, including
/// authenticated cursors.
fn reject_clasify_response_pagination(input: &Value) -> Result<(), RuntimeError> {
    if [
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
    Ok(())
}

/// The batch parallelism budget: how many threads one batch runs its
/// queries on, and how many cores each query's parallel directory walk may
/// use. Read-only rows fan out one thread per query; mutating and
/// self-scheduled tools stay ordered. Each localSearch ripgrep walk also runs
/// its own worker pool, which defaults to every core, so concurrent walks
/// split the cores instead of stacking full pools. Measured on a 5-query
/// localSearch batch over the native crates (12 cores, optimized build): one
/// thread per query with full pools 114 ms and 86 peak threads, three workers
/// 116 ms / 58, two 118 ms / 44, one walk at a time 130 ms / 29, so the rows
/// stay concurrent and only the per-walk pools shrink. Splitting the cores
/// (2 walkers each) against the same build with full pools: 708 vs 710 ms
/// median, 22 vs 72 peak threads; rounding the share up changed nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BatchBudget {
    /// Queries run at once.
    width: usize,
    /// Walk workers per query; `None` keeps the engine default (all cores).
    walk_threads: Option<u32>,
}

impl BatchBudget {
    fn new(id: ToolId, queries: usize, cores: usize) -> Self {
        let width = if id.supports_concurrent_queries() {
            queries.max(1)
        } else {
            1
        };
        let walk_threads =
            (width > 1).then(|| u32::try_from((cores / width).max(1)).unwrap_or(u32::MAX));
        Self {
            width,
            walk_threads,
        }
    }

    fn for_host(id: ToolId, queries: usize) -> Self {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        Self::new(id, queries, cores)
    }
}

fn execute_ordinary_queries(
    tool: ToolId,
    queries: &[Value],
    dispatcher: &super::domain_dispatch::DomainDispatcher,
    context: &ExecutionContext,
) -> Result<Vec<super::dispatch::DomainResult>, ExecutionError> {
    let budget = BatchBudget::for_host(tool, queries.len());
    if budget.width <= 1 {
        return queries
            .iter()
            .map(|query| dispatcher.execute(tool, query, context))
            .collect();
    }

    let context = &ExecutionContext {
        walk_threads: budget.walk_threads,
        ..context.clone()
    };
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
            local_views: Arc::default(),
            page_replays: Arc::default(),
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

    /// Provider settings for one clasify call, resolved before the blocking
    /// worker starts.
    fn clasify_settings(&self) -> ClasifySettings {
        let provider = crate::providers::classification::provider_for(
            self.config.resolved.classification.r#type.as_str(),
        );
        ClasifySettings {
            key: self
                .classification_key()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| SecretString::from(value.to_owned())),
            base_url: self
                .config
                .env_value("OCTOCODE_CLASSIFICATION_API_HOST")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| provider.default_host().to_owned()),
            endpoint_path: provider.endpoint_path().to_owned(),
            model: provider.default_model().to_owned(),
            provider,
            timeout: Duration::from_millis(self.config.resolved.network.timeout as u64),
            retries: self.config.resolved.network.max_retries as u32,
            max_concurrency: self.config.resolved.classification.max_concurrency as usize,
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

    /// CLI `graph ingest`: builds and publishes a code-graph snapshot under
    /// the same path policy and content security as the local tools.
    pub fn graph_ingest(&self, options: &graph_store::IngestOptions) -> graph_store::GraphOutput {
        if !self.config.resolved.local.enabled {
            return graph_store::GraphOutput::error(5, "graph.localDisabled", LOCAL_DISABLED);
        }
        graph_store::ingest(
            options,
            &self.paths,
            &self.security,
            &crate::tools::cancel::NeverCancel,
        )
    }

    /// CLI `graph query`: answers one bounded question from a snapshot.
    pub fn graph_query(&self, options: &graph_store::QueryOptions) -> graph_store::GraphOutput {
        if !self.config.resolved.local.enabled {
            return graph_store::GraphOutput::error(5, "graph.localDisabled", LOCAL_DISABLED);
        }
        graph_store::query(options, &self.paths)
    }

    pub fn is_available(&self, tool: &str) -> bool {
        let local = self.config.resolved.local.enabled;
        let clone = self.input.runtime_surface == RuntimeSurface::Cli
            && self.config.resolved.storage.mode == "persistent";
        let id = ToolId::from_name(tool);
        if self.input.runtime_surface != RuntimeSurface::Cli && id.is_some_and(ToolId::is_cli_only)
        {
            return false;
        }
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
                } else if self.input.runtime_surface != RuntimeSurface::Cli
                    && ToolId::from_name(name).is_some_and(ToolId::is_cli_only)
                {
                    entry["unavailableReason"] = json!("cliOnly");
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
        // Clone and rewrite mutate the user's machine: CLI only. MCP never
        // executes them, even if an embedder used a CLI host surface.
        let requested = ToolId::from_name(&tool);
        if mcp && requested.is_some_and(ToolId::is_cli_only) {
            return Err(RuntimeError::new(
                "toolUnavailable",
                format!("Tool {tool} is not available through MCP; run `octocode {tool}`."),
            ));
        }
        if !self.is_available(&tool) {
            if !mcp && requested.is_some_and(ToolId::is_beta) && !self.config.resolved.local.beta {
                return Err(RuntimeError::new(
                    "missingConfiguration",
                    format!(
                        "{tool} is a beta feature, disabled by default. Set OCTOCODE_BETA=true or local.beta:true before retrying."
                    ),
                ));
            }
            if !mcp
                && requested.is_some_and(ToolId::is_clasify)
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
                let (dt, dq) =
                    resume_cursor(tok, &scope, &self.paths).map_err(cursor_runtime_error)?;
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
        let id = ToolId::from_name(&tool).ok_or_else(|| {
            RuntimeError::new(
                "toolUnavailable",
                format!("Tool {tool} is not available in this native runtime"),
            )
        })?;
        let clasify = if id.is_clasify() {
            reject_clasify_response_pagination(&input)?;
            Some(self.clasify_settings())
        } else {
            None
        };
        // Parse response-paging options before contract validation.
        let options: ResponsePageOptions =
            serde_json::from_value(input.clone()).unwrap_or_default();
        let bulk = !from_cursor && (input.is_array() || input.get("queries").is_some());
        let mut invalid: Vec<(usize, contracts::ContractValidationError)> = Vec::new();
        let prepared_queries: Vec<(usize, Value)> = if from_cursor {
            vec![(0, input.clone())]
        } else {
            match contracts::prepare_many_and_validate(
                &tool,
                input.clone(),
                PrepareOptions::default(),
            ) {
                Ok(prepared) => prepared.into_iter().enumerate().collect(),
                // Row isolation: when only some rows are invalid, execute the
                // valid rows and return the rest as indexed error rows.
                Err(error) => {
                    let Some(rows) =
                        contracts::prepare_rows(&tool, &input, PrepareOptions::default())
                    else {
                        return Err(invalid_input_error(&tool, error, mcp));
                    };
                    let mut prepared = Vec::with_capacity(rows.len());
                    for (index, row) in rows.into_iter().enumerate() {
                        match row {
                            Ok(query) => prepared.push((index, query)),
                            Err(error) => invalid.push((index, error)),
                        }
                    }
                    prepared
                }
            }
        };
        let mut queries = Vec::with_capacity(prepared_queries.len());
        for (index, query) in prepared_queries {
            let checked = self.security.validate_input_parameters(&query);
            let prefix = if bulk {
                vec!["queries".to_owned(), index.to_string()]
            } else {
                Vec::new()
            };
            let issues = security_issues(&checked, &prefix, id.is_clasify());
            if issues.is_empty() {
                queries.push(Value::Object(checked.sanitized_params));
            } else {
                invalid.push((index, contracts::ContractValidationError { issues }));
            }
        }
        // Clasify matrices share batch-level rules, and a call with no valid
        // row has nothing to run: both fail as a whole.
        if !invalid.is_empty() && (queries.is_empty() || id.is_clasify()) {
            invalid.sort_by_key(|(index, _)| *index);
            let issues = invalid
                .into_iter()
                .flat_map(|(_, error)| error.issues)
                .collect();
            return Err(invalid_input_error(
                &tool,
                contracts::ContractValidationError { issues },
                mcp,
            ));
        }
        invalid.sort_by_key(|(index, _)| *index);
        let raw_rows = input
            .get("queries")
            .or(Some(&input))
            .and_then(Value::as_array);
        let rejected_rows: Vec<(usize, Value)> = invalid
            .iter()
            .map(|(index, error)| {
                let raw = raw_rows
                    .and_then(|rows| rows.get(*index))
                    .cloned()
                    .unwrap_or(Value::Null);
                rejected_row(id, *index, raw, error, mcp)
            })
            .collect();
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
            handle,
            lsp_pool: self.lsp_pool.clone(),
            local_views: self.local_views.clone(),
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
        let stats_enabled = config::is_stats_enabled(&self.config.resolved);
        let redact_emails = self.config.resolved.output.redact_emails;
        let auto_page_chars = self.config.resolved.output.pagination.default_char_length as usize;
        let text_format =
            super::render::TextFormat::from_config(&self.config.resolved.output.format);
        let output_tool = tool.clone();
        let replay_key = page_replay_key(&tool, mcp, self.input.revision, &queries, &rejected_rows);
        let replayed = options
            .response_snapshot
            .as_deref()
            .filter(|_| clasify.is_none())
            .and_then(|snapshot| self.page_replays.get(&replay_key, snapshot));
        let page_replays = self.page_replays.clone();
        let outcome = self
            .requests
            .execute_blocking_admitted(admission, move |context| {
                if let Some(replay) = replayed {
                    return super::response_stage::finish(
                        super::response_stage::StageInput {
                            tool: id,
                            structured: replay.structured,
                            response_query: replay.response_query,
                            options,
                            mcp,
                            failure: replay.failure,
                            auto_page_chars,
                            text_format,
                            allow_auto_paging: true,
                            source_digest: replay.source_digest,
                        },
                        &context,
                    );
                }
                if let Some(clasify) = &clasify {
                    let receipts = clasify.execute(
                        &queries,
                        rejected_rows,
                        &dispatcher,
                        &context,
                        |usage| {
                            super::session_stats::record_classification(
                                &home,
                                stats_enabled,
                                usage,
                            );
                        },
                    )?;
                    return super::response_stage::finish_receipts(
                        receipts,
                        response_query,
                        options,
                        &context,
                    );
                }
                let evaluated = execute_ordinary_queries(id, &queries, &dispatcher, &context)?;
                let mut rows = Vec::with_capacity(queries.len());
                let mut source_digest = None;
                let mut failure = None;
                let mut evaluated = evaluated.into_iter();
                for (index, query) in queries.iter().enumerate() {
                    context.check()?;
                    let result = evaluated.next().ok_or(ExecutionError::WorkerFailed)?;
                    context.check()?;
                    if queries.len() == 1 {
                        source_digest = result.source_digest;
                    }
                    failure = failure.or(result.failure);
                    let mut row =
                        response::result_row(id, index, query, result.data, result.status);
                    response::attach_diagnostics(&mut row, result.diagnostics);
                    if result.cache {
                        row["cache"] = json!(1);
                    }
                    response::apply_hint_policy(&mut row, id, query);
                    response::minimize_row(&mut row, id, query);
                    rows.push(row);
                }
                let rejected_at: Vec<usize> =
                    rejected_rows.iter().map(|(index, _)| *index).collect();
                merge_rejected_rows(&mut rows, rejected_rows);
                let by_row = super::clasify_handoff::row_queries(&queries, &rejected_at);
                let mut structured = response::envelope_in(rows, id, &by_row, &dispatcher.paths);
                response::finalize_output_fields(
                    &mut structured,
                    id,
                    &security,
                    &context,
                    redact_emails,
                )?;
                super::clasify_handoff::attach(&mut structured, &tool, &by_row);
                super::continuations::inherit_briefs(&mut structured, &by_row);
                super::continuations::filter_unavailable_cross_tool_next(
                    &mut structured,
                    &tool,
                    |target| {
                        (!mcp || target != "ghCloneRepo")
                            && dispatcher.available_tools.contains(&target)
                    },
                );
                let seed = (
                    structured.clone(),
                    response_query.clone(),
                    failure,
                    source_digest.clone(),
                );
                let finished = super::response_stage::finish(
                    super::response_stage::StageInput {
                        tool: id,
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
                )?;
                if let Ok(outcome) = &finished {
                    let pagination = &outcome.structured_content["responsePagination"];
                    if pagination["hasMore"] == true
                        && let Some(snapshot) = pagination["snapshot"].as_str()
                    {
                        let (structured, response_query, failure, source_digest) = seed;
                        page_replays.insert(PageReplay {
                            key: replay_key,
                            snapshot: snapshot.to_owned(),
                            stored: std::time::Instant::now(),
                            bytes: structured.to_string().len(),
                            structured,
                            response_query,
                            failure,
                            source_digest,
                        });
                    }
                }
                Ok(finished)
            })
            .await
            .map_err(runtime_execution_error)?
            .map_err(|error| output_contract_error(&output_tool, error))?;
        Ok(outcome)
    }
}

/// Decode a `{cursor}` token and verify its source; returns the resumed
/// tool and query.
fn resume_cursor(
    token: &str,
    scope: &str,
    paths: &PathPolicy,
) -> Result<(String, Value), super::cursor::CursorError> {
    let cursor = super::cursor::Cursor::decode(token, scope)?;
    cursor.verify_source(paths)?;
    Ok(cursor.into_parts())
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
            ToolId::GhCloneRepo,
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
mod batch_budget_tests {
    use super::*;

    #[test]
    fn read_only_batches_fan_out_one_thread_per_query_and_mutations_stay_ordered() {
        for id in ToolId::ALL {
            let ordered = matches!(
                id,
                ToolId::GhCloneRepo | ToolId::AstRewrite | ToolId::Clasify
            );
            let width = BatchBudget::new(id, 5, 12).width;
            assert_eq!(width, if ordered { 1 } else { 5 }, "{id}");
            assert_eq!(BatchBudget::new(id, 1, 12).width, 1, "{id}");
        }
    }

    #[test]
    fn concurrent_walks_split_the_cores_and_a_single_walk_keeps_the_default() {
        // Serializing walks measured slower, so rows stay concurrent and each
        // walk gets its share of the cores (see `BatchBudget`).
        let five = BatchBudget::new(ToolId::LocalSearch, 5, 12);
        assert_eq!(five.width, 5);
        assert_eq!(five.walk_threads, Some(2));
        assert_eq!(
            BatchBudget::new(ToolId::LocalSearch, 1, 12).walk_threads,
            None
        );
        // Never below one worker, even with more walks than cores.
        assert_eq!(
            BatchBudget::new(ToolId::LocalSearch, 5, 2).walk_threads,
            Some(1)
        );
        // Ordered tools run one row at a time with the full default.
        assert_eq!(
            BatchBudget::new(ToolId::AstRewrite, 5, 12).walk_threads,
            None
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

    fn workspace_paths(root: &std::path::Path) -> PathPolicy {
        PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.to_path_buf()),
            ..Default::default()
        })
        .expect("path policy")
    }

    #[test]
    fn universal_cursor_failing_its_own_check_reports_its_own_error() {
        let root = tempfile::tempdir().expect("temp workspace");
        let paths = workspace_paths(root.path());
        let token = super::super::cursor::Cursor::universal(
            "astSearch",
            json!({"path":"/workspace"}),
            "scope-a".into(),
        )
        .expect("universal cursor");
        assert_eq!(
            resume_cursor(&token, "scope-b", &paths).map(|_| ()),
            Err(super::super::cursor::CursorError::ChangedScope)
        );
        assert_eq!(
            cursor_runtime_error(super::super::cursor::CursorError::ChangedScope).code,
            "invalidCursor"
        );
        let (tool, query) = resume_cursor(&token, "scope-a", &paths).expect("resume");
        assert_eq!(tool, "astSearch");
        assert_eq!(query, json!({"path":"/workspace"}));
    }

    #[test]
    fn changing_one_local_fetch_source_stales_only_that_rows_cursor() {
        let root = tempfile::tempdir().expect("temp workspace");
        let first_path = root.path().join("a.rs");
        let second_path = root.path().join("b.rs");
        std::fs::write(&first_path, b"first-v1").expect("write first fixture");
        std::fs::write(&second_path, b"second-v1").expect("write second fixture");
        let paths = workspace_paths(root.path());
        let digest = |bytes: &[u8]| hex::encode(Sha256::digest(bytes));
        let first = super::super::cursor::Cursor::read(
            "localFetch",
            json!({"path":first_path}),
            digest(b"first-v1"),
            "scope".into(),
        )
        .and_then(|token| super::super::cursor::Cursor::decode(&token, "scope"))
        .expect("first read cursor");
        let second = super::super::cursor::Cursor::read(
            "localFetch",
            json!({"path":second_path}),
            digest(b"second-v1"),
            "scope".into(),
        )
        .and_then(|token| super::super::cursor::Cursor::decode(&token, "scope"))
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
