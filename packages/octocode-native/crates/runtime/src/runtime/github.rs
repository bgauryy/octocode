//! One pooled HTTP client; credential acquisition stays inside admitted blocking work.
use super::{FailureKind, github_cache::GitHubContentCache};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::{
    config::ConfigOutput,
    id::ToolId,
    policy::path::PathPolicy,
    providers::github::{AuthMode, Authentication},
    response::outcome::DomainResult,
    security::ContentSecurity,
    tools::gh_clone_repo::SystemGit,
};
use octocode_github::*;
use octocode_io::budget::RequestBudget;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};

mod access;
mod release_tags;
mod rows;
mod tools;

pub(super) use access::ResponseAccess;
pub(crate) use release_tags::ReleaseTagCheck;
use rows::advise_login_for_anonymous_quota;
pub(super) use rows::provider_error;

/// Auth failure of a token the host supplied with the request: the remote
/// caller has no local `octocode auth login` to run.
const REQUEST_TOKEN_AUTH_HINT: &str = "GitHub rejected the token supplied with this request; re-authenticate with the host and retry.";

pub(super) struct QueryExecution<'a> {
    pub context: &'a ExecutionContext,
    pub handle: &'a tokio::runtime::Handle,
    pub access: &'a ResponseAccess,
}

pub(super) struct GitHubServices {
    credentials: Authentication,
    provider: GitHubProvider,
    timeout: Duration,
    home: PathBuf,
    /// ghCloneRepo's git, keeping the runtime env's PATH, proxy and CA names.
    git: SystemGit,
    /// The automatic response page: file and patch pages are sized to fit
    /// it when the call names no `responseChars`.
    auto_page_chars: usize,
    /// Sanitized full views of recently paged files (scoped to this runtime's
    /// single security policy), so each `next.nextPage` skips a full rescan.
    sanitized_views: Arc<crate::security::scan::SanitizedViewMemo>,
    source_views: Arc<crate::tools::source_view::ViewCache>,
}

impl GitHubServices {
    pub(super) fn clear_memory(&self) {
        self.provider.transport.clear_metadata();
    }

    pub(crate) fn capture_content(
        &self,
        query: &Value,
        context: &ExecutionContext,
        handle: &tokio::runtime::Handle,
        max_bytes: usize,
        access: &ResponseAccess,
    ) -> Result<ContentResponse, ProviderError> {
        let mut request_context = self.request_context(context, handle, access)?;
        // GitHub's JSON content envelope includes base64 and metadata. The
        // decoded bytes are checked separately, including cache hits.
        request_context.budget.max_body_bytes = max_bytes.saturating_mul(2);
        let request = ContentRequest {
            owner: query["owner"].as_str().unwrap_or_default().into(),
            repo: query["repo"].as_str().unwrap_or_default().into(),
            path: query["path"].as_str().unwrap_or_default().into(),
            reference: query["ref"].as_str().map(str::to_owned),
            force_refresh: false,
        };
        let result = handle.block_on(self.provider.get_file_bytes(&request, &request_context));
        if result
            .as_ref()
            .is_err_and(|error| error.kind == ProviderErrorKind::Authentication)
            && context.github_credential.is_none()
        {
            self.reject_credential(Some(&request_context));
        }
        result
    }

    pub fn new(
        config: Arc<ConfigOutput>,
        limits: &crate::execution::RuntimeLimits,
        home: PathBuf,
        cache: GitHubContentCache,
        source_views: Arc<crate::tools::source_view::ViewCache>,
    ) -> Result<Self, ProviderError> {
        let endpoint =
            GitHubEndpoint::new(url::Url::parse(&config.resolved.github.api_url).map_err(
                |_| ProviderError::new(ProviderErrorKind::Configuration, "Invalid GitHub API URL"),
            )?)?;
        let mut transport = GitHubTransport::new(
            endpoint,
            RetryPolicy {
                max_attempts: limits.max_retries.saturating_add(1),
                max_rate_limit_wait: limits.max_rate_limit_wait,
                ..Default::default()
            },
            octocode_github::GitHubBudget::paced(limits.github_pacing),
        )?;
        transport.graphql_enabled = config.resolved.github.graphql_enabled;
        // Same persistence switch as the response cache: cross-process
        // rate-limit facts live under ~/.octocode/tmp/ratelimit.
        transport.set_rate_limit_state_dir(
            crate::config::is_persistent_storage_enabled(&config.resolved)
                .then(|| home.join("tmp").join("ratelimit")),
        );
        let git = SystemGit::with_env(&config.credential_env());
        let credentials = Authentication::new(config);
        Ok(Self {
            credentials,
            provider: GitHubProvider::new(transport, cache),
            timeout: limits.network_timeout,
            home,
            git,
            auto_page_chars: limits.auto_page_chars,
            sanitized_views: Arc::new(crate::security::scan::SanitizedViewMemo::new()),
            source_views,
        })
    }

    pub fn execute_query(
        &self,
        tool: ToolId,
        query: &Value,
        execution: QueryExecution<'_>,
        security: &ContentSecurity,
        paths: &PathPolicy,
        selection: &crate::tools::selection::Runtime,
    ) -> Result<DomainResult, ExecutionError> {
        let QueryExecution {
            context,
            handle,
            access,
        } = execution;
        let Some(github_tool) = tool.github() else {
            return Err(ExecutionError::WorkerFailed);
        };
        context.check()?;
        let request_context = self.request_context(context, handle, access);
        context.check()?;
        let mut result = handle.block_on(self.execute_resolved(
            github_tool,
            query,
            request_context.as_ref(),
            context,
            security,
            paths,
            selection,
        ))?;
        if result.failure == Some(FailureKind::Authentication) {
            if context.github_credential.is_some() {
                // The host supplied this token: the host re-authenticates
                // its caller. Ambient credential state stays untouched.
                result.data["hints"] = json!([REQUEST_TOKEN_AUTH_HINT]);
            } else {
                // GitHub rejected the credential: the next row selects again without it.
                self.reject_credential(request_context.as_ref().ok());
            }
        }
        let anonymous = request_context
            .as_ref()
            .is_ok_and(|request| request.resolved_credential().is_none());
        advise_login_for_anonymous_quota(&mut result.data, anonymous);
        super::dispatch::offer_retry(tool, query, &mut result.data);
        Ok(result)
    }

    fn request_context(
        &self,
        context: &ExecutionContext,
        handle: &tokio::runtime::Handle,
        access: &ResponseAccess,
    ) -> Result<RequestContext, ProviderError> {
        handle.block_on(self.resolve_request(context, access))
    }

    fn reject_credential(&self, request: Option<&RequestContext>) {
        self.credentials.reject(
            self.provider.transport.endpoint().credential_host(),
            request.and_then(RequestContext::resolved_credential),
        );
    }

    fn request_budget(&self, context: &ExecutionContext) -> RequestBudget {
        let mut budget = RequestBudget::with_timeout(self.timeout, 16 * 1024 * 1024);
        budget.deadline = context.deadline.min(budget.deadline);
        budget.wait_until = context.deadline;
        budget.cancellation = context.cancellation.clone();
        budget
    }

    /// Read-only local auth inspection for an already saved response.
    pub(super) fn response_identity(
        &self,
        context: &ExecutionContext,
        handle: &tokio::runtime::Handle,
    ) -> Result<String, ProviderError> {
        let host = self.provider.transport.endpoint().credential_host();
        if let Some(credential) = &context.github_credential {
            return Ok(Partition::new(host, Some(credential.secret()))
                .identity()
                .to_owned());
        }
        handle.block_on(
            self.credentials
                .inspect_response_identity(host, &self.request_budget(context)),
        )
    }

    /// Resolve and record the exact credential used by every GitHub entry.
    async fn resolve_request(
        &self,
        context: &ExecutionContext,
        access: &ResponseAccess,
    ) -> Result<RequestContext, ProviderError> {
        let host = self.provider.transport.endpoint().credential_host();
        let result = async {
            let budget = self.request_budget(context);
            let credential = match &context.github_credential {
                Some(credential) => Some(credential.clone()),
                None => self
                    .credentials
                    .resolve(host, AuthMode::Request, &budget)
                    .await?
                    .map(|selection| selection.credential),
            };
            Ok(RequestContext::new(budget, credential))
        }
        .await;
        access.observe(host, &result);
        result
    }

    /// artifactSearch's upstream release-tag checks for one row, through
    /// this transport: the configured API URL, credential and budget.
    pub(crate) fn release_tags<'a>(
        &'a self,
        context: &'a ExecutionContext,
        access: &'a ResponseAccess,
    ) -> ReleaseTagCheck<'a> {
        ReleaseTagCheck {
            services: self,
            context,
            access,
        }
    }
}

#[cfg(test)]
mod tests;
