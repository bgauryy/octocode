mod admission;
mod cache_control;
mod channel;
mod construction;
mod cursors;
mod execute;
mod graph;
mod page_replay;

use octocode_contract_types::ErrorCode;
use octocode_runtime_base::config::{self, ConfigInput, ConfigOutput, RuntimeSurface};
use octocode_runtime_base::contracts;
use octocode_runtime_base::execution::{ExecutionError, RequestRuntime, RuntimeLimits};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::exit::ExitClass;
use octocode_runtime_base::security::ContentSecurity;
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
    /// The caller's per-request budget in seconds; a call past it answers an
    /// honest partial. Default: none, as on every surface (CLI, warm server,
    /// MCP): no deadline, and each layer's own limit and a cancel end the
    /// work ([`octocode_runtime_base::execution::RequestLimits::timeout`]).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// The config files this process already read for this directory, so a
    /// start does not read them again; `env`, `cwd` and `surface` still decide.
    #[serde(skip)]
    pub loaded: Option<config::ConfigInput>,
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
    github_services: Arc<
        std::sync::OnceLock<Result<super::github::GitHubServices, octocode_github::ProviderError>>,
    >,
    /// Provider answers kept across calls and, when persistent, processes.
    provider_answers: Arc<octocode_runtime_base::providers::saved_answers::KeptAnswers>,
    lsp_pool: Arc<octocode_engine::lsp::pool::LspClientPool>,
    /// Whether this runtime created [`Self::lsp_pool`]: only its creator
    /// stops the servers on close; a runtime given a shared pool leaves them
    /// to the pool's owner and to its other runtimes.
    owns_lsp_pool: bool,
    /// Pre-paging envelopes of recent paged responses (this runtime's config
    /// and workspace only), so a later page skips re-executing the batch.
    page_replays: Arc<PageReplayMemo>,
    cursors: Arc<cursors::Registry>,
    section_sessions: Arc<octocode_tool_clasify::clasify::sections::Sessions>,
    /// Resolved from config and surface without provider requests. Provider
    /// failures are request-local and do not remove an advertised tool.
    availability: Arc<octocode_runtime_base::execution::availability::ToolAvailability>,
    /// Provider settings and selection availability, resolved once.
    evaluator: octocode_tools_shared::judgment::Evaluator,
    /// Candidate pages selection judged, kept in memory so a replayed cursor
    /// bills no second judgment.
    selection_pages: Arc<octocode_tools_shared::selection::SavedPlans>,
    selection_available: bool,
    lsp_execution_config: Arc<octocode_tool_lsp::lsp_search::LspExecutionConfig>,
    lsp_answers: Arc<octocode_tool_lsp::lsp_search::ContinuationAnswers>,
    /// Complete graph answer rows its first page retained for continuations.
    graph_rows: Arc<octocode_tool_graph::ast_graph::store::SavedRows>,
    /// The sandbox workspace root (configured workspaceRoot, else host cwd).
    workspace_root: PathBuf,
}

/// Who makes one call. Both read the same JSON envelope; they differ in
/// admission (MCP runs only the tools its availability settings enable) and in the saved-page store
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
        // A rate-limit fact a finished request wrote without awaiting must
        // reach the next process: the CLI and the MCP server end right after
        // this close.
        octocode_github::GitHubBudget::flush_detached_writes().await;
        if self.owns_lsp_pool {
            self.lsp_pool.clear_all().await;
        }
        self.page_replays.store.clear_memory();
        self.cursors.clear_memory();
        self.section_sessions.clear_memory();
        self.lsp_answers.clear_memory();
        self.graph_rows.clear_memory();
        self.selection_pages.clear_memory();
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
    ) -> Option<&octocode_runtime_base::execution::availability::Availability> {
        self.availability.get(tool)
    }

    /// The resolved language-server settings `lspSearch` discovers with.
    pub fn lsp_execution_config(&self) -> &octocode_tool_lsp::lsp_search::LspExecutionConfig {
        &self.lsp_execution_config
    }

    /// Start the language servers of the runtime's workspace in the
    /// background, for a host that serves calls from now on (`octocode mcp`
    /// after `initialize`): only under `lsp.prewarm` `all`, its top languages,
    /// and only while `lspSearch` is available. Each start is a whole-project
    /// index, up to gigabytes for a session that may never call `lspSearch`,
    /// so `targeted` waits for a response that serves an lspSearch call.
    /// See [`octocode_tool_lsp::lsp_search::prewarm::workspace::schedule_workspace`].
    pub fn prewarm_lsp_workspace(&self, handle: &tokio::runtime::Handle) {
        let starts = match self.config.resolved.lsp.prewarm {
            octocode_runtime_base::config::LspPrewarm::Off
            | octocode_runtime_base::config::LspPrewarm::Targeted => return,
            octocode_runtime_base::config::LspPrewarm::All => {
                octocode_tool_lsp::lsp_search::prewarm::MAX_PREWARM_KEYS
            }
        };
        if !self.is_available(ToolId::LspSearch.as_str()) {
            return;
        }
        octocode_tool_lsp::lsp_search::prewarm::workspace::schedule_workspace(
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
        octocode_engine::lsp::config::available_server_languages(
            &self.lsp_root().to_string_lossy(),
            &self.lsp_execution_config.discovery(),
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
                if let Some(reason) = self.availability(name).and_then(
                    octocode_runtime_base::execution::availability::Availability::reason_code,
                ) {
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
    let mut served = RuntimeError::new(error.code(), error.to_string());
    // The cause is the environment: its recovery replaces the generic
    // "report a tool failure".
    if matches!(error, ExecutionError::StorageUnavailable(_)) {
        let mut envelope = contracts::tool_error(&served.message, Vec::new());
        octocode_runtime_base::response::result::set_hints(
            &mut envelope,
            [octocode_runtime_base::response::rows::WRITE_ACCESS_HINT],
        );
        served.payload = Some(Box::new(envelope));
    }
    served
}

#[cfg(test)]
mod output_recovery_tests;

#[cfg(test)]
mod security_matrix;

#[cfg(test)]
mod construction_tests;

#[cfg(test)]
mod exit_class_tests;
