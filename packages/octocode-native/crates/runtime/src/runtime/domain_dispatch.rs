//! One domain execution path for public tools and Jev's bounded read context.
mod lsp_row;
mod registry_row;
mod source_reader;
use super::{dispatch, github};
use octocode_contract_types::{ErrorCode, LspSearchQuery};
use octocode_engine::contain::Phase;
use octocode_github::ProviderError;
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::{
    config::ConfigOutput,
    id::{ToolFamily, ToolId},
    policy::path::PathPolicy,
    response::outcome::{DomainResult, Upstream},
    security::{CheckedQuery, ContentSecurity},
};
use octocode_tool_lsp::lsp_search::lsp_search_query;
use octocode_tool_lsp::lsp_search::{LspExecutionConfig, prewarm};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

/// What the rows of one batch share ([`DomainDispatcher::plan_warm`]): the
/// GitHub warm-up of what they name, and a package tool's call share.
pub(crate) struct RowsShare {
    github: Option<github::BatchWarm>,
    packages: Option<octocode_tool_packages::CallShare>,
}

impl RowsShare {
    /// The GitHub warm-up's memo, for tests that watch what it leaves.
    #[cfg(test)]
    pub(crate) fn memo(&self) -> &Arc<octocode_github::RefMemo> {
        match &self.github {
            Some(github) => github.memo(),
            None => panic!("these rows share no GitHub warm-up"),
        }
    }
}

pub(crate) struct DomainDispatcher {
    pub paths: Arc<PathPolicy>,
    /// [`Self::paths`] with credential file names withheld whatever the setting.
    pub egress_paths: Arc<PathPolicy>,
    pub security: Arc<ContentSecurity>,
    pub(super) github_services: Arc<OnceLock<Result<github::GitHubServices, ProviderError>>>,
    pub(super) provider_answers: Arc<octocode_runtime_base::providers::saved_answers::KeptAnswers>,
    pub(super) github_access: github::ResponseAccess,
    pub config: Arc<ConfigOutput>,
    /// Internal limits: the runtime's code constants.
    pub limits: octocode_runtime_base::execution::RuntimeLimits,
    pub home: PathBuf,
    pub handle: tokio::runtime::Handle,
    pub lsp_pool: Arc<octocode_engine::lsp::pool::LspClientPool>,
    pub lsp_execution_config: Arc<LspExecutionConfig>,
    pub lsp_answers: Arc<octocode_tool_lsp::lsp_search::ContinuationAnswers>,
    /// What a row's waits may take when the caller set no budget: the
    /// channel's default (`RuntimeLimits::mcp_reply_budget` on MCP, none on
    /// the CLI). It bounds an lspSearch row's wait for its server here; the
    /// call's GitHub waits are bounded by the same budget from the call's
    /// start ([`github::ResponseAccess::bound_waits`]).
    pub reply_budget: Option<std::time::Duration>,
    pub graph_rows: Arc<octocode_tool_graph::ast_graph::store::SavedRows>,
    pub availability: Arc<octocode_runtime_base::execution::availability::ToolAvailability>,
    /// A provider key is configured and clasify is available.
    pub(crate) clasify_available: bool,
    pub(crate) evaluator: octocode_tools_shared::judgment::Evaluator,
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

    /// The policy a local row reads under. A graph writes what it reads to disk
    /// and a source reader captures a row for a classification provider
    /// (`egress`), so neither reads credential files.
    fn local_paths(&self, tool: ToolId, egress: bool) -> &PathPolicy {
        if matches!(tool, ToolId::GraphBuild | ToolId::GraphQuery) || egress {
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
        warm: Option<&RowsShare>,
    ) -> Result<DomainResult, ExecutionError> {
        self.execute_row_under(tool, query, context, false, warm)
    }

    /// What the rows of one batch share, made before they run, or `None` when
    /// they share nothing: GitHub reads to warm, and for a package tool's
    /// rows the call share that makes each upstream read once
    /// ([`octocode_tool_packages::CallShare`]). Run [`Self::run_warm`] beside
    /// the rows; they wait for it only where they read what it warms.
    pub(crate) fn plan_warm(&self, tool: ToolId, queries: &[CheckedQuery]) -> Option<RowsShare> {
        let github = github::BatchWarm::plan(tool, queries, &self.paths);
        let packages = (matches!(tool, ToolId::ArtifactSearch | ToolId::SecurityCheck)
            && queries.len() > 1)
            .then(octocode_tool_packages::CallShare::new);
        (github.is_some() || packages.is_some()).then_some(RowsShare { github, packages })
    }

    /// The GitHub warm-up of [`Self::plan_warm`], to completion; nothing when
    /// the rows share no GitHub read.
    pub(crate) fn run_warm(&self, warm: &RowsShare, context: &ExecutionContext) {
        let Some(warm) = &warm.github else {
            return;
        };
        let _enter = self.handle.enter();
        let _settle = warm.settling();
        let Ok(services) = self.github_services() else {
            return;
        };
        // The warm-up only saves requests: a panic in it leaves each row to
        // ask for itself (the guard above releases any row waiting on it).
        if let Err(panic) = octocode_engine::contain::catch(Phase::Warm, || {
            services.warm(warm, context, &self.handle, &self.github_access);
        }) {
            octocode_io::notice::raise(
                "gitHubWarmUp",
                format!(
                    "The batch's GitHub warm-up failed at {}; each row asked for itself.",
                    panic.site.as_deref().unwrap_or("an unknown site")
                ),
            );
        }
    }

    /// [`Self::execute`]; `egress` when the row's text leaves the local read.
    fn execute_row_under(
        &self,
        tool: ToolId,
        query: &CheckedQuery,
        context: &ExecutionContext,
        egress: bool,
        warm: Option<&RowsShare>,
    ) -> Result<DomainResult, ExecutionError> {
        let _enter = self.handle.enter();
        context.check()?;
        let packages = warm.and_then(|warm| warm.packages.as_ref());
        let warm = warm.and_then(|warm| warm.github.as_ref());
        let mut result = execute_row(|| match tool.family() {
            ToolFamily::GitHub => self.execute_github(tool, query, context, egress, warm),
            ToolFamily::Local if tool == ToolId::LspSearch => {
                self.execute_lsp(query, context, egress)
            }
            ToolFamily::Local => self.execute_local(tool, query, context, egress),
            // Clasify runs its own batch, never a domain row.
            ToolFamily::Remote if tool.is_clasify() => Err(ExecutionError::UnroutedTool),
            ToolFamily::Remote if tool == ToolId::SecurityCheck => {
                self.execute_security_check(query, context, packages)
            }
            ToolFamily::Remote => self.execute_artifact(query, context, packages),
        })?;
        if matches!(tool.family(), ToolFamily::Local) {
            // A graph names its own rows from its snapshot root.
            let graph = matches!(tool, ToolId::GraphBuild | ToolId::GraphQuery);
            // Found from the row's absolute paths, before they are shown.
            let commit = (!graph)
                .then(|| {
                    octocode_runtime_base::response::checkout::checkout_commit(
                        &result.data,
                        &self.paths,
                        query,
                    )
                })
                .flatten();
            octocode_runtime_base::response::paths::shown_paths(
                graph,
                &mut result.data,
                &self.paths,
                query,
            );
            if let (Some(commit), Some(fields)) = (commit, result.data.as_object_mut()) {
                fields
                    .entry("commitSha")
                    .or_insert(serde_json::Value::String(commit));
            }
        }
        Ok(result)
    }

    fn execute_github(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
        egress: bool,
        warm: Option<&github::BatchWarm>,
    ) -> Result<DomainResult, ExecutionError> {
        if tool == ToolId::GhGetFileContent
            && egress
            && let Some(withheld) = source_reader::withheld_github_read(query, &self.paths)
        {
            return Ok(withheld);
        }
        match self.github_services() {
            Ok(services) => services.execute_query(
                tool,
                query,
                github::QueryExecution {
                    context,
                    handle: &self.handle,
                    access: &self.github_access,
                    warm,
                },
                &self.security,
                &self.paths,
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
        // `lsp.prewarm` warms the file a row names, as an editor starts a
        // server when a file of its language opens: `all` for every local
        // tool, `targeted` for a read or search (localSearch, localFetch,
        // astSearch). Both also warm the file of the lspSearch call the
        // settled response serves ([`Self::warm_served_leads`]).
        let prewarm_query =
            warms_named_file(self.config.resolved.lsp.prewarm, tool).then_some(query);
        // The row already runs on a blocking or scoped worker thread inside
        // the runtime context ([`Self::execute`]): no further thread hop.
        let discovery = self.lsp_execution_config.discovery();
        let result = dispatch::execute_local(
            tool,
            query,
            &context,
            &dispatch::LocalEnv {
                memory_only: !octocode_runtime_base::config::is_persistent_storage_enabled(
                    &self.config.resolved,
                ),
                graph_home: self
                    .config
                    .resolved
                    .local
                    .graph_home
                    .as_deref()
                    .map(std::path::Path::new),
                lsp: Some(&discovery),
                cargo: self
                    .config
                    .env_value(octocode_runtime_base::config::environment::OCTOCODE_CARGO),
                paths: self.local_paths(tool, egress),
                security: &self.security,
                clasify: self.clasify_available,
                graph_rows: Some(&self.graph_rows),
            },
        )?;
        // Start the server of the file the row names in the background, one
        // file per row; a file with no configured server starts none. The
        // pool bounds it: at most three start at once.
        if let Some(file) =
            prewarm_query.and_then(|query| prewarm::anchor_file(&self.paths, query, &result.data))
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

impl DomainDispatcher {
    /// `lsp.prewarm` `targeted` or `all`: start, in the background, the
    /// language server of the file the first lspSearch call served by a
    /// local-family `structured` response names, so the agent's lspSearch
    /// finds it warm. Runs on the settled response only: a lead the stage
    /// dropped starts no server, and a page replay never reaches it.
    pub(crate) fn warm_served_leads(&self, tool: ToolId, structured: &Value) {
        if self.config.resolved.lsp.prewarm == octocode_runtime_base::config::LspPrewarm::Off
            || tool.family() != ToolFamily::Local
            || tool == ToolId::LspSearch
        {
            return;
        }
        if let Some(file) = prewarm::lead_file(&self.paths, tool, structured) {
            prewarm::schedule(
                &self.handle,
                &self.lsp_pool,
                &self.paths,
                &self.lsp_execution_config,
                &file,
            );
        }
    }
}

/// Whether `prewarm` starts the server of the file a `tool` row names.
fn warms_named_file(prewarm: octocode_runtime_base::config::LspPrewarm, tool: ToolId) -> bool {
    use octocode_runtime_base::config::LspPrewarm;
    match prewarm {
        LspPrewarm::Off => false,
        LspPrewarm::Targeted => matches!(
            tool,
            ToolId::LocalSearch | ToolId::LocalFetch | ToolId::AstSearch
        ),
        LspPrewarm::All => true,
    }
}

/// When the language-server waits of a request end: the caller's deadline,
/// else `reply_budget` from `now` (the channel's default, `None` when it
/// has none).
fn lsp_wait_deadline(
    caller: Option<std::time::Instant>,
    reply_budget: Option<std::time::Duration>,
    now: std::time::Instant,
) -> Option<std::time::Instant> {
    caller.or_else(|| reply_budget.and_then(|budget| now.checked_add(budget)))
}

/// Runs language-server work for the request `context` admits. A starved
/// server may use the time the request still has: its waits extend while it
/// progresses, up to the caller's deadline; with none, up to `reply_budget`
/// from now ([`lsp_wait_deadline`]), and with neither for as long as it
/// progresses. The reply budget ends only the server waits (the row says
/// what it has); it never times the request out. Cancelling the request
/// (the caller's cancel, or the runtime at the deadline) drops `work` at
/// once, which cancels its pending server requests, instead of holding the
/// worker until the wait ends.
async fn within_request<F: Future>(
    context: &ExecutionContext,
    reply_budget: Option<std::time::Duration>,
    work: F,
) -> Result<F::Output, ExecutionError> {
    let deadline = lsp_wait_deadline(context.deadline, reply_budget, std::time::Instant::now())
        .map(tokio::time::Instant::from_std);
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
    octocode_engine::contain::catch(Phase::Row, work).unwrap_or_else(|panic| {
        let at = panic
            .site
            .map(|site| format!(" at {site}"))
            .unwrap_or_default();
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
