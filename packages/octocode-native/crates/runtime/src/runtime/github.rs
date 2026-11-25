//! One pooled HTTP client; credential acquisition stays inside admitted blocking work.

use octocode_github::*;
use octocode_io::budget::{PROVIDER_BODY_BYTES, RequestBudget};
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::{
    config::ConfigOutput,
    id::ToolId,
    policy::path::PathPolicy,
    providers::github::{Authentication, CallSelection},
    response::outcome::DomainResult,
    security::ContentSecurity,
};
use octocode_tools_shared::gh_git::SystemGit;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};

mod access;
mod release_tags;
mod repository_read;
mod rows;
mod tools;
mod warm;

pub(super) use access::ResponseAccess;
pub(crate) use release_tags::ReleaseTagCheck;
pub(crate) use repository_read::RepositoryReader;
use rows::advise_login_for_anonymous_quota;
pub(super) use rows::provider_error;
pub(crate) use warm::BatchWarm;

/// Auth failure of a token the host supplied with the request: the remote
/// caller has no local `octocode auth login` to run.
const REQUEST_TOKEN_AUTH_HINT: &str = "GitHub rejected the token supplied with this request; re-authenticate with the host and retry.";

pub(super) struct QueryExecution<'a> {
    pub context: &'a ExecutionContext,
    pub handle: &'a tokio::runtime::Handle,
    pub access: &'a ResponseAccess,
    /// The batch's warm-up, when its rows share one.
    pub warm: Option<&'a BatchWarm>,
}

pub(super) struct GitHubServices {
    credentials: Authentication,
    provider: GitHubProvider,
    timeout: Duration,
    home: PathBuf,
    /// `storage.mode` allows writing under the home: materialize needs it.
    persistent: bool,
    lsp_trust_fetched_code: bool,
    /// `storage.cloneCacheLimit` in bytes (0: no cap).
    clone_cap_bytes: u64,
    /// ghCloneRepo's git, keeping the runtime env's PATH, proxy and CA names.
    git: std::sync::Arc<SystemGit>,
    /// The automatic response page: file and patch pages are sized to fit
    /// it when the call names no `responseChars`.
    auto_page_chars: usize,
}

impl GitHubServices {
    pub(crate) fn capture_content(
        &self,
        query: &Value,
        context: &ExecutionContext,
        handle: &tokio::runtime::Handle,
        max_bytes: usize,
        access: &ResponseAccess,
        withholds: &(dyn Fn(&str) -> bool + Sync),
    ) -> Result<Result<(ContentResponse, Option<String>), String>, ProviderError> {
        let mut request_context = self.request_context(context, handle, access)?;
        // GitHub's JSON content envelope includes base64 and metadata. The
        // decoded bytes are checked separately.
        request_context.budget.max_body_bytes = max_bytes.saturating_mul(2);
        let request = ContentRequest {
            owner: query["owner"].as_str().unwrap_or_default().into(),
            repo: query["repo"].as_str().unwrap_or_default().into(),
            path: query["path"].as_str().unwrap_or_default().into(),
            reference: query["ref"].as_str().map(str::to_owned),
        };
        // A symlink is followed to its file: `withholds` judges the target's
        // name before any of its bytes are asked for, and the name the read
        // resolved to comes back beside the bytes.
        let result = handle.block_on(self.provider.get_file_bytes_following_links(
            &request,
            &request_context,
            withholds,
        ));
        if result
            .as_ref()
            .is_err_and(|error| error.kind == ProviderErrorKind::Authentication)
            && context.github_credential.is_none()
        {
            self.reject_credential(access.selection(), Some(&request_context));
        }
        result
    }

    pub(super) fn new(
        config: Arc<ConfigOutput>,
        limits: &octocode_runtime_base::execution::RuntimeLimits,
        home: PathBuf,
        answers: Arc<octocode_runtime_base::providers::saved_answers::KeptAnswers>,
    ) -> Result<Self, ProviderError> {
        let endpoint =
            GitHubEndpoint::new(url::Url::parse(&config.resolved.github.api_url).map_err(
                |_| ProviderError::new(ProviderErrorKind::Configuration, "Invalid GitHub API URL"),
            )?)?;
        let mut transport = GitHubTransport::new(
            endpoint,
            RetryPolicy {
                max_attempts: limits.max_retries.saturating_add(1),
                ..Default::default()
            },
            octocode_github::GitHubBudget::paced(limits.github_pacing),
        )?;
        // Cross-process rate-limit coordination follows the storage policy.
        transport.set_rate_limit_state_dir(
            octocode_runtime_base::config::is_persistent_storage_enabled(&config.resolved)
                .then(|| home.join("tmp").join("ratelimit")),
        );
        transport.set_saved_answers(Some(answers));
        let git = std::sync::Arc::new(SystemGit::with_env(&config.credential_env()));
        let persistent =
            octocode_runtime_base::config::is_persistent_storage_enabled(&config.resolved);
        let lsp_trust_fetched_code = config.resolved.lsp.trust_fetched_code;
        let clone_cap_bytes = octocode_tools_shared::gh_git::cap::clone_cap_from_setting(
            &config.resolved.storage.clone_cache_limit,
        );
        let credentials = Authentication::new(config);
        Ok(Self {
            credentials,
            provider: GitHubProvider::new(transport),
            timeout: limits.network_timeout,
            home,
            persistent,
            lsp_trust_fetched_code,
            clone_cap_bytes,
            git,
            auto_page_chars: limits.auto_page_chars,
        })
    }

    pub(super) fn execute_query(
        &self,
        tool: ToolId,
        query: &Value,
        execution: QueryExecution<'_>,
        security: &ContentSecurity,
        paths: &PathPolicy,
    ) -> Result<DomainResult, ExecutionError> {
        let QueryExecution {
            context,
            handle,
            access,
            warm,
        } = execution;
        let Some(github_tool) = tool.github() else {
            return Err(ExecutionError::WorkerFailed);
        };
        context.check()?;
        let request_context =
            self.request_context(context, handle, access)
                .map(|request| match warm {
                    Some(warm) => request.with_ref_memo(warm.memo.clone()),
                    None => request,
                });
        context.check()?;
        let mut result = handle.block_on(self.execute_resolved(
            github_tool,
            query,
            request_context.as_ref(),
            context,
            security,
            paths,
        ))?;
        if let Ok(request) = &request_context {
            access.collect(request);
            // Only an observation older than this call is news: an answer
            // another row of the call just observed is fresh.
            if let Some(observed) = request
                .reused_since()
                .filter(|&observed| access.predates(observed))
            {
                octocode_runtime_base::providers::saved_answers::disclose_reuse(
                    &mut result.data,
                    observed,
                );
            }
            // The row's own waits before its sends, by kind: latency the
            // provider did not cause. A row that waited for nothing says nothing.
            let waits = request.client_waits();
            if let Some(data) = result.data.as_object_mut()
                && !waits.is_empty()
            {
                data.insert("clientWaits".into(), json!(waits));
            }
        }
        if octocode_runtime_base::id::error_code_of(&result.data)
            == Some(octocode_contract_types::ErrorCode::Authentication)
        {
            if context.github_credential.is_some() {
                // The host supplied this token: the host re-authenticates
                // its caller. Ambient credential state stays untouched.
                octocode_runtime_base::response::result::set_hints(
                    &mut result.data,
                    [REQUEST_TOKEN_AUTH_HINT],
                );
            } else {
                // GitHub rejected the credential: the next row selects again without it.
                self.reject_credential(access.selection(), request_context.as_ref().ok());
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
        handle
            .block_on(self.resolve_request(context, access))
            .map(|request| access.resume(request))
    }

    fn reject_credential(&self, call: &CallSelection, request: Option<&RequestContext>) {
        self.credentials.reject_shared(
            call,
            self.provider.transport.endpoint().credential_host(),
            request.and_then(RequestContext::resolved_credential),
        );
    }

    fn request_budget(&self, context: &ExecutionContext) -> RequestBudget {
        // The call's own deadline ends the request; the network timeout is
        // a stall bound on each read, so a response still flowing is never cut.
        RequestBudget::new(
            octocode_runtime_base::execution::cancel::deadline_of(context),
            context.cancellation.clone(),
            PROVIDER_BODY_BYTES,
        )
        .with_stall(self.timeout)
    }

    /// The identity a saved response is checked against: the call's own
    /// credential once a row selected it, else a read-only inspection.
    pub(super) fn response_identity(
        &self,
        context: &ExecutionContext,
        handle: &tokio::runtime::Handle,
        access: &ResponseAccess,
    ) -> Result<String, ProviderError> {
        let host = self.provider.transport.endpoint().credential_host();
        if let Some(credential) = &context.github_credential {
            return Ok(Partition::new(host, Some(credential.secret()))
                .identity()
                .to_owned());
        }
        handle.block_on(self.credentials.identity_shared(
            access.selection(),
            host,
            &self.request_budget(context),
        ))
    }

    /// The call's credential, resolved by its first row and recorded for every entry.
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
                    .resolve_shared(access.selection(), host, &budget)
                    .await?
                    .map(|selection| selection.credential),
            };
            Ok(access.bounded(RequestContext::new(budget, credential), context.deadline))
        }
        .await;
        access.observe(host, &result);
        result
    }

    /// securityCheck's repository read for one row, through this transport:
    /// the same credential and budget as the release-tag checks.
    pub(crate) fn repository_reader<'a>(
        &'a self,
        context: &'a ExecutionContext,
        access: &'a ResponseAccess,
    ) -> RepositoryReader<'a> {
        RepositoryReader {
            services: self,
            context,
            access,
            anonymous: std::sync::atomic::AtomicBool::new(false),
        }
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
