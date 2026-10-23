//! One domain execution path for public tools and Jev's bounded read context.
mod source_reader;
use super::{dispatch, github, github_cache::GitHubContentCache};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::tools::lsp_search::lsp_search_query;
use crate::{
    config::ConfigOutput,
    id::{ToolFamily, ToolId},
    policy::path::PathPolicy,
    response::outcome::DomainResult,
    security::{CheckedQuery, ContentSecurity},
    tools::lsp_search::{LspExecutionConfig, prewarm},
};
use octocode_contract_types::ErrorCode;
use octocode_github::ProviderError;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

pub(crate) struct DomainDispatcher {
    pub paths: Arc<PathPolicy>,
    pub security: Arc<ContentSecurity>,
    pub(super) github_services: Arc<OnceLock<Result<github::GitHubServices, ProviderError>>>,
    pub(super) github_cache: GitHubContentCache,
    pub(super) github_access: github::ResponseAccess,
    pub config: Arc<ConfigOutput>,
    /// Internal limits: the runtime's code constants.
    pub limits: crate::execution::RuntimeLimits,
    pub home: PathBuf,
    pub handle: tokio::runtime::Handle,
    pub lsp_pool: Arc<octocode_engine::lsp::pool::LspClientPool>,
    pub(crate) local_views: Arc<crate::security::scan::SanitizedViewMemo>,
    pub lsp_execution_config: Arc<LspExecutionConfig>,
    pub availability: Arc<crate::execution::availability::ToolAvailability>,
    /// Provider readiness and an enabled goals surface, independent of the
    /// public clasify tool's availability.
    pub(crate) selection_available: bool,
    pub(crate) evaluator: crate::tools::judgment::Evaluator,
    /// Registry answers for artifactSearch; `None` under memory storage.
    pub(crate) artifact_cache: Option<Arc<crate::providers::artifact::ArtifactCache>>,
}

/// A failed local capture as a classification error, with the reader's own
/// code, message, hints and leads (`viewTree`, `findFile`, `read`).
fn local_capture_failure(
    result: Box<crate::tools::source_view::LocalFetchResult>,
) -> crate::tools::judgment::ClassificationError {
    let leads = result
        .next
        .as_ref()
        .and_then(|next| serde_json::to_value(next).ok())
        .and_then(|next| next.as_object().cloned())
        .map(|next| next.into_iter().collect())
        .unwrap_or_default();
    crate::tools::judgment::ClassificationError {
        leads,
        ..crate::tools::judgment::ClassificationError::with_hints(
            result
                .error_code
                .unwrap_or(ErrorCode::ClassificationContextUnavailable),
            result
                .error
                .unwrap_or_else(|| "The local source could not be captured.".into()),
            result.hints,
        )
    }
}

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
                    self.github_cache.clone(),
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
            .response_identity(context, &self.handle)
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

    /// Every route matches the contract family exhaustively; within a
    /// family only the tools with their own runner are named. The runtime
    /// resolves the wire name once.
    pub(crate) fn execute(
        &self,
        tool: ToolId,
        query: &CheckedQuery,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        let _enter = self.handle.enter();
        context.check()?;
        execute_row(|| match tool.family() {
            ToolFamily::GitHub => self.execute_github(tool, query, context),
            ToolFamily::Local if tool == ToolId::LspSearch => self.execute_lsp(query, context),
            ToolFamily::Local => self.execute_local(tool, query, context),
            // Clasify runs its own batch, never a domain row.
            ToolFamily::Remote if tool.is_clasify() => Err(ExecutionError::UnroutedTool),
            ToolFamily::Remote => self.execute_artifact(query, context),
        })
    }

    fn execute_github(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
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

    fn execute_artifact(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        let row_query = query;
        let query = match dispatch::parse_query(query) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        // Release tags are checked through the GitHub transport; without
        // usable GitHub services a lookup reads no tags.
        let services = self.github_services().ok();
        let tags = services.map(|services| services.release_tags(context, &self.github_access));
        let env = self.config.credential_env();
        let call = crate::tools::artifact_search::ArtifactCall {
            deadline: context.deadline,
            cancellation: context.cancellation.clone(),
            allow_private_registry: self.config.resolved.network.allow_private_registry,
            cache: self.artifact_cache.as_deref(),
            timeout: self.limits.network_timeout,
            max_retries: self.limits.max_retries,
            tags: tags
                .as_ref()
                .map(|tags| tags as &dyn crate::providers::artifact::ReleaseTags),
            env: &env,
        };
        self.handle.block_on(async {
            Ok(
                match crate::tools::artifact_search::execute(query, call).await {
                    Ok(data) => dispatch::data_result(data),
                    Err(error) => {
                        let next = error.next;
                        let mut row = dispatch::provider_failure(
                            error.message,
                            error.code,
                            error.hints,
                            error.status,
                            None,
                        );
                        // A repair lead is the recovery: no code fallback
                        // text beside it.
                        if let Some(next) = next
                            && let Some(data) = row.data.as_object_mut()
                        {
                            data.shift_remove("hints");
                            data.insert("next".into(), *next);
                        }
                        // A retryable registry failure offers the same
                        // `next.retry` every GitHub failure does.
                        dispatch::offer_retry(
                            crate::id::ToolId::ArtifactSearch,
                            row_query,
                            &mut row.data,
                        );
                        row
                    }
                },
            )
        })
    }

    fn execute_lsp(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        let query: crate::tools::lsp_search::LspSearchQuery = match dispatch::parse_query(query) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        let requested = lsp_search_query::path(&query).map(str::to_owned);
        self.handle.block_on(async {
            match within_request(
                context,
                crate::tools::lsp_search::execute(
                    query,
                    context,
                    &self.lsp_pool,
                    &self.paths,
                    &self.lsp_execution_config,
                    local_response_window(ToolId::LspSearch, context, self.limits.auto_page_chars),
                ),
            )
            .await?
            {
                Ok(data) => Ok(dispatch::value_result(data)),
                Err(failure) => {
                    // A cancelled/expired request is a runtime outcome,
                    // not a provider failure.
                    context.check()?;
                    let missing = failure.code == ErrorCode::PathNotFound;
                    let mut row = dispatch::provider_failure(
                        failure.message,
                        failure.code,
                        vec![failure.hint.into()],
                        None,
                        Some(failure.retryable),
                    );
                    // The same recovery every local tool gives a missing path.
                    if missing
                        && let Some(next) = requested.as_deref().and_then(|requested| {
                            crate::response::result::missing_path_lead(&self.paths, requested)
                        })
                    {
                        row.data["next"] = next;
                    }
                    Ok(row)
                }
            }
        })
    }

    fn execute_local(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        let context = ExecutionContext {
            response_window: local_response_window(tool, context, self.limits.auto_page_chars),
            ..context.clone()
        };
        let selection = self.selection_runtime(&context);
        // `lsp.prewarm`: `targeted` warms the server an offered lspSearch
        // call names; `all` also warms the file a read or search names.
        let prewarm_mode = self.config.resolved.lsp.prewarm;
        let prewarm_query = (prewarm_mode == crate::config::LspPrewarm::All).then_some(query);
        let lsp = &self.lsp_execution_config;
        // The row already runs on a blocking or scoped worker thread inside
        // the runtime context ([`Self::execute`]): no further thread hop.
        // lspSearch leads offered by local tools probe the runtime's resolved
        // server settings, not the bare process env.
        let result = crate::tools::leads::lsp::with_lead_discovery(
            lsp.lead_discovery(&self.paths),
            lsp.lead_cache.clone(),
            || {
                dispatch::execute_local(
                    tool,
                    query,
                    &context,
                    &dispatch::LocalEnv {
                        paths: &self.paths,
                        security: &self.security,
                        views: &self.local_views,
                        selection: Some(&selection),
                    },
                )
            },
        )?;
        // Start a language server in the background so a following
        // lspSearch finds a warm pooled client: for the file an offered
        // lspSearch call names, or (opt-in) the file a read or search names.
        let lead = (prewarm_mode != crate::config::LspPrewarm::Off)
            .then(|| prewarm::lead_file(&result.data))
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
/// `responseLength` is per call, so it never moves a page cut and a
/// continuation under another window names the same page). localFetch
/// continues by offset: its reader fits the row's share of the response
/// page, the caller's or the automatic ([`ExecutionContext::row_page_chars`],
/// see `response::read_share`), and each row continues by its own
/// `next.continueWindow`.
fn local_response_window(
    tool: ToolId,
    context: &ExecutionContext,
    auto_page_chars: usize,
) -> Option<usize> {
    let window = if tool == ToolId::LocalFetch {
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
fn execute_row(
    work: impl FnOnce() -> Result<DomainResult, ExecutionError>,
) -> Result<DomainResult, ExecutionError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|_| {
        Ok(DomainResult::failure(
            ErrorCode::ExecutionFailed,
            "The tool row failed on pathological input.",
            Vec::new(),
            None,
        ))
    })
}

#[cfg(test)]
mod row_tests;
