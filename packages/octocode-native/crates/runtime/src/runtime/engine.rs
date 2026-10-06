use super::{ExecutionContext, ExecutionError, RequestRuntime, RuntimeLimits};
use crate::config::{self, ConfigInput, ConfigOutput, RuntimeSurface};
use crate::contracts::{self, PrepareOptions};
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::regex::{IsolatedRegexEngine, IsolatedRegexLimits};
use crate::response::pager::{ResponsePageOptions, TextContent};
use crate::response::{continuations, read_share, render, rows, stage, verbose};
use crate::security::{ContentSecurity, ValidationResult};
use crate::tools::ast_graph::store as graph_store;
use crate::tools::cancel::CancellationCheck;
use crate::tools::id::ToolId;
use crate::tools::local_fetch::LocalFetchRegex;
use crate::tools::result::FailureKind;

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
    pub(crate) fn new(code: &str, message: impl Into<String>) -> Self {
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
    /// Registry answers for artifactSearch; `None` under memory storage.
    artifact_cache: Option<Arc<crate::providers::artifact::ArtifactCache>>,
    /// Resolved from the immutable config and surface; only
    /// [`ToolRuntime::probe_classification`] narrows it afterwards.
    available_tools: std::sync::RwLock<Arc<[&'static str]>>,
    /// Set when the startup probe found the classification provider unusable.
    classification_unreachable: std::sync::atomic::AtomicBool,
    family_excluded: Arc<[&'static str]>,
    lsp_execution_config: Arc<crate::tools::lsp_search::LspExecutionConfig>,
}

/// One paged response's envelope before paging, replayed only for a request
/// that carries the page's `responseSnapshot` while its sources are
/// unchanged since it executed.
#[derive(Clone)]
struct PageReplay {
    key: [u8; 32],
    snapshot: String,
    /// When the rows started executing: the sources must be unchanged since.
    started: std::time::SystemTime,
    stored: std::time::Instant,
    bytes: usize,
    structured: Value,
    response_query: Value,
    failure: Option<FailureKind>,
    source_digest: Option<String>,
}

/// Bounded LRU of [`PageReplay`]s. A page served from it is byte-identical
/// to the one a same-snapshot re-execution would produce, without the
/// dispatch; a mismatched or missing snapshot, or changed sources, always
/// re-execute, so a later page answers as a fresh runtime would.
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
/// and rejected rows in canonical key order, so a compacted `next.query`
/// replay maps to its origin however the caller ordered its fields.
fn page_replay_key(
    tool: &str,
    mcp: bool,
    queries: &[Value],
    rejected: &[(usize, Value)],
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let identity = json!({
        "tool": tool,
        "mcp": mcp,
        "queries": queries,
        "rejected": rejected,
    });
    Sha256::digest(
        crate::canonical_json::canonicalize(identity)
            .to_string()
            .as_bytes(),
    )
    .into()
}

#[derive(Debug)]
pub struct ToolOutcome {
    pub structured_content: Value,
    pub content: Vec<TextContent>,
    pub source_digest: Option<String>,
    pub failure: Option<FailureKind>,
    pub all_failed: bool,
    pub(crate) tool: ToolId,
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
    let mut row = rows::result_row(tool, index, &raw, data, Some("error"));
    verbose::prune_row(&mut row, tool, &raw);
    (index, row)
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
/// resource read's `query` — would silently change what a tool runs and leak
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
    let redactable = |field: &&String| clasify && !field.starts_with("resources[].query.");
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
pub(crate) fn merge_rejected_rows(rows: &mut Vec<Value>, rejected: Vec<(usize, Value)>) {
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

/// The MCP CallToolResult, built by move (the structured content is never
/// re-serialized).
fn mcp_result(result: ToolOutcome) -> Result<Value, RuntimeError> {
    let content = serde_json::to_value(result.content)
        .map_err(|_| RuntimeError::new("response", "Cannot serialize response"))?;
    let mut response = serde_json::Map::with_capacity(3);
    response.insert("content".into(), content);
    response.insert("structuredContent".into(), result.structured_content);
    response.insert("isError".into(), Value::Bool(result.all_failed));
    Ok(Value::Object(response))
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
    response_page: Option<usize>,
    dispatcher: &super::domain_dispatch::DomainDispatcher,
    context: &ExecutionContext,
) -> Result<Vec<super::dispatch::DomainResult>, ExecutionError> {
    // Each row's share of the response page its output must fit.
    let context = &ExecutionContext {
        response_window: response_page
            .filter(|page| *page > 0)
            .map(|page| page / queries.len().max(1)),
        ..context.clone()
    };
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
        );
        let mut runtime = Self::new(input)?;
        // Misconfiguration never fails startup; say what was ignored and
        // where. stderr only: stdout carries CLI results and MCP JSON-RPC.
        for diagnostic in &runtime.config.diagnostics {
            eprintln!("{diagnostic}");
        }
        // storage.mode=memory must not touch the disk cache at all.
        if config::is_persistent_storage_enabled(&runtime.config.resolved) {
            super::maintenance::run_if_due(&runtime.config.home);
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
        let octocode_home = config.home.clone();
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
        let response_dir = config::is_persistent_storage_enabled(&config.resolved)
            .then(|| octocode_home.join("tmp").join("response"));
        let github_cache = super::github_cache::GitHubContentCache::new(
            crate::cache::CacheConfig::default(),
            response_dir.clone(),
        );
        let artifact_cache = response_dir
            .map(|dir| Arc::new(crate::providers::artifact::ArtifactCache::new(Some(dir))));
        let lsp_execution_config = Arc::new(crate::tools::lsp_search::LspExecutionConfig {
            config_path: config.resolved.lsp.config_path.clone(),
            trust_project_config: input.trusted_project,
            env: config
                .effective_env
                .iter()
                .filter(|(key, _)| key.starts_with("OCTOCODE_"))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            octocode_home: Some(octocode_home),
        });
        let mut runtime = Self {
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
            artifact_cache,
            available_tools: std::sync::RwLock::default(),
            classification_unreachable: std::sync::atomic::AtomicBool::new(false),
            family_excluded: Arc::default(),
            lsp_execution_config,
        };
        runtime.available_tools = std::sync::RwLock::new(
            ToolId::ALL
                .iter()
                .filter(|id| runtime.resolve_available(**id))
                .map(|id| id.as_str())
                .collect(),
        );
        runtime.family_excluded = ToolId::ALL
            .iter()
            .map(|id| id.as_str())
            .filter(|name| runtime.excluded_by_family(name))
            .collect();
        Ok(runtime)
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
    fn clasify_settings(&self) -> crate::tools::clasify::ClasifySettings {
        let provider = crate::providers::classification::provider_for(
            self.config.resolved.classification.r#type.as_str(),
        );
        crate::tools::clasify::ClasifySettings {
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

    /// True when the `tools.family` preset leaves the tool out: `local` keeps
    /// local and remote tools, `github` keeps GitHub and remote tools (core
    /// `TOOL_POLICIES` family). It only narrows the tool lists.
    fn excluded_by_family(&self, tool: &str) -> bool {
        let Some(id) = ToolId::from_name(tool) else {
            return false;
        };
        match self.config.resolved.tools.family.as_str() {
            "local" => id.is_github(),
            "github" => id.is_local(),
            _ => false,
        }
    }

    /// MCP presentation switches (`mcp.*` config): the available tools served
    /// only through the deferred-tool dispatcher.
    fn presentation(&self) -> Value {
        let mcp = &self.config.resolved.mcp;
        let deferred: Vec<&str> = ToolId::ALL
            .iter()
            .map(|id| id.as_str())
            .filter(|name| {
                mcp.deferred
                    .as_ref()
                    .is_some_and(|names| names.iter().any(|deferred| deferred == name))
                    && self.is_available(name)
            })
            .collect();
        json!({ "deferred": deferred })
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
            self.config.env_value("OCTOCODE_CARGO"),
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
        self.available_tools().contains(&tool)
    }

    fn available_tools(&self) -> Arc<[&'static str]> {
        self.available_tools
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Host startup check for clasify: when it is available, send one
    /// minimal judgment. A provider that cannot answer (bad key, quota,
    /// unreachable host, invalid response) makes clasify unavailable in this
    /// runtime, so hosts never offer it and no `next.clasify` lead names it.
    /// A rate limit proves the key and endpoint work, so clasify stays.
    pub async fn probe_classification(&self) -> Value {
        let clasify = ToolId::Clasify.as_str();
        if !self.is_available(clasify) {
            return json!({ "probed": false, "available": false });
        }
        let Err(error) = self.clasify_settings().probe().await else {
            return json!({ "probed": true, "available": true });
        };
        let available = error.code == "classificationRateLimited";
        if !available {
            self.classification_unreachable
                .store(true, std::sync::atomic::Ordering::Relaxed);
            let mut tools = self
                .available_tools
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *tools = tools
                .iter()
                .copied()
                .filter(|name| *name != clasify)
                .collect();
        }
        json!({
            "probed": true,
            "available": available,
            "code": error.code,
            "message": error.message,
        })
    }

    fn resolve_available(&self, id: ToolId) -> bool {
        let tool = id.as_str();
        if self.input.runtime_surface != RuntimeSurface::Cli && id.is_cli_only() {
            return false;
        }
        // Local tools follow the local switch; GitHub and other remote tools
        // need no credentials.
        let family = !id.is_local() || self.config.resolved.local.enabled;
        // The tool's own runtime prerequisite (contract policy or setting).
        let prerequisite = match id.availability_config_path() {
            Some("local.beta") => self.config.resolved.local.beta,
            Some("storage.mode") => {
                self.input.runtime_surface == RuntimeSurface::Cli
                    && self.config.resolved.storage.mode == "persistent"
            }
            Some("classification.api") => self
                .classification_key()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            _ => true,
        };
        family
            && prerequisite
            && !self.excluded_by_tool_list(tool)
            && !self.excluded_by_family(tool)
    }

    /// The resolved language-server settings `lspSearch` discovers with.
    pub fn lsp_execution_config(&self) -> &crate::tools::lsp_search::LspExecutionConfig {
        &self.lsp_execution_config
    }

    fn lsp_server_languages(&self) -> Vec<&'static str> {
        let config_path = self
            .lsp_execution_config
            .config_path
            .as_deref()
            .and_then(|path| self.paths.validate_read(path).ok())
            .map(|validated| validated.canonical);
        let root = std::env::current_dir()
            .map(|dir| dir.to_string_lossy().into_owned())
            .unwrap_or_else(|_| ".".to_owned());
        octocode_engine::lsp::config::available_server_languages(
            &root,
            &self.lsp_execution_config.discovery(config_path),
        )
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
                } else if self.excluded_by_family(name) {
                    entry["unavailableReason"] = json!("family");
                } else if self.input.runtime_surface != RuntimeSurface::Cli
                    && ToolId::from_name(name).is_some_and(ToolId::is_cli_only)
                {
                    entry["unavailableReason"] = json!("cliOnly");
                } else if name == ToolId::Clasify.as_str()
                    && self
                        .classification_unreachable
                        .load(std::sync::atomic::Ordering::Relaxed)
                {
                    entry["unavailableReason"] = json!("providerUnreachable");
                }
                entry
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "contractFormatVersion": contract["contractFormatVersion"],
            "fingerprint": contract["fingerprint"],
            "grammarCapabilities": octocode_engine::portable::grammar_capabilities(),
            // Languages whose lspSearch server resolves here (resolution only);
            // hosts render it into the lspSearch description.
            "lspServers": self.lsp_server_languages(),
            "presentation": self.presentation(),
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
        let input = contracts::normalize_input(&tool, input);
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
        let input = contracts::normalize_input(&tool, input);
        let result = match self
            .execute_channel(admission, tool.clone(), input, true)
            .await
        {
            Ok(result) => result,
            Err(error) => return super::error::mcp_input_error(&tool, &error).ok_or(error),
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
        let id = self.admit_tool(&tool, mcp)?;
        let clasify = if id.is_clasify() {
            crate::tools::clasify::reject_response_pagination(&input)?;
            Some(self.clasify_settings())
        } else {
            None
        };
        // Parse response-paging options before contract validation.
        let options = ResponsePageOptions::deserialize(&input).unwrap_or_default();
        let (queries, rejected_rows) = self.admit_queries(id, &input, mcp)?;
        let response_query = if queries.len() == 1 {
            queries[0].clone()
        } else {
            json!({"queries":queries})
        };
        let replay_key = page_replay_key(&tool, mcp, &queries, &rejected_rows);
        let replayed = options
            .response_snapshot
            .as_deref()
            .filter(|_| clasify.is_none())
            .and_then(|snapshot| self.page_replays.get(&replay_key, snapshot));
        let run = ChannelRun {
            id,
            mcp,
            options,
            queries,
            rejected_rows,
            response_query,
            clasify,
            dispatcher: self.dispatcher(),
            output: OutputSettings {
                stats_enabled: config::is_stats_enabled(&self.config.resolved),
                redact_emails: self.config.resolved.output.redact_emails,
                auto_page_chars: self.config.resolved.output.pagination.default_char_length
                    as usize,
                text_format: render::TextFormat::from_config(&self.config.resolved.output.format),
            },
            replayed,
            replays: (self.page_replays.clone(), replay_key),
            family_scope: (!self.family_excluded.is_empty())
                .then(|| format!("tools.family {}", self.config.resolved.tools.family)),
            family_excluded: self.family_excluded.clone(),
        };
        self.requests
            .execute_blocking_admitted(admission, move |context| run.run(&context))
            .await
            .map_err(runtime_execution_error)?
            .map_err(|error| output_contract_error(&tool, error))
    }

    /// The tool behind `tool` when this surface may run it, else the
    /// actionable reason it may not.
    fn admit_tool(&self, tool: &str, mcp: bool) -> Result<ToolId, RuntimeError> {
        // Contract `cliOnly` tools run only from the CLI. MCP never executes
        // them, even if an embedder used a CLI host surface.
        let requested = ToolId::from_name(tool);
        if mcp && requested.is_some_and(ToolId::is_cli_only) {
            return Err(RuntimeError::new(
                "toolUnavailable",
                format!("Tool {tool} is not available through MCP; run `octocode {tool}`."),
            ));
        }
        let unavailable = || {
            RuntimeError::new(
                "toolUnavailable",
                format!("Tool {tool} is not available in this native runtime"),
            )
        };
        if self.is_available(tool) {
            return requested.ok_or_else(unavailable);
        }
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
            let vendor = self.config.resolved.classification.r#type.as_str();
            let p = crate::providers::classification::provider_for(vendor);
            return Err(RuntimeError::new(
                "missingConfiguration",
                format!(
                    "clasify requires OCTOCODE_CLASSIFICATION_API (or the {vendor} \
                         vendor's {}). Create a classification provider API key ({}) \
                         and set OCTOCODE_CLASSIFICATION_API before retrying.",
                    p.key_env(),
                    p.docs_url(),
                ),
            ));
        }
        Err(unavailable())
    }

    /// Validated, security-checked rows to run, and the rows rejected by
    /// index. A batch isolates invalid rows; clasify (whose matrices share
    /// batch-level rules) and a call with no valid row fail as a whole.
    #[allow(clippy::type_complexity)]
    fn admit_queries(
        &self,
        id: ToolId,
        input: &Value,
        mcp: bool,
    ) -> Result<(Vec<Value>, Vec<(usize, Value)>), RuntimeError> {
        let tool = id.as_str();
        let (prepared, mut invalid) = prepare_queries(tool, input, mcp)?;
        if id.is_clasify() {
            crate::tools::clasify::admission::check(prepared.iter().map(|(_, query)| query))
                .map_err(|error| invalid_input_error(tool, error, mcp))?;
        }
        let bulk = input.is_array() || input.get("queries").is_some();
        let mut queries = Vec::with_capacity(prepared.len());
        for (index, query) in prepared {
            let checked = self.security.validate_input_parameters(&query);
            let prefix = if bulk {
                vec!["queries".to_owned(), index.to_string()]
            } else {
                Vec::new()
            };
            let mut issues = security_issues(&checked, &prefix, id.is_clasify());
            if id.is_clasify() {
                issues.extend(crate::tools::clasify::context::unavailable_context_issues(
                    &query,
                    &prefix,
                    |name| self.is_available(name),
                ));
            }
            if issues.is_empty() {
                queries.push(Value::Object(checked.sanitized_params));
            } else {
                invalid.push((index, contracts::ContractValidationError { issues }));
            }
        }
        invalid.sort_by_key(|(index, _)| *index);
        if !invalid.is_empty() && (queries.is_empty() || id.is_clasify()) {
            let issues = invalid
                .into_iter()
                .flat_map(|(_, error)| error.issues)
                .collect();
            return Err(invalid_input_error(
                tool,
                contracts::ContractValidationError { issues },
                mcp,
            ));
        }
        let raw_rows = input
            .get("queries")
            .or(Some(input))
            .and_then(Value::as_array);
        let rejected = invalid
            .iter()
            .map(|(index, error)| {
                let raw = raw_rows
                    .and_then(|rows| rows.get(*index))
                    .cloned()
                    .unwrap_or(Value::Null);
                rejected_row(id, *index, raw, error, mcp)
            })
            .collect();
        Ok((queries, rejected))
    }

    /// The domain execution path for this call's rows.
    fn dispatcher(&self) -> super::domain_dispatch::DomainDispatcher {
        super::domain_dispatch::DomainDispatcher {
            paths: self.paths.clone(),
            security: self.security.clone(),
            regex: LocalFetchRegex::new(self.regex.clone()),
            github_services: self.github_services.clone(),
            github_cache: self.github_cache.clone(),
            config: self.config.clone(),
            home: self.config.home.clone(),
            handle: tokio::runtime::Handle::current(),
            lsp_pool: self.lsp_pool.clone(),
            local_views: self.local_views.clone(),
            lsp_execution_config: self.lsp_execution_config.clone(),
            available_tools: self.available_tools(),
            artifact_cache: self.artifact_cache.clone(),
        }
    }
}

/// Contract-validated rows by input index, and the invalid ones. Row
/// isolation: when only some rows of a batch are invalid, the valid rows
/// still run and the rest become indexed error rows.
#[allow(clippy::type_complexity)]
fn prepare_queries(
    tool: &str,
    input: &Value,
    mcp: bool,
) -> Result<
    (
        Vec<(usize, Value)>,
        Vec<(usize, contracts::ContractValidationError)>,
    ),
    RuntimeError,
> {
    match contracts::prepare_many_and_validate(tool, input.clone(), PrepareOptions::default()) {
        Ok(prepared) => Ok((prepared.into_iter().enumerate().collect(), Vec::new())),
        Err(error) => {
            let Some(rows) = contracts::prepare_rows(tool, input, PrepareOptions::default()) else {
                return Err(invalid_input_error(tool, error, mcp));
            };
            let mut prepared = Vec::with_capacity(rows.len());
            let mut invalid = Vec::new();
            for (index, row) in rows.into_iter().enumerate() {
                match row {
                    Ok(query) => prepared.push((index, query)),
                    Err(error) => invalid.push((index, error)),
                }
            }
            Ok((prepared, invalid))
        }
    }
}

type StageResult = Result<Result<ToolOutcome, contracts::ContractValidationError>, ExecutionError>;

/// Output settings one call reads from the resolved config.
#[derive(Clone, Copy)]
struct OutputSettings {
    stats_enabled: bool,
    redact_emails: bool,
    auto_page_chars: usize,
    text_format: render::TextFormat,
}

/// One admitted call, moved into its blocking worker: admit → run → shape
/// → check → respond.
struct ChannelRun {
    id: ToolId,
    mcp: bool,
    options: ResponsePageOptions,
    queries: Vec<Value>,
    rejected_rows: Vec<(usize, Value)>,
    response_query: Value,
    clasify: Option<crate::tools::clasify::ClasifySettings>,
    dispatcher: super::domain_dispatch::DomainDispatcher,
    output: OutputSettings,
    replayed: Option<PageReplay>,
    replays: (Arc<PageReplayMemo>, [u8; 32]),
    family_excluded: Arc<[&'static str]>,
    family_scope: Option<String>,
}

impl ChannelRun {
    fn run(mut self, context: &ExecutionContext) -> StageResult {
        if let Some(replay) = self.replayed.take()
            && super::source_identity::unchanged_since(
                self.id,
                &self.queries,
                &self.dispatcher.paths,
                replay.started,
            )
        {
            let stage = self.stage(replay.structured, replay.response_query, true);
            return stage::finish(
                stage::StageInput {
                    failure: replay.failure,
                    source_digest: replay.source_digest,
                    ..stage
                },
                context,
            );
        }
        let rejected = std::mem::take(&mut self.rejected_rows);
        let mcp = self.mcp;
        let available = |target: &str| {
            // An MCP-mode call can never execute a contract `cliOnly` tool
            // (see the admission check in `execute_channel`).
            !(mcp && ToolId::from_name(target).is_some_and(ToolId::is_cli_only))
                && self.dispatcher.available_tools.contains(&target)
        };
        // A family preset's cross-family drops are disclosed.
        let disclose = |target: &str| {
            self.family_scope
                .as_deref()
                .filter(|_| self.family_excluded.contains(&target))
        };
        let scope = continuations::Scope {
            is_available: &available,
            disclose: &disclose,
        };
        if self.clasify.is_some() {
            self.run_clasify(rejected, &scope, context)
        } else {
            self.run_rows(rejected, &scope, context)
        }
    }

    /// The shared response-stage input for `structured`; the caller sets the
    /// row facts (failure, source digest).
    fn stage(
        &self,
        structured: Value,
        response_query: Value,
        allow_auto_paging: bool,
    ) -> stage::StageInput {
        stage::StageInput {
            tool: self.id,
            structured,
            response_query,
            options: self.options.clone(),
            mcp: self.mcp,
            failure: None,
            auto_page_chars: self.output.auto_page_chars,
            text_format: self.output.text_format,
            allow_auto_paging,
            source_digest: None,
        }
    }

    /// Clasify runs its own batch and pages at the evidence level
    /// (`next.clasify`), so its response never auto-pages.
    fn run_clasify(
        &self,
        rejected: Vec<(usize, Value)>,
        scope: &continuations::Scope<'_>,
        context: &ExecutionContext,
    ) -> StageResult {
        let Some(clasify) = &self.clasify else {
            return Err(ExecutionError::WorkerFailed);
        };
        let (home, stats_enabled) = (&self.dispatcher.home, self.output.stats_enabled);
        let mut receipts = clasify.call(
            &self.queries,
            rejected,
            &self.dispatcher,
            context,
            |usage| crate::tools::clasify::stats::record_classification(home, stats_enabled, usage),
        )?;
        if let Err(error) = continuations::finalize(
            &mut receipts.structured,
            self.id,
            &continuations::Sources::Matrices(&self.response_query),
            scope,
        ) {
            debug_assert!(false, "invalid clasify continuation: {:?}", error.issues);
            return Ok(Err(error));
        }
        let stage = self.stage(receipts.structured, self.response_query.clone(), false);
        stage::finish(
            stage::StageInput {
                failure: receipts.failure,
                source_digest: receipts.source_digest,
                ..stage
            },
            context,
        )
    }

    /// Ordinary rows: execute, shape each row, envelope, continuations, then
    /// the shared response stage (and a replay seed when the response pages).
    fn run_rows(
        &self,
        rejected: Vec<(usize, Value)>,
        scope: &continuations::Scope<'_>,
        context: &ExecutionContext,
    ) -> StageResult {
        let started = std::time::SystemTime::now();
        let EvaluatedRows {
            mut rows,
            mut failure,
            source_digest,
        } = self.evaluate_rows(context)?;
        let rejected_at: Vec<usize> = rejected.iter().map(|(index, _)| *index).collect();
        merge_rejected_rows(&mut rows, rejected);
        let tool = self.id.as_str();
        let by_row = crate::tools::clasify::handoff::row_queries(&self.queries, &rejected_at);
        let mut structured = rows::envelope_in(rows, self.id, &by_row, &self.dispatcher.paths);
        rows::sanitize_output(
            &mut structured,
            self.id,
            &self.dispatcher.security,
            context,
            self.output.redact_emails,
        )?;
        crate::tools::clasify::handoff::attach(&mut structured, tool, &by_row);
        // An invalid continuation is a runtime defect: it fails loudly in
        // debug builds and withholds its row otherwise.
        if let Err(error) = continuations::finalize(
            &mut structured,
            self.id,
            &continuations::Sources::Rows(&by_row),
            scope,
        ) {
            debug_assert!(false, "invalid {tool} continuation: {:?}", error.issues);
            match contracts::isolate_row_violations(tool, &structured, &error) {
                Some(patched) => {
                    structured = patched;
                    failure = failure.or(Some(FailureKind::Execution));
                }
                None => return Ok(Err(error)),
            }
        }
        // A replay seed only for a response that can page: an explicit
        // window, or one near the automatic page size.
        let pageable = self.options.explicit()
            || crate::tools::stream_page::json_chars(&structured) * 2 > self.output.auto_page_chars;
        let seed = pageable.then(|| PageReplay {
            key: self.replays.1,
            snapshot: String::new(),
            started,
            stored: std::time::Instant::now(),
            bytes: 0,
            structured: structured.clone(),
            response_query: self.response_query.clone(),
            failure,
            source_digest: source_digest.clone(),
        });
        let stage = self.stage(structured, self.response_query.clone(), true);
        let finished = stage::finish(
            stage::StageInput {
                failure,
                source_digest,
                ..stage
            },
            context,
        )?;
        if let (Ok(outcome), Some(seed)) = (&finished, seed) {
            remember_page(&self.replays.0, outcome, seed);
        }
        Ok(finished)
    }

    /// Runs every row (sharing the response window) and shapes it: result
    /// row, diagnostics, cache mark, hint policy, verbose prune, minimize.
    fn evaluate_rows(&self, context: &ExecutionContext) -> Result<EvaluatedRows, ExecutionError> {
        let (id, queries, dispatcher) = (self.id, &self.queries, &self.dispatcher);
        let auto_page_chars = self.output.auto_page_chars;
        let response_page = self.options.response_length.unwrap_or(auto_page_chars);
        let evaluated =
            execute_ordinary_queries(id, queries, Some(response_page), dispatcher, context)?;
        let evaluated = read_share::share_window(
            id,
            queries,
            evaluated,
            (!self.options.explicit()).then_some(auto_page_chars),
            |shortened| {
                execute_ordinary_queries(id, shortened, Some(response_page), dispatcher, context)
            },
        )?;
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
            let mut row = rows::result_row(id, index, query, result.data, result.status);
            rows::attach_diagnostics(&mut row, result.diagnostics);
            if result.cache {
                row["cache"] = json!(1);
            }
            rows::apply_hint_policy(&mut row, id, query);
            verbose::prune_row(&mut row, id, query);
            rows::minimize_row(&mut row, id, query);
            rows.push(row);
        }
        Ok(EvaluatedRows {
            rows,
            failure,
            source_digest,
        })
    }
}

/// Shaped result rows of one call, with the batch's failure kind and the
/// single row's source digest.
struct EvaluatedRows {
    rows: Vec<Value>,
    failure: Option<FailureKind>,
    source_digest: Option<String>,
}

/// Keeps a paged response's pre-paging envelope for its later pages.
fn remember_page(replays: &PageReplayMemo, outcome: &ToolOutcome, mut seed: PageReplay) {
    let pagination = &outcome.structured_content["responsePagination"];
    if pagination["hasMore"] == true
        && let Some(snapshot) = pagination["snapshot"].as_str()
    {
        seed.snapshot = snapshot.to_owned();
        seed.bytes = json_bytes(&seed.structured);
        replays.insert(seed);
    }
}

/// Serialized JSON length of `value` in bytes, without building the string.
fn json_bytes(value: &Value) -> usize {
    struct ByteCounter(usize);
    impl std::io::Write for ByteCounter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = ByteCounter(0);
    serde_json::to_writer(&mut counter, value).map_or(0, |()| counter.0)
}

fn runtime_execution_error(error: ExecutionError) -> RuntimeError {
    RuntimeError::new(
        match error {
            ExecutionError::Closed => "closed",
            ExecutionError::Busy => "busy",
            ExecutionError::DuplicateRequest => "duplicateRequest",
            ExecutionError::Cancelled => "cancelled",
            ExecutionError::Timeout => "timeout",
            ExecutionError::ResponseTooLarge => "responseTooLarge",
            ExecutionError::UnroutedTool => "unroutedTool",
            ExecutionError::InvalidLimits | ExecutionError::WorkerFailed => "executionFailed",
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
    use super::*;
    use stage::{isolate_output_rows, response_all_failed};

    fn mcp_from_rows(mut structured: Value) -> Value {
        assert!(isolate_output_rows("ghCloneRepo", &mut structured).expect("row isolation"));
        let all_failed = response_all_failed(&structured);
        let text = render::render_tool(
            ToolId::GhCloneRepo,
            &structured,
            &json!({"owner":"a","repo":"b"}),
            render::TextFormat::Yaml,
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
            tool: ToolId::GhCloneRepo,
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
                    "complete":true,"resolvedRef":"main"
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
mod security_issue_tests {
    use super::*;

    /// Clasify evidence may be redacted in place; a credential-looking value
    /// in a resource read's `query` would run redacted, so it is rejected.
    #[test]
    fn a_secret_in_a_clasify_resource_query_is_rejected_not_redacted() {
        let token = format!("ghp_{}", "a".repeat(36));
        let checked = crate::security::ContentSecurity::new().validate_input_parameters(
            &json!({"resources":[
                {"id":"held","value":token},
                {"id":"read","tool":"localSearch","query":{"path":".","matchString":token}}
            ]}),
        );
        let issues = security_issues(&checked, &[], true);
        let paths = issues
            .iter()
            .map(|issue| issue.path.join("."))
            .collect::<Vec<_>>();
        assert_eq!(paths, ["resources[].query.matchString"], "{issues:?}");
        assert!(
            issues
                .iter()
                .all(|issue| issue.rule_id == "security.credential"),
            "{issues:?}"
        );
    }
}

#[cfg(test)]
mod construction_tests {
    use super::*;
    use crate::config::FileInput;

    fn runtime(home: &std::path::Path, env: &[(&str, &str)]) -> ToolRuntime {
        let mut vars: BTreeMap<String, String> = env
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect();
        vars.insert("OCTOCODE_HOME".into(), home.to_string_lossy().into_owned());
        ToolRuntime::new(crate::config::ConfigInput {
            env: vars,
            cwd: home.to_path_buf(),
            os_home: home.to_path_buf(),
            trusted_project: false,
            global_env: FileInput::Missing {
                path: home.join(".env"),
            },
            project_env: FileInput::Missing {
                path: home.join(".octocode/.env"),
            },
            config_file: FileInput::Missing {
                path: home.join(".octocoderc"),
            },
            project_config_file: FileInput::Missing {
                path: home.join(".octocode/.octocoderc"),
            },
            runtime_surface: RuntimeSurface::Mcp,
        })
        .expect("runtime")
    }

    #[test]
    fn availability_is_resolved_once_per_runtime() {
        let home = tempfile::tempdir().expect("home");
        let runtime = runtime(
            home.path(),
            &[
                ("ENABLE_LOCAL", "false"),
                ("OCTOCODE_TOOL_FAMILY", "github"),
            ],
        );
        for id in ToolId::ALL {
            assert_eq!(
                runtime.is_available(id.as_str()),
                runtime.resolve_available(id),
                "{id}"
            );
            assert_eq!(
                runtime.family_excluded.contains(&id.as_str()),
                runtime.excluded_by_family(id.as_str()),
                "{id}"
            );
        }
        assert!(!runtime.is_available("localSearch"));
        assert!(!runtime.is_available("noSuchTool"));
    }

    #[test]
    fn only_persistent_storage_keeps_a_registry_cache() {
        let home = tempfile::tempdir().expect("home");
        let memory = runtime(home.path(), &[("OCTOCODE_STORAGE_MODE", "memory")]);
        assert!(memory.artifact_cache.is_none());
        let persistent = runtime(home.path(), &[("OCTOCODE_STORAGE_MODE", "persistent")]);
        assert!(persistent.artifact_cache.is_some());
    }

    #[test]
    fn replay_bytes_are_the_serialized_length() {
        let value = json!({"a": "é😀\n\"", "b": [1, 2.5, null]});
        assert_eq!(json_bytes(&value), value.to_string().len());
    }
}
