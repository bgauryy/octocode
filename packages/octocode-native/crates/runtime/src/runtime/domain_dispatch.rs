//! One domain execution path for public tools and Jev's bounded read context.
mod finalize;
mod lsp_row;
mod panic_site;
mod registry_row;
mod source_reader;
use super::{dispatch, github};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::tools::lsp_search::lsp_search_query;
use crate::{
    config::ConfigOutput,
    id::{ToolFamily, ToolId},
    policy::path::PathPolicy,
    response::outcome::{DomainResult, Upstream},
    security::{CheckedQuery, ContentSecurity},
    tools::lsp_search::{LspExecutionConfig, prewarm},
};
use octocode_contract_types::{ErrorCode, LspSearchQuery};
use octocode_github::ProviderError;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

pub(crate) struct DomainDispatcher {
    pub paths: Arc<PathPolicy>,
    /// [`Self::paths`] with credential file names withheld whatever the setting.
    pub egress_paths: Arc<PathPolicy>,
    pub security: Arc<ContentSecurity>,
    pub(super) github_services: Arc<OnceLock<Result<github::GitHubServices, ProviderError>>>,
    pub(super) provider_answers: Arc<crate::providers::saved_answers::KeptAnswers>,
    pub(super) github_access: github::ResponseAccess,
    pub config: Arc<ConfigOutput>,
    /// Internal limits: the runtime's code constants.
    pub limits: crate::execution::RuntimeLimits,
    pub home: PathBuf,
    pub handle: tokio::runtime::Handle,
    pub lsp_pool: Arc<octocode_engine::lsp::pool::LspClientPool>,
    pub lsp_execution_config: Arc<LspExecutionConfig>,
    pub lsp_answers: Arc<crate::tools::lsp_search::ContinuationAnswers>,
    /// An lspSearch ran in this session: the agent uses language servers, so
    /// `targeted` prewarm may start the ones later results point at.
    pub lsp_requested: Arc<std::sync::atomic::AtomicBool>,
    pub graph_rows: Arc<crate::tools::ast_graph::store::SavedRows>,
    pub availability: Arc<crate::execution::availability::ToolAvailability>,
    /// Provider readiness and an enabled goals surface, independent of the
    /// public clasify tool's availability.
    pub(crate) selection_available: bool,
    pub(crate) evaluator: crate::tools::judgment::Evaluator,
}

#[cfg(test)]
use source_reader::local_capture_failure;

#[cfg(test)]
mod local_capture_tests;

impl DomainDispatcher {
    /// This runtime's GitHub services, built on first use and shared by
    /// every row, capture and identity check after it.
    fn github_services(&self) -> Result<&github::GitHubServices, &ProviderError> {
        self.github_services
            .get_or_init(|| {
                github::GitHubServices::new(
                    self.config.clone(),
                    &self.limits,
                    self.home.clone(),
                    self.provider_answers.clone(),
                )
            })
            .as_ref()
    }

    pub(super) fn github_response_identity(
        &self,
        context: &ExecutionContext,
    ) -> Result<String, ProviderError> {
        self.github_services()
            .map_err(Clone::clone)?
            .response_identity(context, &self.handle, &self.github_access)
    }

    pub(super) fn selection_runtime(
        &self,
        context: &ExecutionContext,
    ) -> crate::tools::selection::Runtime {
        crate::tools::selection::Runtime {
            limits: self.limits.selection,
            available: self.selection_available,
            judge: Arc::new(self.evaluator.clone()),
            handle: self.handle.clone(),
            context: context.clone(),
        }
    }

    /// The policy a local row reads under. A graph writes what it reads to disk
    /// and a classifier sends it to a provider, so neither reads credential
    /// files; nor does a row a source reader captures for one (`egress`).
    fn local_paths(&self, tool: ToolId, query: &Value, egress: bool) -> &PathPolicy {
        if matches!(tool, ToolId::GraphBuild | ToolId::GraphQuery)
            || goes_to_provider(query, egress)
        {
            &self.egress_paths
        } else {
            &self.paths
        }
    }

    /// Every route matches the contract family exhaustively; within a
    /// family only the tools with their own runner are named. The runtime
    /// resolves the wire name once.
    pub(crate) fn execute(
        &self,
        tool: ToolId,
        query: &CheckedQuery,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        self.execute_row_under(tool, query, context, false)
    }

    /// [`Self::execute`]; `egress` when the row's text leaves the local read.
    fn execute_row_under(
        &self,
        tool: ToolId,
        query: &CheckedQuery,
        context: &ExecutionContext,
        egress: bool,
    ) -> Result<DomainResult, ExecutionError> {
        let _enter = self.handle.enter();
        context.check()?;
        let mut result = execute_row(|| match tool.family() {
            ToolFamily::GitHub => self.execute_github(tool, query, context, egress),
            ToolFamily::Local if tool == ToolId::LspSearch => {
                self.execute_lsp(query, context, egress)
            }
            ToolFamily::Local => self.execute_local(tool, query, context, egress),
            // Clasify runs its own batch, never a domain row.
            ToolFamily::Remote if tool.is_clasify() => Err(ExecutionError::UnroutedTool),
            ToolFamily::Remote if tool == ToolId::SecurityCheck => {
                self.execute_security_check(query, context)
            }
            ToolFamily::Remote => self.execute_artifact(query, context),
        })?;
        if matches!(tool.family(), ToolFamily::Local) {
            crate::response::paths::workspace_relative(&mut result.data, &self.paths);
        }
        Ok(result)
    }

    fn execute_github(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
        egress: bool,
    ) -> Result<DomainResult, ExecutionError> {
        if tool == ToolId::GhGetFileContent
            && goes_to_provider(query, egress)
            && let Some(withheld) = source_reader::withheld_github_read(query)
        {
            return Ok(withheld);
        }
        let selection = self.selection_runtime(context);
        match self.github_services() {
            Ok(services) => services.execute_query(
                tool,
                query,
                github::QueryExecution {
                    context,
                    handle: &self.handle,
                    access: &self.github_access,
                },
                &self.security,
                &self.paths,
                &selection,
            ),
            Err(error) => Ok(github::provider_error(error.clone())),
        }
    }

    fn execute_local(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
        egress: bool,
    ) -> Result<DomainResult, ExecutionError> {
        let context = ExecutionContext {
            response_window: local_response_window(tool, context, self.limits.auto_page_chars),
            ..context.clone()
        };
        let selection = self.selection_runtime(&context);
        // `lsp.prewarm`: `targeted` warms the server an offered lspSearch
        // call names once this session has run an lspSearch; `all` warms it
        // from the start, and also the file a read or search names.
        let prewarm_mode = self.config.resolved.lsp.prewarm;
        let prewarm_query = (prewarm_mode == crate::config::LspPrewarm::All).then_some(query);
        let lsp = &self.lsp_execution_config;
        // The row already runs on a blocking or scoped worker thread inside
        // the runtime context ([`Self::execute`]): no further thread hop.
        // lspSearch leads offered by local tools probe the runtime's resolved
        // server settings, not the bare process env.
        let result = crate::tools::leads::lsp::with_lead_discovery(lsp.discovery(), || {
            dispatch::execute_local(
                tool,
                query,
                &context,
                &dispatch::LocalEnv {
                    memory_only: !crate::config::is_persistent_storage_enabled(
                        &self.config.resolved,
                    ),
                    cargo: self
                        .config
                        .env_value(crate::config::environment::OCTOCODE_CARGO),
                    paths: self.local_paths(tool, query, egress),
                    security: &self.security,
                    selection: Some(&selection),
                    graph_rows: Some(&self.graph_rows),
                },
            )
        })?;
        // Start a language server in the background so a following
        // lspSearch finds a warm pooled client: for the file an offered
        // lspSearch call names, or (opt-in) the file a read or search names.
        // A lead alone is no reason to index a whole project.
        let warms_leads = match prewarm_mode {
            crate::config::LspPrewarm::Off => false,
            crate::config::LspPrewarm::Targeted => self
                .lsp_requested
                .load(std::sync::atomic::Ordering::Relaxed),
            crate::config::LspPrewarm::All => true,
        };
        let lead = warms_leads
            .then(|| prewarm::lead_file(tool, &result.data))
            .flatten();
        if let Some(file) = lead
            .or_else(|| prewarm_query.and_then(|query| prewarm::anchor_file(query, &result.data)))
        {
            prewarm::schedule(
                &self.handle,
                &self.lsp_pool,
                &self.paths,
                &self.lsp_execution_config,
                &file,
            );
        }
        Ok(result)
    }
}

/// Whether a row's text goes to a classification provider: a source reader
/// captures it for one (`egress`), or the row judges with one.
fn goes_to_provider(query: &Value, egress: bool) -> bool {
    egress
        || ["classification", "selection"]
            .into_iter()
            .any(|field| query.get(field).is_some())
}

/// Runs language-server work for the request `context` admits. A starved
/// server may use the time the request still has: its waits extend while it
/// progresses, up to the request deadline. Cancelling the request (the
/// caller's cancel, or the runtime at the deadline) drops `work` at once,
/// which cancels its pending server requests, instead of holding the worker
/// until the wait ends.
async fn within_request<F: Future>(
    context: &ExecutionContext,
    work: F,
) -> Result<F::Output, ExecutionError> {
    let deadline = tokio::time::Instant::from_std(context.deadline);
    tokio::select! {
        biased;
        () = context.cancellation.cancelled() => {
            Err(context.check().err().unwrap_or(ExecutionError::Cancelled))
        }
        output = octocode_engine::lsp::with_caller_deadline(deadline, work) => Ok(output),
    }
}

#[cfg(test)]
mod within_request_tests;

/// The one page budget of a local row, whichever local tool it is: the
/// window its own pages fill (`None` when the runtime has no window).
/// Streamed listing, walk and lspSearch pages are cut at the automatic
/// response window, which every call of a walk shares (an explicit
/// `responseChars` is per call, so it never moves a page cut and a
/// continuation under another window names the same page). localFetch and
/// graphQuery continue by offset: a read or graph page fits the row's share
/// of the response page, the caller's or the automatic
/// ([`ExecutionContext::row_page_chars`], see `response::read_share`), and
/// each row continues by its own `next` offset.
fn local_response_window(
    tool: ToolId,
    context: &ExecutionContext,
    auto_page_chars: usize,
) -> Option<usize> {
    let window = if tool.pages_by_row_window() {
        context.row_page_chars(auto_page_chars)
    } else {
        auto_page_chars
    };
    (window > 0).then_some(window)
}

#[cfg(test)]
mod page_chars_tests;

/// A failed engine operation becomes one row; sibling rows keep executing.
/// Worker-thread and FFI boundaries retain their own unwind containment.
/// A panic is a defect: its failure names the source location to report.
fn execute_row(
    work: impl FnOnce() -> Result<DomainResult, ExecutionError>,
) -> Result<DomainResult, ExecutionError> {
    panic_site::contain(work).unwrap_or_else(|site| {
        let at = site.map(|site| format!(" at {site}")).unwrap_or_default();
        Ok(DomainResult::failure(
            ErrorCode::ExecutionFailed,
            format!("The tool row panicked{at}; this is a tool defect."),
            Vec::new(),
            None,
        ))
    })
}

#[cfg(test)]
mod row_tests;
