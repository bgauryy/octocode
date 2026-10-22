//! One domain execution path for public tools and Jev's bounded read context.
use super::{dispatch, github, github_cache::GitHubContentCache};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::tools::lsp_search::lsp_search_query;
use crate::{
    config::ConfigOutput,
    id::{ToolFamily, ToolId},
    policy::path::PathPolicy,
    response::outcome::DomainResult,
    security::ContentSecurity,
    tools::lsp_search::{LspExecutionConfig, prewarm},
    tools::source_capture::SourceReader,
};
use octocode_github::ProviderError;
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
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

fn retained_access_error() -> crate::tools::judgment::ClassificationError {
    crate::tools::judgment::ClassificationError::new(
        "classificationContextUnavailable",
        "The saved source access identity changed or could not be verified. No saved evidence was returned.",
        "Check source access, then start a fresh request explicitly.",
    )
}

impl SourceReader for DomainDispatcher {
    fn paths(&self) -> &PathPolicy {
        &self.paths
    }

    fn security(&self) -> &ContentSecurity {
        &self.security
    }

    fn config(&self) -> &ConfigOutput {
        &self.config
    }

    fn limits(&self) -> &crate::execution::RuntimeLimits {
        &self.limits
    }

    fn home(&self) -> &Path {
        &self.home
    }

    fn handle(&self) -> &tokio::runtime::Handle {
        &self.handle
    }

    fn availability(&self) -> &crate::execution::availability::ToolAvailability {
        &self.availability
    }

    fn evaluator(&self) -> &crate::tools::judgment::Evaluator {
        &self.evaluator
    }

    fn execute(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<DomainResult, ExecutionError> {
        DomainDispatcher::execute(self, tool, query, context)
    }

    fn retained_github_identity(
        &self,
        context: &ExecutionContext,
    ) -> Result<Option<String>, crate::tools::judgment::ClassificationError> {
        let observed = self
            .github_access
            .identity()
            .map_err(|()| retained_access_error())?;
        self.validate_retained_github_identity(observed.as_deref(), context)?;
        Ok(observed)
    }

    fn validate_retained_github_identity(
        &self,
        expected: Option<&str>,
        context: &ExecutionContext,
    ) -> Result<(), crate::tools::judgment::ClassificationError> {
        let check = || {
            context.check().map_err(|error| {
                crate::tools::judgment::ClassificationError::new(
                    error.code(),
                    error.to_string(),
                    "Retry the request explicitly.",
                )
            })
        };
        check()?;
        let Some(expected) = expected else {
            return Ok(());
        };
        let identity = self.github_response_identity(context);
        check()?;
        if identity.as_ref().is_ok_and(|identity| identity == expected) {
            Ok(())
        } else {
            Err(retained_access_error())
        }
    }

    fn text_capture_limit(&self) -> usize {
        crate::tools::local_fetch::MAX_SOURCE_BYTES as usize
    }

    fn capture_local_text(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<(String, PathBuf, String), crate::tools::judgment::ClassificationError> {
        let parsed = serde_json::from_value(query.clone()).map_err(|_| {
            crate::tools::judgment::ClassificationError::new(
                "invalidClassificationContext",
                "The local resource query could not be decoded.",
                "Inspect the localFetch schema.",
            )
        })?;
        let captured =
            crate::tools::local_fetch::capture_text(&parsed, self.paths.as_ref(), context);
        context.check().map_err(|error| {
            crate::tools::judgment::ClassificationError::new(
                error.code(),
                error.to_string(),
                "Retry the request explicitly.",
            )
        })?;
        let captured = captured.map_err(|result| crate::tools::judgment::ClassificationError {
            code: result
                .error_code
                .unwrap_or_else(|| "classificationContextUnavailable".into()),
            message: result
                .error
                .unwrap_or_else(|| "The local source could not be captured.".into()),
            hints: result.hints,
            ..Default::default()
        })?;
        Ok((captured.text, captured.canonical, captured.snapshot))
    }

    /// Acquire source bytes through the same GitHub credential, commit
    /// resolution, byte cache and request budget as a file read.
    fn capture_github_content(
        &self,
        query: &Value,
        context: &ExecutionContext,
        max_bytes: usize,
    ) -> Result<octocode_github::ContentResponse, crate::tools::judgment::ClassificationError> {
        let acquired = self
            .github_services()
            .map_err(Clone::clone)
            .and_then(|services| {
                services.capture_content(
                    query,
                    context,
                    &self.handle,
                    max_bytes,
                    &self.github_access,
                )
            });
        acquired.map_err(|error| {
            let row = github::provider_error(error);
            crate::tools::judgment::ClassificationError {
                code: row.data["errorCode"]
                    .as_str()
                    .unwrap_or("classificationContextFailed")
                    .into(),
                message: row.data["error"]
                    .as_str()
                    .unwrap_or("The GitHub source could not be acquired.")
                    .into(),
                failure: row.failure,
                hints: row.data["hints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                ..Default::default()
            }
        })
    }
}

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
    pub fn execute(
        &self,
        tool: ToolId,
        query: &Value,
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
                    context.response_window,
                ),
            )
            .await?
            {
                Ok(data) => Ok(dispatch::value_result(data)),
                Err(failure) => {
                    // A cancelled/expired request is a runtime outcome,
                    // not a provider failure.
                    context.check()?;
                    let missing = failure.code == crate::policy::PATH_NOT_FOUND;
                    let mut row = dispatch::provider_failure(
                        failure.message,
                        failure.code.into(),
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
        let window = local_page_chars(tool, context, self.limits.auto_page_chars);
        let context = ExecutionContext {
            response_window: (window > 0).then_some(window),
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
mod within_request_tests {
    use super::*;

    fn context(deadline: std::time::Duration) -> ExecutionContext {
        ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: std::time::Instant::now() + deadline,
            walk_threads: None,
            response_window: None,
            github_credential: None,
        }
    }

    #[tokio::test]
    async fn finished_work_returns_its_output() {
        let context = context(std::time::Duration::from_secs(60));
        assert_eq!(within_request(&context, async { 7 }).await, Ok(7));
    }

    /// A cancelled request does not wait for its language-server work.
    #[tokio::test]
    async fn cancelling_the_request_drops_pending_work_at_once() {
        let context = context(std::time::Duration::from_secs(600));
        let token = context.cancellation.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            token.cancel();
        });
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            within_request(&context, std::future::pending::<()>()),
        )
        .await
        .expect("cancellation ends the work without waiting for it");
        assert_eq!(outcome, Err(ExecutionError::Cancelled));
    }
}

/// The characters one local row's own pages fill. Streamed listing and walk
/// pages are cut at the automatic response window, which every call of a
/// walk shares (an explicit `responseLength` is per call, so it never moves
/// a page cut). localFetch continues by offset: its reader fits the row's
/// share of the response page, the caller's or the automatic
/// ([`ExecutionContext::row_page_chars`], see `response::read_share`), and
/// each row continues by its own `next.continueWindow`.
fn local_page_chars(tool: ToolId, context: &ExecutionContext, auto_page_chars: usize) -> usize {
    if tool == ToolId::LocalFetch {
        context.row_page_chars(auto_page_chars)
    } else {
        auto_page_chars
    }
}

#[cfg(test)]
mod page_chars_tests {
    use super::*;

    #[test]
    fn local_fetch_fills_the_rows_share_and_other_tools_the_automatic_page() {
        let context = |response_window| ExecutionContext {
            cancellation: tokio_util::sync::CancellationToken::new(),
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(5),
            walk_threads: None,
            response_window,
            github_credential: None,
        };
        let fetch = |window| local_page_chars(ToolId::LocalFetch, &context(window), 20_000);
        assert_eq!(fetch(Some(5_000)), 5_000);
        assert_eq!(fetch(Some(50_000)), 50_000);
        assert_eq!(fetch(None), 20_000);
        for tool in [ToolId::LocalSearch, ToolId::StructureSearch] {
            assert_eq!(
                local_page_chars(tool, &context(Some(5_000)), 20_000),
                20_000
            );
        }
    }
}

/// A failed engine operation becomes one row; sibling rows keep executing.
/// Worker-thread and FFI boundaries retain their own unwind containment.
fn execute_row(
    work: impl FnOnce() -> Result<DomainResult, ExecutionError>,
) -> Result<DomainResult, ExecutionError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or_else(|_| {
        Ok(DomainResult::failure(
            "executionFailed",
            "The tool row failed on pathological input.",
            Vec::new(),
            None,
        ))
    })
}

#[cfg(test)]
mod row_tests {
    use super::*;

    #[test]
    fn panic_in_one_row_does_not_abort_following_rows() {
        let rows = [true, false].map(|panics| {
            execute_row(|| {
                assert!(!panics, "injected row failure with private input");
                Ok(DomainResult::payload(serde_json::json!({"value": 7}), None))
            })
            .expect("each row returns its own result")
        });
        assert_eq!(rows[0].status, Some("error"));
        assert_eq!(rows[0].data["errorCode"], "executionFailed");
        assert!(!rows[0].data.to_string().contains("private input"));
        assert_eq!(rows[1].data["value"], 7);
        assert_eq!(rows[1].status, None);
    }
}
