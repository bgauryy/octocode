use super::page_replay::PageReplayMemo;
use super::{HostOptions, RuntimeError, ToolRuntime, runtime_execution_error};
use crate::config::{self, ConfigInput, ConfigOutput};
use crate::execution::{RequestLimits, RequestRuntime, RuntimeLimits};
use crate::id::ToolId;
use crate::policy::path::{PathPolicy, PathPolicyConfig};
use crate::security::ContentSecurity;
use octocode_contract_types::ErrorCode;
use secrecy::SecretString;
use std::path::PathBuf;
use std::sync::Arc;

/// The configuration diagnostics every runtime start prints. A key no
/// setting reads changes nothing, so
/// `octocode config` reports it instead of every call; an ignored value of a
/// live setting (invalid, protected, an unreadable file) is said each time.
pub(super) fn startup_diagnostics(
    diagnostics: &[config::ConfigDiagnostic],
) -> impl Iterator<Item = &config::ConfigDiagnostic> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code != config::UNKNOWN_CONFIG_CODE)
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
                    ErrorCode::InvalidInput,
                    format!("timeoutSecs must be between 1 and {max}; got {secs}"),
                ));
            }
        }
        let cwd = options
            .cwd
            .or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| {
                RuntimeError::new(ErrorCode::Config, "Cannot resolve working directory")
            })?;
        let home = std::env::home_dir()
            .ok_or_else(|| RuntimeError::new(ErrorCode::Config, "Cannot resolve home directory"))?;
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
            super::super::maintenance::run_if_due(&runtime.config.home, runtime.limits.clone.ttl);
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
        .map_err(|error| RuntimeError::new(ErrorCode::Policy, error.message))?;
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
        let github_cache = super::super::github_cache::GitHubContentCache::new(
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
        let cursors = Arc::new(super::cursors::Registry::new(
            PageReplayMemo::cache_config(),
            persistent.then(|| config.home.join("tmp").join("continuations")),
            page_replays.policy.clone(),
        ));
        let section_sessions = Arc::new(crate::tools::clasify::sections::Sessions::new(
            PageReplayMemo::cache_config(),
            persistent.then(|| config.home.join("tmp").join("section-assessments")),
            page_replays.policy.clone(),
        ));
        // Notices reach stderr and the warm-server reply outside any
        // envelope: they pass the same secret scrub as error text.
        octocode_io::notice::set_scrubber(crate::security::scrub_error_text);
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
            cursors,
            section_sessions,
            artifact_cache,
            lsp_execution_config,
            workspace_root,
        })
    }
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

/// A visible read tool may use the provider through its opt-in `classification`
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
                    && crate::contracts::query_schema_value(id, None, "classification", "type")
                        .is_some()
            })
}
