mod admission;
mod channel;
mod page_replay;

use crate::config::{self, ConfigInput, ConfigOutput, RuntimeSurface};
use crate::contracts;
use crate::execution::cancel::CancellationCheck;
use crate::execution::{
    Cost, ExecutionContext, ExecutionError, RequestLimits, RequestRuntime, RuntimeLimits,
};
use crate::id::{ToolFamily, ToolId};
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::response::exit::ExitClass;
use crate::response::outcome::ToolOutcome;
use crate::response::pager::ResponsePageOptions;
use crate::response::render;
use crate::security::ContentSecurity;
use crate::tools::ast_graph::store as graph_store;
use admission::{AdmittedRows, output_contract_error};
use channel::{ChannelRun, OutputSettings};
use page_replay::{PageReplayMemo, page_replay_key};

use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostOptions {
    pub cwd: Option<PathBuf>,
    /// Explicit host environment. Embedders use this to keep configuration
    /// injection deterministic; standalone surfaces fall back to process env.
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub surface: RuntimeSurface,
    /// Override the per-request execution timeout in seconds (default:
    /// [`crate::execution::DEFAULT_TIMEOUT`], the budget of every surface).
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

    #[must_use]
    pub fn exit_class(&self) -> ExitClass {
        ExitClass::of_code(&self.code)
    }
}

pub struct ToolRuntime {
    pub requests: RequestRuntime,
    input: ConfigInput,
    config: Arc<ConfigOutput>,
    /// Internal limits: code constants ([`RuntimeLimits::DEFAULT`]), never
    /// configuration.
    limits: RuntimeLimits,
    paths: Arc<PathPolicy>,
    security: Arc<ContentSecurity>,
    github_cache: super::github_cache::GitHubContentCache,
    github_services: Arc<
        std::sync::OnceLock<Result<super::github::GitHubServices, octocode_github::ProviderError>>,
    >,
    lsp_pool: Arc<octocode_engine::lsp::pool::LspClientPool>,
    /// Whether this runtime created [`Self::lsp_pool`]: only its creator
    /// stops the servers on close; a runtime given a shared pool leaves them
    /// to the pool's owner and to its other runtimes.
    owns_lsp_pool: bool,
    /// Sanitized full views of recently paged local files (this runtime's
    /// security policy only), so each localFetch page skips a full rescan.
    local_views: Arc<crate::security::scan::SanitizedViewMemo>,
    /// Pre-paging envelopes of recent paged responses (this runtime's config
    /// and workspace only), so a later page skips re-executing the batch.
    page_replays: Arc<PageReplayMemo>,
    section_sessions: Arc<crate::tools::clasify::sections::Sessions>,
    /// Registry answers for artifactSearch; `None` under memory storage.
    artifact_cache: Option<Arc<crate::providers::artifact::ArtifactCache>>,
    /// Resolved once from the immutable config and surface. Provider
    /// failures are request-local and do not remove an advertised tool.
    availability: Arc<crate::execution::availability::ToolAvailability>,
    /// Provider settings and selection availability, resolved once.
    evaluator: crate::tools::judgment::Evaluator,
    selection_available: bool,
    lsp_execution_config: Arc<crate::tools::lsp_search::LspExecutionConfig>,
    /// The sandbox workspace root (configured workspaceRoot, else host cwd).
    workspace_root: PathBuf,
    /// The built-in classification provider.
    classification: &'static dyn crate::providers::classification::ClassificationProvider,
}

/// The configuration diagnostics every runtime start prints. A key no
/// setting reads (unknown, or removed by the contract) changes nothing, so
/// `octocode config` reports it instead of every call; an ignored value of a
/// live setting (invalid, protected, an unreadable file) is said each time.
fn startup_diagnostics(
    diagnostics: &[config::ConfigDiagnostic],
) -> impl Iterator<Item = &config::ConfigDiagnostic> {
    diagnostics.iter().filter(|diagnostic| {
        ![config::UNKNOWN_CONFIG_CODE, config::REMOVED_SETTING_CODE]
            .contains(&diagnostic.code.as_str())
    })
}

/// Largest file a [`Cost::Light`] local row reads.
const LIGHT_FILE_BYTES: u64 = 1024 * 1024;

/// What the caller of one call reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    /// The structured JSON only (a CLI writing JSON).
    Structured,
    /// The structured JSON and the rendered text (a CLI on a terminal).
    Rendered,
    /// An MCP CallToolResult: both, under MCP admission (no `cliOnly` tool).
    Mcp,
}

impl ToolRuntime {
    pub fn from_host(options: HostOptions) -> Result<Self, RuntimeError> {
        Self::from_host_sharing(options, None)
    }

    /// [`Self::from_host`] whose language servers come from `lsp_pool`, a
    /// pool its owner shares across runtimes (the CLI's warm server keeps
    /// one runtime per working directory and one set of servers). Closing
    /// this runtime leaves the pool running; the owner clears it.
    pub fn from_host_with_lsp_pool(
        options: HostOptions,
        lsp_pool: Arc<octocode_engine::lsp::pool::LspClientPool>,
    ) -> Result<Self, RuntimeError> {
        Self::from_host_sharing(options, Some(lsp_pool))
    }

    /// Print the configuration diagnostics every runtime start prints: on
    /// stderr, since stdout carries CLI results and MCP JSON-RPC. A host
    /// that answers a call from another process's runtime (the CLI's warm
    /// server client) prints its own configuration's, as an in-process
    /// start would.
    pub fn print_startup_diagnostics(config: &ConfigOutput) {
        for diagnostic in startup_diagnostics(&config.diagnostics) {
            eprintln!("{diagnostic}");
        }
    }

    fn from_host_sharing(
        options: HostOptions,
        lsp_pool: Option<Arc<octocode_engine::lsp::pool::LspClientPool>>,
    ) -> Result<Self, RuntimeError> {
        // Reject a bad timeout before config acquisition or maintenance run.
        if let Some(secs) = options.timeout_secs {
            let max = crate::execution::MAX_TIMEOUT.as_secs();
            if !(1..=max).contains(&secs) {
                return Err(RuntimeError::new(
                    "invalidInput",
                    format!("timeoutSecs must be between 1 and {max}; got {secs}"),
                ));
            }
        }
        let cwd = options
            .cwd
            .or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| RuntimeError::new("config", "Cannot resolve working directory"))?;
        let home = std::env::home_dir()
            .ok_or_else(|| RuntimeError::new("config", "Cannot resolve home directory"))?;
        let input = config::acquire_config_input(
            options.env.unwrap_or_else(octocode_io::env::vars_lossy),
            cwd,
            home,
            options.surface,
        );
        let mut runtime = Self::build(input, RuntimeLimits::DEFAULT, lsp_pool)?;
        // Misconfiguration never fails startup; say what was ignored and
        // where.
        Self::print_startup_diagnostics(&runtime.config);
        // storage.mode=memory must not touch the disk cache at all.
        if config::is_persistent_storage_enabled(&runtime.config.resolved) {
            super::maintenance::run_if_due(&runtime.config.home, runtime.limits.clone.ttl);
        }
        if let Some(secs) = options.timeout_secs {
            // An embedder's budget replaces the surface default.
            runtime.requests = RequestRuntime::new(RequestLimits {
                timeout: std::time::Duration::from_secs(secs),
                ..RequestLimits::default()
            })
            .map_err(runtime_execution_error)?;
        }
        Ok(runtime)
    }

    pub fn new(input: ConfigInput) -> Result<Self, RuntimeError> {
        Self::with_limits(input, RuntimeLimits::DEFAULT)
    }

    /// Tests only; production uses [`Self::new`]. A runtime whose internal
    /// limits are `limits` instead of the code constants. Not configuration:
    /// nothing reads them from the environment or a file.
    pub fn with_limits(input: ConfigInput, limits: RuntimeLimits) -> Result<Self, RuntimeError> {
        Self::build(input, limits, None)
    }

    /// A runtime for `input` whose language servers come from `lsp_pool`
    /// when given, else from a pool of its own.
    fn build(
        input: ConfigInput,
        limits: RuntimeLimits,
        lsp_pool: Option<Arc<octocode_engine::lsp::pool::LspClientPool>>,
    ) -> Result<Self, RuntimeError> {
        let config = Arc::new(config::resolve_config(&input));
        let local = &config.resolved.local;
        let octocode_home = config.home.clone();
        let mut additional_roots: Vec<PathBuf> =
            local.allowed_paths.iter().map(PathBuf::from).collect();
        // Local tools read the GitHub checkouts (clones, materialized trees)
        // and clasify URL captures under the cache home, never the rest of
        // it (credentials, logs).
        additional_roots.extend([
            crate::tools::gh_clone_repo::cache::clones_root(&octocode_home),
            crate::tools::gh_structure::materialize::materialize_root(&octocode_home),
            crate::tools::source_capture::url::capture_root(&octocode_home),
        ]);
        // Default sandbox: the configured workspace, or the process cwd when
        // unconfigured — never all of $HOME. Broad $HOME access is opt-in
        // only, via an explicit workspaceRoot or an allowedPaths entry.
        let workspace_root = local
            .workspace_root
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| input.cwd.clone());
        let paths = PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(workspace_root.clone()),
            additional_roots,
            include_home: false,
            home_dir: Some(input.os_home.clone()),
        })
        .map_err(|error| RuntimeError::new("policy", error.message))?;
        let security = ContentSecurity::new();
        let requests =
            RequestRuntime::new(RequestLimits::default()).map_err(runtime_execution_error)?;
        // Each response cache owns its directory, so one cache's budget or
        // clear never touches the other's entries.
        let persistent = config::is_persistent_storage_enabled(&config.resolved);
        let response_dir = persistent.then(|| octocode_home.join("tmp").join("response"));
        if persistent {
            crate::tools::declarations_cache::persist_symbols_views(
                octocode_home.join("tmp").join("outline"),
            );
        }
        let github_cache = super::github_cache::GitHubContentCache::new(
            crate::cache::CacheConfig::default(),
            response_dir,
        );
        let artifact_cache = persistent.then(|| {
            Arc::new(crate::providers::artifact::ArtifactCache::new(Some(
                octocode_home.join("tmp").join("artifact"),
            )))
        });
        let lsp_execution_config = Arc::new(crate::tools::lsp_search::LspExecutionConfig {
            config_path: config.resolved.lsp.config_path.clone(),
            trust_project_config: config.resolved.lsp.trust_project_config,
            // A resolved setting the engine reads by name: `lsp.cacheDir` may
            // come from `.octocoderc`, not only the environment, and the
            // resolved value wins either way.
            env: config
                .effective_env
                .iter()
                .filter(|(key, _)| key.starts_with("OCTOCODE_"))
                .map(|(key, value)| (key.clone(), value.clone()))
                .chain(
                    config::field::LSP_CACHE_DIR
                        .env_name()
                        .zip(config.resolved.lsp.cache_dir.clone())
                        .map(|(name, dir)| (name.to_owned(), dir)),
                )
                .collect(),
            octocode_home: Some(octocode_home),
            invocation_dir: Some(input.cwd.clone()),
            lead_cache: std::sync::Arc::default(),
            prewarm_state: std::sync::Arc::default(),
        });
        let classification = crate::providers::classification::provider();
        let evaluator = evaluator(&config, &limits, classification, persistent);
        let availability = Arc::new(crate::execution::availability::ToolAvailability::resolve(
            &config,
            input.runtime_surface,
        ));
        let selection_available = selection_available(&config, &availability);
        let page_replays = Arc::new(PageReplayMemo::new(&config, &input.cwd));
        let section_sessions = Arc::new(crate::tools::clasify::sections::Sessions::new(
            PageReplayMemo::cache_config(),
            persistent.then(|| config.home.join("tmp").join("section-assessments")),
            page_replays.policy.clone(),
        ));
        Ok(Self {
            requests,
            input,
            config,
            limits,
            classification,
            availability,
            evaluator,
            selection_available,
            paths: Arc::new(paths),
            security: Arc::new(security),
            github_cache,
            github_services: Arc::new(std::sync::OnceLock::new()),
            owns_lsp_pool: lsp_pool.is_none(),
            lsp_pool: lsp_pool.unwrap_or_default(),
            local_views: Arc::default(),
            page_replays,
            section_sessions,
            artifact_cache,
            lsp_execution_config,
            workspace_root,
        })
    }

    pub fn config(&self) -> &ConfigOutput {
        &self.config
    }
    pub fn begin_close(&self) {
        self.requests.begin_close();
    }
    pub async fn close(&self) {
        self.begin_close();
        self.requests.close().await;
        if self.owns_lsp_pool {
            self.lsp_pool.clear_all().await;
        }
        self.github_cache.clear_memory();
        self.page_replays.store.clear_memory();
        self.section_sessions.clear_memory();
    }
    pub fn inspect_config(&self) -> config::ConfigInspectorData {
        config::inspector_data(&self.input, &self.config)
    }

    /// Resolved presentation switches shared by MCP and CLI discovery.
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
        json!({
            "deferred": deferred,
            "selection": self.selection_available,
        })
    }

    /// CLI `graph ingest`: builds and publishes a code-graph snapshot under
    /// the same path policy and content security as the local tools.
    pub fn graph_ingest(&self, options: &graph_store::IngestOptions) -> graph_store::GraphOutput {
        if !self.config.resolved.local.enabled {
            return graph_local_disabled();
        }
        let options = graph_store::IngestOptions {
            workspace: Some(self.graph_workspace(options.workspace.as_deref())),
            ..options.clone()
        };
        graph_store::ingest(
            &options,
            &self.paths,
            &self.security,
            &crate::execution::cancel::NeverCancel,
            self.config.env_value(config::environment::OCTOCODE_CARGO),
        )
    }

    /// CLI `graph query`: answers one bounded question from a snapshot.
    pub fn graph_query(&self, options: &graph_store::QueryOptions) -> graph_store::GraphOutput {
        if !self.config.resolved.local.enabled {
            return graph_local_disabled();
        }
        let options = graph_store::QueryOptions {
            workspace: Some(self.graph_workspace(options.workspace.as_deref())),
            ..options.clone()
        };
        graph_store::query(&options, &self.paths)
    }

    /// The workspace that owns a graph: `explicit`, else the one this
    /// runtime's working directory belongs to (not the process's).
    fn graph_workspace(&self, explicit: Option<&std::path::Path>) -> PathBuf {
        explicit.map_or_else(
            || graph_store::default_workspace(self.input.cwd.clone(), &self.paths),
            std::path::Path::to_path_buf,
        )
    }

    pub fn is_available(&self, tool: &str) -> bool {
        self.availability.is_available(tool)
    }

    /// Whether `tool` runs on this runtime and why not; `None` for an unknown
    /// tool.
    pub fn availability(
        &self,
        tool: &str,
    ) -> Option<&crate::execution::availability::Availability> {
        self.availability.get(tool)
    }

    /// The resolved language-server settings `lspSearch` discovers with.
    pub fn lsp_execution_config(&self) -> &crate::tools::lsp_search::LspExecutionConfig {
        &self.lsp_execution_config
    }

    /// The root `lspServers` resolve from: the runtime's workspace (host cwd
    /// or configured workspaceRoot), never the process cwd.
    fn lsp_root(&self) -> &std::path::Path {
        &self.workspace_root
    }

    fn lsp_server_languages(&self) -> Vec<&'static str> {
        let config_path = self
            .lsp_execution_config
            .config_path
            .as_deref()
            .and_then(|path| self.paths.validate_read(path).ok())
            .map(|validated| validated.canonical);
        octocode_engine::lsp::config::available_server_languages(
            &self.lsp_root().to_string_lossy(),
            &self.lsp_execution_config.discovery(config_path),
        )
    }

    /// Runtime truth only: tool names, availability, grammar capabilities, and
    /// the enforcement contract fingerprint. Presentation and MCP instructions
    /// are delivered to agents by the JS layers from `@octocodeai/octocode-core`.
    pub fn catalog(&self) -> Value {
        // Generated constants only: the catalog never parses the contract.
        let tools = ToolId::ALL
            .into_iter()
            .map(|id| {
                let name = id.as_str();
                let mut entry = json!({
                    "name": name,
                    "shortDescription": id.short_description(),
                    "available": self.is_available(name),
                });
                if let Some(reason) = self.availability(name).and_then(|a| a.reason_code()) {
                    entry["unavailableReason"] = json!(reason);
                }
                entry
            })
            .collect::<Vec<_>>();
        json!({
            "fingerprint": octocode_contract_types::TOOL_TYPES_CONTRACT_FINGERPRINT,
            "grammarCapabilities": octocode_engine::signatures::languages::grammar_capabilities(),
            // Languages whose lspSearch server resolves here (resolution only);
            // hosts render it into the lspSearch description.
            "lspServers": self.lsp_server_languages(),
            "presentation": self.presentation(),
            "tools": tools,
        })
    }

    pub fn admit(
        &self,
        request_id: String,
    ) -> Result<crate::execution::RequestAdmission, RuntimeError> {
        self.requests
            .admit(request_id)
            .map_err(runtime_execution_error)
    }

    /// [`Self::admit`] with the host's per-request options
    /// (`{githubToken?}`). A supplied token is the request's only GitHub
    /// credential; a request without one resolves credentials as before.
    pub fn admit_with(
        &self,
        request_id: String,
        options: Option<Value>,
    ) -> Result<crate::execution::RequestAdmission, RuntimeError> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct RequestOptions {
            github_token: Option<String>,
        }
        let token = match options {
            None | Some(Value::Null) => None,
            // Never echo the options: they carry a secret.
            Some(value) => serde_json::from_value::<RequestOptions>(value)
                .map_err(|_| RuntimeError::new("invalidInput", "Invalid request options"))?
                .github_token
                .map(|token| {
                    if token.trim().is_empty() {
                        Err(RuntimeError::new(
                            "invalidInput",
                            "Request option githubToken is empty",
                        ))
                    } else {
                        Ok(SecretString::from(token))
                    }
                })
                .transpose()?,
        };
        let admission = self.admit(request_id)?;
        Ok(match token {
            Some(token) => admission.with_github_token(token),
            None => admission,
        })
    }

    /// Run one admitted call and answer in `channel`. Every interface
    /// (CLI, warm server, N-API/MCP) executes through here; a rejected call
    /// projects once through [`super::error::tool_error_result`].
    pub async fn execute(
        &self,
        admission: crate::execution::RequestAdmission,
        tool: String,
        input: Value,
        channel: Channel,
    ) -> Result<ToolOutcome, RuntimeError> {
        let input = contracts::normalize_input(&tool, input);
        self.execute_channel(
            admission,
            tool,
            input,
            channel == Channel::Mcp,
            channel != Channel::Structured,
        )
        .await
    }

    async fn execute_channel(
        &self,
        admission: crate::execution::RequestAdmission,
        tool: String,
        input: Value,
        mcp: bool,
        render_text: bool,
    ) -> Result<ToolOutcome, RuntimeError> {
        let id = match self.admit_tool(&tool, mcp) {
            Ok(id) => id,
            // A setting the call lacks is reported after its input: an
            // invalid query fails as invalidInput whatever is configured.
            Err(error) => {
                if error.code == "missingConfiguration"
                    && let Some(id) = ToolId::from_name(&tool)
                    && let Err(invalid) = self.admit_queries(id, &input)
                    && invalid.code == "invalidInput"
                {
                    return Err(invalid);
                }
                return Err(error);
            }
        };
        let clasify = id.is_clasify();
        // Parse response-paging options before contract validation.
        let mut options = ResponsePageOptions::deserialize(&input).unwrap_or_default();
        options.render_text = Some(render_text);
        let access = page_replay_key(&tool, mcp, &[], &[], admission.github_credential());
        let (queries, rejected_rows, row_indices, response_query, replay_key, replayed) =
            if options.response_snapshot.is_some() || options.response_offset.unwrap_or(0) > 0 {
                let (key, replay) = self.open_saved(&tool, &input, &mut options, access)?;
                (
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    replay.response_query.clone(),
                    key,
                    Some(replay),
                )
            } else {
                if clasify && options.response_scope.is_none() {
                    options.response_scope = Some("rows".into());
                }
                let AdmittedRows {
                    queries,
                    rejected: rejected_rows,
                } = self.admit_queries(id, &input)?;
                let response_query = if queries.len() == 1 {
                    queries[0].clone()
                } else {
                    json!({"queries": queries})
                };
                let rejected_at: Vec<usize> =
                    rejected_rows.iter().map(|(index, _)| *index).collect();
                let row_indices = super::row_indices::admitted_indices(queries.len(), &rejected_at);
                let key = page_replay_key(
                    &tool,
                    mcp,
                    &queries,
                    &row_indices,
                    admission.github_credential(),
                );
                (
                    queries,
                    rejected_rows,
                    row_indices,
                    response_query,
                    key,
                    None,
                )
            };
        let cost = self.request_cost(id, &queries, replayed.is_some());
        let dispatcher = self.dispatcher();
        let run = ChannelRun {
            id,
            mcp,
            options,
            queries,
            rejected_rows,
            row_indices,
            response_query,
            clasify,
            dispatcher,
            output: OutputSettings {
                redact_emails: self.config.resolved.output.redact_emails,
                auto_page_chars: self.limits.auto_page_chars,
                text_format: render::TextFormat::from_config(&self.config.resolved.output.format),
            },
            replayed,
            replays: (self.page_replays.clone(), replay_key),
            section_sessions: self.section_sessions.clone(),
            access,
        };
        self.requests
            .execute_blocking_admitted(admission.with_cost(cost), move |context| {
                if let Err(error) = run.check_replay_access(&context) {
                    return Ok(Err(error));
                }
                run.run(&context)
                    .map(|result| result.map_err(|error| output_contract_error(&tool, error)))
            })
            .await
            .map_err(runtime_execution_error)?
    }

    /// The admission [`Cost`] of `queries`: light for a provider page the
    /// memo serves (no dispatch, no source check) or for local rows the local
    /// runner serves that each read one regular file of at most
    /// [`LIGHT_FILE_BYTES`] and judge nothing. Anything else may walk a tree,
    /// or wait on a provider (a `clasifyContext` or `selection` row asks the
    /// classification provider) or a language server.
    fn request_cost(&self, id: ToolId, queries: &[Value], replayed: bool) -> Cost {
        if replayed {
            return Cost::Light;
        }
        let light_row = |row: &Value| {
            let judged = row.get("clasifyContext").is_some() || row.get("selection").is_some();
            !judged
                && row["path"].as_str().is_some_and(|path| {
                    std::fs::metadata(self.paths.expand_and_resolve(std::path::Path::new(path)))
                        .is_ok_and(|meta| meta.is_file() && meta.len() <= LIGHT_FILE_BYTES)
                })
        };
        let light = match id.family() {
            ToolFamily::GitHub | ToolFamily::Remote => replayed,
            ToolFamily::Local => {
                id != ToolId::LspSearch && !queries.is_empty() && queries.iter().all(light_row)
            }
        };
        if light { Cost::Light } else { Cost::Walk }
    }

    /// The domain execution path for this call's rows.
    fn dispatcher(&self) -> super::domain_dispatch::DomainDispatcher {
        super::domain_dispatch::DomainDispatcher {
            paths: self.paths.clone(),
            security: self.security.clone(),
            github_services: self.github_services.clone(),
            github_cache: self.github_cache.clone(),
            github_access: super::github::ResponseAccess::default(),
            config: self.config.clone(),
            limits: self.limits,
            home: self.config.home.clone(),
            handle: tokio::runtime::Handle::current(),
            lsp_pool: self.lsp_pool.clone(),
            local_views: self.local_views.clone(),
            lsp_execution_config: self.lsp_execution_config.clone(),
            availability: self.availability.clone(),
            selection_available: self.selection_available,
            evaluator: self.evaluator.clone(),
            artifact_cache: self.artifact_cache.clone(),
        }
    }
}

/// `graph` reads local files, so it runs only where local tools do.
fn graph_local_disabled() -> graph_store::GraphOutput {
    graph_store::GraphOutput::error(
        "missingConfiguration",
        format!(
            "Local tools are disabled; graph commands read local files. Set {}.",
            config::field::LOCAL_ENABLED.display_name()
        ),
    )
}

/// The one classification evaluator of a runtime: public clasify and every
/// ranked read share its client, judgment cache and accounting. Judgments
/// persist across processes only under persistent storage.
fn evaluator(
    config: &ConfigOutput,
    limits: &RuntimeLimits,
    provider: &'static dyn crate::providers::classification::ClassificationProvider,
    persistent: bool,
) -> crate::tools::judgment::Evaluator {
    crate::tools::judgment::Evaluator::new(
        crate::execution::availability::classification_key(config)
            .map(|value| SecretString::from(value.to_owned())),
        config::field::CLASSIFICATION_API_HOST
            .env_name()
            .and_then(|env| config.env_value(env))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| provider.defaults().host.to_owned()),
        config
            .resolved
            .classification
            .model
            .clone()
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| provider.defaults().model.to_owned()),
        provider,
        limits.network_timeout,
        u32::from(limits.max_retries),
        limits.classification_concurrency,
        persistent.then(|| config.home.join("tmp").join("judgment")),
        config::is_stats_enabled(&config.resolved).then(|| config.home.clone()),
    )
}

/// A visible read tool may use the provider through its opt-in `clasifyContext`
/// field even when the public clasify tool is hidden; eligibility comes from
/// the embedded contract instead of another tool list.
fn selection_available(
    config: &ConfigOutput,
    availability: &crate::execution::availability::ToolAvailability,
) -> bool {
    crate::execution::availability::classification_key(config).is_some()
        && availability
            .available()
            .iter()
            .filter_map(|name| ToolId::from_name(name))
            .any(|id| {
                id != ToolId::Clasify
                    && crate::contracts::query_schema_value(id, None, "clasifyContext", "type")
                        .is_some()
            })
}

fn runtime_execution_error(error: ExecutionError) -> RuntimeError {
    RuntimeError::new(error.code(), error.to_string())
}

impl CancellationCheck for ExecutionContext {
    fn check(&self) -> Result<(), String> {
        ExecutionContext::check(self).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod output_recovery_tests {
    use super::*;
    use crate::response::pager::TextContent;
    use crate::response::result::FailureKind;
    use crate::response::stage::{isolate_output_rows, response_all_failed};

    fn mcp_from_rows(mut structured: Value) -> Value {
        assert!(isolate_output_rows("ghCloneRepo", &mut structured).expect("row isolation"));
        let all_failed = response_all_failed(&structured);
        let text = render::render_tool(
            ToolId::GhCloneRepo,
            &structured,
            &json!({"owner":"a","repo":"b"}),
            render::TextFormat::Yaml,
        );
        super::super::error::mcp_result(
            "ghCloneRepo",
            Ok(ToolOutcome {
                structured_content: structured,
                content: vec![TextContent {
                    r#type: "text".into(),
                    text,
                }],
                failure: Some(FailureKind::Execution),
                all_failed,
            }),
        )
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
                "owner":"a","repo":"b","bytes":0,
                "location":{
                    "kind":"repo","localPath":"/tmp/repo",
                    "cached":false,"commitSha":"abc",
                    "resolvedRef":"main"
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
mod construction_tests {
    use super::*;
    use crate::config::FileInput;
    use crate::tools::source_capture::SourceReader;

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

    async fn section_call(
        runtime: &ToolRuntime,
        query: Value,
        channel: Channel,
        token: &str,
    ) -> Value {
        let admission = runtime
            .admit("sections".into())
            .expect("admit")
            .with_github_token(SecretString::from(token.to_owned()));
        runtime
            .execute(admission, "clasify".into(), query, channel)
            .await
            .expect("sections response")
            .structured_content
    }

    async fn section_fixture(
        home: &std::path::Path,
        mode: &str,
    ) -> (ToolRuntime, wiremock::MockServer) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::builder().start().await;
        Mock::given(method("POST")).and(path("/v1/systemone"))
            .respond_with(|request: &wiremock::Request| {
                let body: Value = serde_json::from_slice(&request.body).expect("body");
                let answers: serde_json::Map<String, Value> = body["questions"].as_object().expect("questions")
                    .keys().map(|key| (key.clone(), json!({"type":"noul","noul":0.9}))).collect();
                ResponseTemplate::new(200).set_body_json(json!({"model":"jev-1.13.0","answers":answers,"usage":{"input_tokens":10,"output_tokens":2}}))
            }).mount(&server).await;
        let runtime = runtime(
            home,
            &[
                ("OCTOCODE_BETA", "true"),
                ("OCTOCODE_CLASSIFICATION_API", "secret"),
                ("OCTOCODE_CLASSIFICATION_API_HOST", &server.uri()),
                ("OCTOCODE_STORAGE_MODE", mode),
            ],
        );
        let mut limits = RuntimeLimits::DEFAULT;
        limits.selection.max_candidates = 1;
        let runtime =
            ToolRuntime::with_limits(runtime.input.clone(), limits).expect("bounded runtime");
        (runtime, server)
    }

    fn section_query(home: &std::path::Path) -> Value {
        std::fs::write(
            home.join("first.txt"),
            "First source: restart the service.\n",
        )
        .expect("first");
        std::fs::write(
            home.join("second.txt"),
            "Second source: stop the service.\n",
        )
        .expect("second");
        json!({"queries":[{"id":"sections","operation":"sections","resources":{
            "first":{"tool":"localFetch","query":{"path":home.join("first.txt"),"fullContent":true}},
            "second":{"tool":"localFetch","query":{"path":home.join("second.txt"),"fullContent":true}}
        },"questions":{"restart":{"type":"noul","instructions":"Does this section require a restart?"}}}]})
    }

    #[tokio::test]
    async fn sections_runtime_resumes_replays_and_partitions_access_and_policy() {
        for mode in ["memory", "persistent"] {
            let home = tempfile::tempdir().expect("home");
            let (runtime, server) = section_fixture(home.path(), mode).await;
            let channel = if mode == "memory" {
                Channel::Mcp
            } else {
                Channel::Structured
            };
            let first =
                section_call(&runtime, section_query(home.path()), channel, "caller-a").await;
            let next = first["queries"][0]["next"]["nextSectionPage"]["query"].clone();
            assert!(next.is_object(), "{first}");
            assert_eq!(server.received_requests().await.unwrap().len(), 1);
            let denied = section_call(&runtime, next.clone(), channel, "caller-b").await;
            assert!(
                denied
                    .to_string()
                    .contains("classificationContextUnavailable"),
                "{denied}"
            );
            assert_eq!(server.received_requests().await.unwrap().len(), 1);
            let runtime = if mode == "persistent" {
                let mut changed = runtime.input.clone();
                changed
                    .env
                    .insert("OCTOCODE_ENABLE_LOCAL".into(), "false".into());
                let other =
                    ToolRuntime::with_limits(changed, runtime.limits).expect("other policy");
                let denied = section_call(&other, next.clone(), channel, "caller-a").await;
                assert!(
                    denied
                        .to_string()
                        .contains("classificationContextUnavailable"),
                    "{denied}"
                );
                runtime.close().await;
                ToolRuntime::with_limits(runtime.input.clone(), runtime.limits).expect("reopened")
            } else {
                runtime
            };
            let second = section_call(&runtime, next.clone(), channel, "caller-a").await;
            assert_eq!(
                second["queries"][0]["resources"]["second"]["sections"][0]["answers"]["restart"]["status"],
                "answered",
                "{second}"
            );
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
            let replay = section_call(&runtime, next, channel, "caller-a").await;
            assert_eq!(
                replay["queries"][0]["resources"],
                second["queries"][0]["resources"]
            );
            assert_eq!(replay["queries"][0]["replayed"], true);
            assert_eq!(server.received_requests().await.unwrap().len(), 2);
            runtime.close().await;
            if mode == "memory" {
                assert!(!home.path().join("tmp/section-assessments").exists());
            }
        }
    }

    #[tokio::test]
    async fn sections_runtime_refuses_changed_sources_and_one_shot_memory_continuations() {
        let home = tempfile::tempdir().expect("home");
        let (runtime, server) = section_fixture(home.path(), "memory").await;
        let query = section_query(home.path());
        let one_shot = section_call(&runtime, query.clone(), Channel::Structured, "caller").await;
        assert!(one_shot.to_string().contains("unassessed"), "{one_shot}");
        assert!(one_shot["queries"][0].get("next").is_none());
        assert!(server.received_requests().await.unwrap().is_empty());
        let first = section_call(&runtime, query, Channel::Mcp, "caller").await;
        let next = first["queries"][0]["next"]["nextSectionPage"]["query"].clone();
        assert!(next.is_object(), "{first}");
        std::fs::write(home.path().join("second.txt"), "Changed source.").expect("change");
        let changed = section_call(&runtime, next, Channel::Mcp, "caller").await;
        assert!(changed.to_string().contains("sourceMismatch"), "{changed}");
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        runtime.close().await;
    }

    #[tokio::test]
    async fn sections_runtime_checks_paths_and_sanitizes_before_inference() {
        let home = tempfile::tempdir().expect("home");
        let outside = tempfile::NamedTempFile::new().expect("outside");
        let (runtime, server) = section_fixture(home.path(), "memory").await;
        let secret = format!("ghp_{}", "A".repeat(36));
        let file = home.path().join("secret.txt");
        std::fs::write(&file, format!("Restart with token {secret}.\n")).expect("source");
        let result = section_call(&runtime, json!({"queries":[{"id":"secure","operation":"sections","resources":{
            "secret":{"tool":"localFetch","query":{"path":file,"fullContent":true}},
            "outside":{"tool":"localFetch","query":{"path":outside.path(),"fullContent":true}}
        },"questions":{"check":{"type":"noul","instructions":"Does this section require a restart?"}}}]}), Channel::Mcp, "caller").await;
        assert_eq!(
            result["queries"][0]["resources"]["outside"]["status"], "error",
            "{result}"
        );
        assert_eq!(
            result["queries"][0]["resources"]["secret"]["sections"][0]["answers"]["check"]["status"],
            "answered",
            "{result}"
        );
        let sent = server.received_requests().await.unwrap();
        assert_eq!(sent.len(), 1);
        let body = String::from_utf8(sent[0].body.clone()).unwrap();
        assert!(!body.contains(&secret), "provider received secret");
        assert!(body.contains("REDACTED"), "{body}");
        assert!(!result.to_string().contains(&secret));
        let mut context = ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: std::time::Instant::now() - std::time::Duration::from_secs(1),
            walk_threads: None,
            response_window: None,
            github_credential: None,
        };
        assert_eq!(
            runtime
                .dispatcher()
                .capture_local_text(&json!({"path":file}), &context)
                .unwrap_err()
                .code,
            "timeout"
        );
        context.deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        context.cancellation.cancel();
        assert_eq!(
            runtime
                .dispatcher()
                .capture_local_text(&json!({"path":file}), &context)
                .unwrap_err()
                .code,
            "cancelled"
        );
        runtime.close().await;
    }

    /// Keys no setting reads (unknown or removed) are left to `octocode
    /// config`; an ignored value of a live setting prints at every start.
    #[test]
    fn startup_prints_only_ignored_values_of_live_settings() {
        let diagnostic = |code: &str| config::ConfigDiagnostic {
            severity: config::Severity::Warning,
            code: code.into(),
            field_path: None,
            message: code.into(),
            source_path: None,
        };
        let diagnostics = [
            diagnostic(config::UNKNOWN_CONFIG_CODE),
            diagnostic(config::REMOVED_SETTING_CODE),
            diagnostic("invalid_config"),
            diagnostic("config_load_error"),
        ];
        let printed: Vec<&str> = startup_diagnostics(&diagnostics)
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert_eq!(printed, ["invalid_config", "config_load_error"]);
    }

    /// Reads of small single files and stored provider pages admit as
    /// light; a directory, a large file, a missing path, lspSearch and a
    /// fresh provider call admit as walks.
    #[test]
    fn small_single_file_reads_and_stored_provider_pages_admit_as_light() {
        let home = tempfile::tempdir().expect("home");
        let root = home.path().canonicalize().expect("root");
        std::fs::write(root.join("small.rs"), "fn main() {}\n").expect("small");
        std::fs::write(
            root.join("large.txt"),
            vec![b'x'; usize::try_from(LIGHT_FILE_BYTES).unwrap() + 1],
        )
        .expect("large");
        std::fs::create_dir(root.join("src")).expect("dir");
        let runtime = runtime(&root, &[]);
        let cost = |id, rows: &[Value], replayed| runtime.request_cost(id, rows, replayed);
        let file = |path: &str| json!({"path": path});
        assert_eq!(
            cost(ToolId::LocalSearch, &[file("small.rs")], false),
            Cost::Light
        );
        assert_eq!(
            cost(ToolId::LocalFetch, &[file("small.rs"), file("src")], false),
            Cost::Walk
        );
        for row in [file("large.txt"), file("missing.rs"), json!({})] {
            assert_eq!(cost(ToolId::LocalFetch, &[row], false), Cost::Walk);
        }
        assert_eq!(
            cost(ToolId::LspSearch, &[file("small.rs")], false),
            Cost::Walk
        );
        // A judged row waits on the classification provider, so it walks
        // even on a small file.
        let judged = json!({"path": "small.rs", "clasifyContext": {"classifiers": ["Relevant?"]}});
        let continued = json!({"path": "small.rs", "selection": "token"});
        for id in [ToolId::LocalFetch, ToolId::LocalSearch] {
            for row in [&judged, &continued] {
                assert_eq!(
                    cost(id, std::slice::from_ref(row), false),
                    Cost::Walk,
                    "{id:?}"
                );
            }
            assert_eq!(
                cost(id, &[file("small.rs"), judged.clone()], false),
                Cost::Walk,
                "{id:?}"
            );
        }
        assert_eq!(cost(ToolId::GhSearchCode, &[json!({})], false), Cost::Walk);
        assert_eq!(cost(ToolId::GhSearchCode, &[json!({})], true), Cost::Light);
    }

    /// A runtime given a pool runs its servers there and leaves them to the
    /// pool's owner on close; one built alone owns and stops its own.
    #[tokio::test]
    async fn a_runtime_on_a_shared_pool_leaves_it_to_its_owner() {
        let home = tempfile::tempdir().expect("home");
        let pool = Arc::new(octocode_engine::lsp::pool::LspClientPool::default());
        let shared = ToolRuntime::from_host_with_lsp_pool(
            HostOptions {
                cwd: Some(home.path().to_path_buf()),
                env: Some(BTreeMap::from([(
                    "OCTOCODE_HOME".to_owned(),
                    home.path().to_string_lossy().into_owned(),
                )])),
                ..HostOptions::default()
            },
            Arc::clone(&pool),
        )
        .expect("runtime");
        assert!(Arc::ptr_eq(&shared.lsp_pool, &pool));
        assert!(!shared.owns_lsp_pool);
        shared.close().await;
        assert_eq!(Arc::strong_count(&pool), 2, "the pool outlives the close");
        let alone = runtime(home.path(), &[]);
        assert!(alone.owns_lsp_pool);
        assert!(!Arc::ptr_eq(&alone.lsp_pool, &pool));
    }

    /// The graph workspace and the language-server invocation directory
    /// come from the runtime's working directory, so a host serving several
    /// directories runs each like a CLI started there.
    #[test]
    fn the_graph_workspace_and_lsp_invocation_dir_follow_the_runtime_cwd() {
        let home = tempfile::tempdir().expect("home");
        let repo = home.path().join("repo");
        let cwd = repo.join("sub");
        std::fs::create_dir_all(repo.join(".git")).expect("git");
        std::fs::create_dir_all(&cwd).expect("sub");
        let mut input = runtime(home.path(), &[]).input.clone();
        input.cwd = cwd.clone();
        input
            .env
            .insert("WORKSPACE_ROOT".into(), repo.to_string_lossy().into_owned());
        let runtime = ToolRuntime::new(input).expect("runtime");
        assert_ne!(Some(cwd.clone()), std::env::current_dir().ok());
        assert_eq!(runtime.graph_workspace(None), repo);
        assert_eq!(runtime.graph_workspace(Some(&cwd)), cwd);
        let discovery = runtime.lsp_execution_config().discovery(None);
        assert_eq!(discovery.invocation_dir, Some(cwd));
    }

    /// Catalog `lspServers` resolve against the runtime's workspace
    /// (host cwd or configured workspaceRoot), not the process cwd.
    #[test]
    fn lsp_server_root_is_the_runtime_workspace_not_the_process_cwd() {
        let home = tempfile::tempdir().expect("home");
        let runtime = runtime(home.path(), &[]);
        assert_eq!(runtime.lsp_root(), home.path());
        assert_ne!(
            Some(runtime.lsp_root().to_path_buf()),
            std::env::current_dir().ok()
        );
    }

    #[test]
    fn availability_is_resolved_once_per_runtime() {
        let home = tempfile::tempdir().expect("home");
        let runtime = runtime(
            home.path(),
            &[
                ("OCTOCODE_ENABLE_LOCAL", "false"),
                ("DISABLE_TOOLS", "ghSearchRepo"),
            ],
        );
        let resolved = crate::execution::availability::ToolAvailability::resolve(
            runtime.config(),
            RuntimeSurface::Mcp,
        );
        for id in ToolId::ALL {
            assert_eq!(
                runtime.is_available(id.as_str()),
                resolved.is_available(id.as_str()),
                "{id}"
            );
        }
        assert!(!runtime.is_available("localSearch"));
        assert!(!runtime.is_available("noSuchTool"));
    }

    #[test]
    fn selection_presentation_requires_provider_and_an_available_goal_tool() {
        let home = tempfile::tempdir().expect("home");
        for (env, expected) in [
            (vec![], false),
            (vec![("OCTOCODE_CLASSIFICATION_API", "fixture-key")], true),
            (vec![("OCTOCODE_CLASSIFICATION_API", "fixture-key")], true),
            (
                vec![
                    ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                    ("DISABLE_TOOLS", "clasify"),
                ],
                true,
            ),
            (
                vec![
                    ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                    ("TOOLS_TO_RUN", "ghSearchHistory"),
                ],
                false,
            ),
            (
                vec![
                    ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                    ("TOOLS_TO_RUN", "ghSearchCode"),
                ],
                true,
            ),
            (
                vec![
                    ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                    ("TOOLS_TO_RUN", "clasify"),
                ],
                false,
            ),
            (
                vec![
                    ("OCTOCODE_CLASSIFICATION_API", "   "),
                    ("DISABLE_TOOLS", "clasify"),
                ],
                false,
            ),
        ] {
            let runtime = runtime(home.path(), &env);
            assert_eq!(runtime.presentation()["selection"], expected, "{env:?}");
        }
    }

    #[tokio::test]
    async fn hidden_public_clasify_keeps_goal_reads_available() {
        let home = tempfile::tempdir().expect("home");
        let runtime = runtime(
            home.path(),
            &[
                ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                ("TOOLS_TO_RUN", "ghSearchCode"),
            ],
        );
        assert!(!runtime.is_available("clasify"));
        assert_eq!(runtime.presentation()["selection"], true);
        assert!(!runtime.is_available("clasify"));
        assert!(runtime.dispatcher().selection_available);
    }

    #[tokio::test]
    async fn hidden_public_clasify_without_goal_surface_has_no_semantic_route() {
        let home = tempfile::tempdir().expect("home");
        let runtime = runtime(
            home.path(),
            &[
                ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                ("TOOLS_TO_RUN", "ghSearchHistory"),
            ],
        );
        assert_eq!(runtime.presentation()["selection"], false);
        assert!(!runtime.dispatcher().selection_available);
    }

    #[test]
    fn only_persistent_storage_keeps_a_registry_cache() {
        let home = tempfile::tempdir().expect("home");
        let memory = runtime(home.path(), &[("OCTOCODE_STORAGE_MODE", "memory")]);
        assert!(memory.artifact_cache.is_none());
        let persistent = runtime(home.path(), &[("OCTOCODE_STORAGE_MODE", "persistent")]);
        assert!(persistent.artifact_cache.is_some());
    }

    /// A file-sourced language-server setting reaches the engine exactly as
    /// its environment spelling does (`lsp.cacheDir`).
    #[test]
    fn file_sourced_lsp_settings_reach_the_engine() {
        let home = tempfile::tempdir().expect("home");
        let mut vars = BTreeMap::new();
        vars.insert(
            "OCTOCODE_HOME".to_owned(),
            home.path().to_string_lossy().into_owned(),
        );
        let runtime = ToolRuntime::new(crate::config::ConfigInput {
            env: vars,
            cwd: home.path().to_path_buf(),
            os_home: home.path().to_path_buf(),
            global_env: FileInput::Missing {
                path: home.path().join(".env"),
            },
            project_env: FileInput::Missing {
                path: home.path().join(".octocode/.env"),
            },
            config_file: FileInput::Read {
                path: home.path().join(".octocoderc"),
                text: r#"{"lsp":{"cacheDir":"/tmp/lsp-cache"}}"#.into(),
            },
            project_config_file: FileInput::Missing {
                path: home.path().join(".octocode/.octocoderc"),
            },
            runtime_surface: RuntimeSurface::Mcp,
        })
        .expect("runtime");
        let env = &runtime.lsp_execution_config().env;
        let name = config::field::LSP_CACHE_DIR.env_name().expect("bound");
        assert_eq!(env.get(name).map(String::as_str), Some("/tmp/lsp-cache"));
    }

    /// Local tools reach only the GitHub checkouts (clones, materialized
    /// trees) and URL captures under the cache home, never the rest of it:
    /// credentials and logs stay outside every root.
    #[test]
    fn cache_home_exposes_only_checkout_trees() {
        let workspace = tempfile::tempdir().expect("workspace");
        let os_home = tempfile::tempdir().expect("os home");
        let home = os_home.path().join(".octocode");
        let credentials = home.join("credentials.json");
        let log = home.join("logs").join("x.log");
        let clone = crate::tools::gh_clone_repo::cache::clones_root(&home).join("o/r/main/f.rs");
        let tree =
            crate::tools::gh_structure::materialize::materialize_root(&home).join("o/r/sha/f.rs");
        let capture = crate::tools::source_capture::url::capture_root(&home).join("sha/source.txt");
        for file in [&credentials, &log, &clone, &tree, &capture] {
            std::fs::create_dir_all(file.parent().expect("parent")).expect("dir");
            std::fs::write(file, "x").expect("file");
        }
        let mut vars = BTreeMap::new();
        vars.insert(
            "OCTOCODE_HOME".to_owned(),
            home.to_string_lossy().into_owned(),
        );
        let runtime = ToolRuntime::new(crate::config::ConfigInput {
            env: vars,
            cwd: workspace.path().to_path_buf(),
            os_home: os_home.path().to_path_buf(),
            global_env: FileInput::Missing {
                path: os_home.path().join(".env"),
            },
            project_env: FileInput::Missing {
                path: workspace.path().join(".octocode/.env"),
            },
            config_file: FileInput::Missing {
                path: os_home.path().join(".octocoderc"),
            },
            project_config_file: FileInput::Missing {
                path: workspace.path().join(".octocode/.octocoderc"),
            },
            runtime_surface: RuntimeSurface::Mcp,
        })
        .expect("runtime");
        for denied in [&credentials, &log, &home] {
            assert!(
                runtime.paths.validate(denied).is_err(),
                "{}",
                denied.display()
            );
        }
        for allowed in [&clone, &tree, &capture] {
            assert!(
                runtime.paths.validate(allowed).is_ok(),
                "{}",
                allowed.display()
            );
        }
    }
}
