//! One domain execution path for public tools and Jev's bounded read context.
use super::{ExecutionContext, ExecutionError, dispatch, github, github_cache::GitHubContentCache};
use crate::{
    config::ConfigOutput,
    policy::path::PathPolicy,
    providers::github::ProviderError,
    security::ContentSecurity,
    tools::{id::ToolId, local_fetch::LocalFetchRegex, lsp_search::LspExecutionConfig},
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

pub(super) struct DomainDispatcher {
    pub paths: Arc<PathPolicy>,
    pub security: Arc<ContentSecurity>,
    pub regex: LocalFetchRegex,
    pub github_services: Arc<OnceLock<Result<github::GitHubServices, ProviderError>>>,
    pub github_cache: GitHubContentCache,
    pub config: Arc<ConfigOutput>,
    pub home: PathBuf,
    pub handle: tokio::runtime::Handle,
    pub lsp_pool: Arc<octocode_engine::lsp::pool::LspClientPool>,
    pub lsp_execution_config: LspExecutionConfig,
    pub available_tools: Vec<&'static str>,
}

impl DomainDispatcher {
    pub fn execute(
        &self,
        tool: &str,
        query: &Value,
        context: &ExecutionContext,
    ) -> Result<dispatch::DomainResult, ExecutionError> {
        let _enter = self.handle.enter();
        context.check()?;
        if matches!(ToolId::from_name(tool), Some(id) if id.is_github()) {
            return match self.github_services.get_or_init(|| {
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
            };
        }
        if tool == "artifactSearch" {
            return self.handle.block_on(async {
                Ok(
                    match crate::tools::artifact_search::execute(
                        query,
                        context.deadline,
                        context.cancellation.clone(),
                        self.config.resolved.network.allow_private_registry,
                        Some(&self.home),
                        self.config.revision,
                        self.config.resolved.storage.mode == "persistent",
                    )
                    .await
                    {
                        Ok(data) => dispatch::value_result(data),
                        Err(error) => dispatch::provider_failure(
                            error.message,
                            error.code,
                            error.hints,
                            error.status,
                        ),
                    },
                )
            });
        }
        if tool == "lspSearch" {
            return self.handle.block_on(async {
                match crate::tools::lsp_search::execute(
                    query.clone(),
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
                        let mut result = dispatch::provider_failure(
                            failure.message,
                            failure.code.into(),
                            vec![failure.hint.into()],
                            None,
                        );
                        result.data["retryable"] = serde_json::json!(failure.retryable);
                        Ok(result)
                    }
                }
            });
        }
        let tool = tool.to_owned();
        let query = query.clone();
        let paths = self.paths.clone();
        let security = self.security.clone();
        let regex = self.regex.clone();
        let context = context.clone();
        // OCTOCODE_BETA is the shared gate for beta local tools. For astRewrite,
        // enabling beta permits both preview and hash-guarded apply.
        let allow_apply = self.config.resolved.local.beta;
        self.handle.block_on(async {
            tokio::task::spawn_blocking(move || {
                dispatch::execute_local(
                    &tool,
                    &query,
                    &paths,
                    &security,
                    &context,
                    &regex,
                    allow_apply,
                )
            })
            .await
            .map_err(|_| ExecutionError::WorkerFailed)?
        })
    }
}
