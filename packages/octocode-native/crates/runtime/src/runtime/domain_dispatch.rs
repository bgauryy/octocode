//! One domain execution path for public tools and Jev's bounded read context.
use super::{ExecutionContext, ExecutionError, dispatch, github, github_cache::GitHubContentCache};
use crate::{
    config::ConfigOutput,
    policy::path::PathPolicy,
    providers::github::ProviderError,
    security::ContentSecurity,
    tools::{
        id::{ToolFamily, ToolId},
        local_fetch::LocalFetchRegex,
        lsp_search::{LspExecutionConfig, prewarm},
    },
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

pub(crate) struct DomainDispatcher {
    pub paths: Arc<PathPolicy>,
    pub security: Arc<ContentSecurity>,
    pub regex: LocalFetchRegex,
    pub(super) github_services: Arc<OnceLock<Result<github::GitHubServices, ProviderError>>>,
    pub(super) github_cache: GitHubContentCache,
    pub config: Arc<ConfigOutput>,
    pub home: PathBuf,
    pub handle: tokio::runtime::Handle,
    pub lsp_pool: Arc<octocode_engine::lsp::pool::LspClientPool>,
    pub local_views: Arc<crate::security::scan::SanitizedViewMemo>,
    pub lsp_execution_config: Arc<LspExecutionConfig>,
    pub available_tools: Arc<[&'static str]>,
    /// Registry answers for artifactSearch; `None` under memory storage.
    pub artifact_cache: Option<Arc<crate::providers::artifact::ArtifactCache>>,
}

impl DomainDispatcher {
    /// Every route matches the contract family exhaustively; within a
    /// family only the tools with their own runner are named. The runtime
    /// resolves the wire name once.
    pub fn execute(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<dispatch::DomainResult, ExecutionError> {
        let _enter = self.handle.enter();
        context.check()?;
        match tool.family() {
            ToolFamily::GitHub => self.execute_github(tool, query, context),
            ToolFamily::Local if tool == ToolId::LspSearch => self.execute_lsp(query, context),
            ToolFamily::Local => self.execute_local(tool, query, context),
            // Clasify runs its own batch, never a domain row.
            ToolFamily::Remote if tool.is_clasify() => Err(ExecutionError::UnroutedTool),
            ToolFamily::Remote => self.execute_artifact(query, context),
        }
    }

    fn execute_github(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<dispatch::DomainResult, ExecutionError> {
        match self.github_services.get_or_init(|| {
            github::GitHubServices::new(
                self.config.clone(),
                self.home.clone(),
                self.github_cache.clone(),
            )
        }) {
            Ok(services) => services.execute_query(
                tool,
                query,
                context,
                &self.security,
                &self.regex,
                &self.handle,
                &self.paths,
            ),
            Err(error) => Ok(github::provider_error(error.clone())),
        }
    }

    fn execute_artifact(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<dispatch::DomainResult, ExecutionError> {
        let query = match dispatch::parse_query(query) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        // Release tags are checked through the GitHub transport; without
        // usable GitHub services a lookup reads no tags.
        let services = self
            .github_services
            .get_or_init(|| {
                github::GitHubServices::new(
                    self.config.clone(),
                    self.home.clone(),
                    self.github_cache.clone(),
                )
            })
            .as_ref()
            .ok();
        let tags = services.map(|services| services.release_tags(context));
        let network = &self.config.resolved.network;
        let env = self.config.credential_env();
        let call = crate::tools::artifact_search::ArtifactCall {
            deadline: context.deadline,
            cancellation: context.cancellation.clone(),
            allow_private_registry: network.allow_private_registry,
            cache: self.artifact_cache.as_deref(),
            timeout: std::time::Duration::from_secs_f64(network.timeout / 1000.0),
            max_retries: network.max_retries as u8,
            tags: tags
                .as_ref()
                .map(|tags| tags as &dyn crate::providers::artifact::ReleaseTags),
            env: &env,
        };
        self.handle.block_on(async {
            Ok(
                match crate::tools::artifact_search::execute(query, call).await {
                    Ok(data) => dispatch::value_result(data),
                    Err(error) => dispatch::provider_failure(
                        error.message,
                        error.code,
                        error.hints,
                        error.status,
                        None,
                    ),
                },
            )
        })
    }

    fn execute_lsp(
        &self,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<dispatch::DomainResult, ExecutionError> {
        let query = match dispatch::parse_query(query) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        self.handle.block_on(async {
            match crate::tools::lsp_search::execute(
                query,
                context,
                &self.lsp_pool,
                &self.paths,
                &self.lsp_execution_config,
            )
            .await
            {
                Ok(data) => Ok(dispatch::value_result(data)),
                Err(failure) => {
                    // A cancelled/expired request is a runtime outcome,
                    // not a provider failure.
                    context.check()?;
                    Ok(dispatch::provider_failure(
                        failure.message,
                        failure.code.into(),
                        vec![failure.hint.into()],
                        None,
                        Some(failure.retryable),
                    ))
                }
            }
        })
    }

    fn execute_local(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<dispatch::DomainResult, ExecutionError> {
        let query = query.clone();
        let paths = self.paths.clone();
        let security = self.security.clone();
        let regex = self.regex.clone();
        let views = self.local_views.clone();
        // Streamed listing pages are sized to fit one automatic response
        // window, which every call of a walk shares (an explicit
        // `responseLength` is per call, so it never moves a page cut).
        let window = self.config.resolved.output.pagination.default_char_length as usize;
        let context = ExecutionContext {
            response_window: (window > 0).then_some(window),
            ..context.clone()
        };
        // OCTOCODE_BETA is the shared gate for beta local tools. For astRewrite,
        // enabling beta permits both preview and hash-guarded apply.
        let allow_apply = self.config.resolved.local.beta;
        let cargo = self.config.env_value("OCTOCODE_CARGO").map(str::to_owned);
        let prewarm_query =
            prewarm::enabled(self.config.env_value("OCTOCODE_LSP_PREWARM")).then(|| query.clone());
        let lsp = self.lsp_execution_config.clone();
        let result = self.handle.block_on(async {
            tokio::task::spawn_blocking(move || {
                // lspSearch leads offered by local tools probe the runtime's
                // resolved server settings, not the bare process env.
                crate::tools::lsp_search::with_lead_discovery(&lsp, &paths, || {
                    dispatch::execute_local(
                        tool,
                        &query,
                        &paths,
                        &security,
                        &context,
                        &regex,
                        &views,
                        allow_apply,
                        cargo.as_deref(),
                    )
                })
            })
            .await
            .map_err(|_| ExecutionError::WorkerFailed)?
        })?;
        // Start a language server in the background so a following
        // lspSearch finds a warm pooled client: for the file an offered
        // lspSearch call names, or (opt-in) the file a read or search names.
        let lead = prewarm::targeted_enabled(self.config.env_value("OCTOCODE_LSP_PREWARM"))
            .then(|| prewarm::lead_file(&result.data))
            .flatten();
        if let Some(file) = lead
            .or_else(|| prewarm_query.and_then(|query| prewarm::anchor_file(&query, &result.data)))
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
