mod admission;
mod channel;
mod construction;
mod cursors;
mod execute;
mod graph;
mod page_replay;
mod provider_check;

use crate::config::{self, ConfigInput, ConfigOutput, RuntimeSurface};
use crate::contracts;
use crate::execution::cancel::CancellationCheck;
use crate::execution::{ExecutionContext, ExecutionError, RequestRuntime, RuntimeLimits};
use crate::id::ToolId;
use crate::policy::path::PathPolicy;
use crate::response::exit::ExitClass;
use crate::security::ContentSecurity;
use octocode_contract_types::ErrorCode;
use page_replay::PageReplayMemo;

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
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Box<Value>>,
    #[serde(skip)]
    pub validation_issues: Option<Vec<contracts::ValidationIssue>>,
}
impl RuntimeError {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            payload: None,
            validation_issues: None,
        }
    }

    #[must_use]
    pub fn exit_class(&self) -> ExitClass {
        ExitClass::of(self.code)
    }
}

pub struct ToolRuntime {
    pub requests: RequestRuntime,
    input: ConfigInput,
    config: Arc<ConfigOutput>,
    analysis_capabilities: octocode_contract_types::AnalysisCapabilities,
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
    cursors: Arc<cursors::Registry>,
    section_sessions: Arc<crate::tools::clasify::sections::Sessions>,
    /// Registry answers for artifactSearch; `None` under memory storage.
    artifact_cache: Option<Arc<crate::providers::artifact::ArtifactCache>>,
    /// Resolved from config, surface, and the startup provider ping. Later
    /// provider failures are request-local and do not remove an advertised tool.
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

/// Largest file a [`Cost::Light`] local row reads.
const LIGHT_FILE_BYTES: u64 = 1024 * 1024;

/// Who makes one call. Both read the same JSON envelope; they differ in
/// admission (MCP never runs a `cliOnly` tool) and in the saved-page store
/// their continuations name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    /// The `octocode` CLI (JSON or its views, printed from the same JSON).
    Cli,
    /// An MCP CallToolResult.
    Mcp,
}

impl ToolRuntime {
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
        self.cursors.clear_memory();
        self.section_sessions.clear_memory();
    }
    pub fn inspect_config(&self) -> config::ConfigInspectorData {
        config::inspector_data(&self.input, &self.config)
    }

    /// Resolved presentation switches shared by MCP and CLI discovery.
    fn presentation(&self) -> Value {
        json!({"selection": self.selection_available})
    }
}

impl ToolRuntime {
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

    /// Start the language servers of the runtime's workspace in the
    /// background, for a host that serves calls from now on (`octocode mcp`
    /// after `initialize`): none when `lsp.prewarm` is `off` or `lspSearch`
    /// is unavailable; the workspace's main language under `targeted`, its
    /// top languages under `all` (each start is a whole-project index). See
    /// [`crate::tools::lsp_search::prewarm::workspace::schedule_workspace`].
    pub fn prewarm_lsp_workspace(&self, handle: &tokio::runtime::Handle) {
        let starts = match self.config.resolved.lsp.prewarm {
            crate::config::LspPrewarm::Off => return,
            crate::config::LspPrewarm::Targeted => 1,
            crate::config::LspPrewarm::All => crate::tools::lsp_search::prewarm::MAX_PREWARM_KEYS,
        };
        if !self.is_available(ToolId::LspSearch.as_str()) {
            return;
        }
        crate::tools::lsp_search::prewarm::workspace::schedule_workspace(
            handle,
            &self.lsp_pool,
            &self.paths,
            &self.lsp_execution_config,
            &self.workspace_root,
            starts,
        );
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
    /// the enforcement contract fingerprint. Native CLI and MCP surfaces render
    /// the core-generated presentation and instructions.
    pub fn catalog(&self) -> Value {
        let analysis = &self.analysis_capabilities;
        // Generated constants only: the catalog never parses the contract.
        let tools = ToolId::ALL
            .into_iter()
            .map(|id| {
                let name = id.as_str();
                let mut entry = json!({
                    "name": name,
                    "title": id.title(),
                    "available": self.is_available(name),
                });
                if let Some(reason) = self
                    .availability(name)
                    .and_then(super::super::execution::availability::Availability::reason_code)
                {
                    entry["unavailableReason"] = json!(reason);
                }
                entry
            })
            .collect::<Vec<_>>();
        json!({
            "fingerprint": octocode_contract_types::TOOL_TYPES_CONTRACT_FINGERPRINT,
            "grammarCapabilities": analysis.grammar_capabilities,
            "sourceDecoding": analysis.source_decoding,
            // Languages whose lspSearch server resolves here (resolution only);
            // hosts render it into the lspSearch description.
            "lspServers": self.lsp_server_languages(),
            "presentation": self.presentation(),
            "tools": tools,
        })
    }
}

fn runtime_execution_error(error: ExecutionError) -> RuntimeError {
    RuntimeError::new(error.code(), error.to_string())
}

impl CancellationCheck for ExecutionContext {
    fn check(&self) -> Result<(), String> {
        Self::check(self).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod output_recovery_tests;

#[cfg(test)]
mod security_matrix;

#[cfg(test)]
mod construction_tests;
