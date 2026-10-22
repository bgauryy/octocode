//! One pooled HTTP client; credential acquisition stays inside admitted blocking work.
use super::{FailureKind, github_cache::GitHubContentCache};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::{
    config::ConfigOutput,
    id::{GitHubTool, ToolId},
    policy::path::PathPolicy,
    providers::github::{AuthMode, Authentication},
    response::outcome::{DomainResult, Upstream},
    response::result::{Continuation, ToolData},
    security::ContentSecurity,
    tools::{
        gh_clone_repo::{self, CloneConfig, CloneContext, CloneFailure, SystemGit},
        gh_get_file_content, gh_get_history_item, gh_search_code, gh_search_history,
        gh_search_repo,
        gh_shared::{GhFailure, provider_hint},
        gh_structure,
    },
};
use octocode_contract_types::ErrorCode;
use octocode_github::*;
use octocode_io::budget::RequestBudget;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};

const ANONYMOUS_RATE_LIMIT_HINT: &str =
    "Wait for the rate-limit reset, or run octocode auth login for a higher quota.";

/// Auth failure of a token the host supplied with the request: the remote
/// caller has no local `octocode auth login` to run.
const REQUEST_TOKEN_AUTH_HINT: &str = "GitHub rejected the token supplied with this request; re-authenticate with the host and retry.";

/// Every GitHub provider failure row: the shared provider row
/// ([`DomainResult::provider`]) with the arm's own hint (a per-call fact
/// the `errorCode` does not state), plus the debug-only request id and
/// documentation link. A row with no hint and no lead gets the code's one
/// shared recovery from the response stage
/// ([`crate::response::rows::apply_hint_policy`]).
fn provider_row(
    error: &ProviderError,
    message: impl Into<String>,
    hints: Vec<String>,
    next: Option<Value>,
) -> DomainResult {
    let code = error.kind.code();
    let upstream = Upstream {
        retryable: error.retryable,
        http_status: error.status,
        rate_limit: error.rate_limit.as_deref().map(rate_limit_block),
    };
    let mut row = DomainResult::provider(code, message, hints, next, upstream);
    if let Some(request_id) = &error.request_id {
        row.data["requestId"] = json!(request_id);
    }
    if let Some(documentation_url) = &error.documentation_url {
        row.data["documentationUrl"] = json!(documentation_url);
    }
    if let Some(sso_url) = &error.sso_url {
        row.data["ssoUrl"] = json!(sso_url);
    }
    row
}

/// `{resource?, remaining?, resetEpochSeconds?, retryAfterSeconds?}`: the
/// members GitHub reported, in seconds; an unknown member is absent (a
/// secondary limit has no remaining count to state).
fn rate_limit_block(rate: &RateLimit) -> Value {
    let mut block = serde_json::Map::new();
    if let Some(resource) = &rate.resource {
        block.insert("resource".into(), json!(resource));
    }
    for (key, value) in [
        ("remaining", rate.remaining),
        ("resetEpochSeconds", rate.reset_epoch_seconds),
        ("retryAfterSeconds", rate.retry_after_seconds),
    ] {
        if let Some(value) = value {
            block.insert(key.into(), json!(value));
        }
    }
    Value::Object(block)
}

/// A failure a retry can fix (throttling, an outage, a timeout, a dropped
/// connection) offers `next.retry`: the same row (a page, so an exact
/// replay) with the provider's delay when it named one. A page keeps a batch
/// that ran other rows incomplete.
fn offer_retry(tool: ToolId, query: &Value, data: &mut Value) {
    if data["retryable"] != json!(true) {
        return;
    }
    let mut retry = Continuation::new(tool, query.clone()).build();
    if let Some(seconds) = data.pointer("/rateLimit/retryAfterSeconds").cloned() {
        retry["retryAfterSeconds"] = seconds;
    }
    data["next"]["retry"] = retry;
}

/// Authenticating raises only the primary quota, so only an anonymous
/// caller whose primary bucket is spent (`rateLimit.remaining == 0`) is told
/// to log in; a secondary limit or circuit cooldown keeps its wait hint.
fn advise_login_for_anonymous_quota(data: &mut Value, anonymous: bool) {
    if anonymous
        && crate::id::error_code_of(data) == Some(ErrorCode::RateLimited)
        && data.pointer("/rateLimit/remaining") == Some(&json!(0))
    {
        data["hints"] = json!([ANONYMOUS_RATE_LIMIT_HINT]);
    }
}

/// Credential identities actually used by one dispatched batch, never shared
/// with another call. Failed or mixed resolution cannot authorize saved pages.
#[derive(Default)]
pub(super) struct ResponseAccess(std::sync::Mutex<(std::collections::BTreeSet<String>, bool)>);

impl ResponseAccess {
    fn observe(&self, host: &str, request: &Result<RequestContext, ProviderError>) {
        let mut used = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match request {
            Ok(request) => {
                used.0.insert(
                    Partition::new(
                        host,
                        request
                            .resolved_credential()
                            .map(ResolvedCredential::secret),
                    )
                    .identity()
                    .to_owned(),
                );
            }
            Err(_) => used.1 = true,
        }
    }

    pub(super) fn identity(&self) -> Result<Option<String>, ()> {
        let used = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if used.1 || used.0.len() > 1 {
            return Err(());
        }
        Ok(used.0.first().cloned())
    }
}

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
    /// The ghCloneRepo cache limits.
    clone_limits: crate::execution::CloneLimits,
    /// ghCloneRepo's git, keeping the runtime env's PATH, proxy and CA names.
    git: SystemGit,
    /// The automatic response page: file and patch pages are sized to fit
    /// it when the call names no `responseLength`.
    auto_page_chars: usize,
    /// Sanitized full views of recently paged files (scoped to this runtime's
    /// single security policy), so each `next.continueWindow` skips a full rescan.
    sanitized_views: Arc<crate::security::scan::SanitizedViewMemo>,
}

impl GitHubServices {
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
            clone_limits: limits.clone,
            git,
            auto_page_chars: limits.auto_page_chars,
            sanitized_views: Arc::new(crate::security::scan::SanitizedViewMemo::new()),
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
        offer_retry(tool, query, &mut result.data);
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

    async fn execute_resolved(
        &self,
        tool: GitHubTool,
        query: &Value,
        request: Result<&RequestContext, &ProviderError>,
        context: &ExecutionContext,
        security: &ContentSecurity,
        paths: &PathPolicy,
        selection: &crate::tools::selection::Runtime,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        // Generated query types carry the meta fields, so every tool parses
        // the validated row as-is.
        macro_rules! typed {
            ($type:ty) => {
                match super::dispatch::parse_query::<$type>(query) {
                    Ok(query) => query,
                    Err(row) => return Ok(*row),
                }
            };
        }
        let request = request.map_err(Clone::clone);
        let row = match tool {
            GitHubTool::GhSearchRepo => {
                let query = typed!(gh_search_repo::GhSearchRepoQuery);
                tool_row(gh_search_repo::run(&self.provider, &query, request).await)
            }
            GitHubTool::GhSearchCode => {
                let query = typed!(gh_search_code::GhSearchCodeQuery);
                tool_row(gh_search_code::run(&self.provider, &query, request, security).await)
            }
            GitHubTool::GhStructure => {
                let query = typed!(gh_structure::GhStructureQuery);
                tool_row(gh_structure::run(&self.provider, &query, request, &self.home).await)
            }
            GitHubTool::GhGetFileContent => {
                let query = typed!(gh_get_file_content::GhGetFileContentQuery);
                let shaping = gh_get_file_content::Shaping {
                    security: security.clone(),
                    memo: self.sanitized_views.clone(),
                    context: context.clone(),
                    selection: selection.clone(),
                };
                match gh_get_file_content::run(
                    &self.provider,
                    &query,
                    request,
                    Some(context.row_page_chars(self.auto_page_chars)),
                    shaping,
                )
                .await
                {
                    Ok(read) => DomainResult {
                        cache: read.cache,
                        ..tool_row(Ok(read.output))
                    },
                    Err(failure) => tool_row(Err(failure)),
                }
            }
            GitHubTool::GhCloneRepo => {
                let query = typed!(gh_clone_repo::GhCloneRepoQuery);
                self.run_clone(&query, request, context, paths).await
            }
            GitHubTool::GhGetHistoryItem => match request {
                Ok(request) => {
                    self.execute_history_item_resolved(query, request, context, security)
                        .await?
                }
                Err(error) => history_error(error, false, None),
            },
            GitHubTool::GhSearchHistory => match request {
                Ok(request) => {
                    self.execute_search_history_resolved(query, request, context, security)
                        .await?
                }
                Err(error) => history_error(error, true, None),
            },
        };
        context.check()?;
        Ok(row)
    }

    /// ghCloneRepo clones with git and checks the caller's access before
    /// serving a cache hit; CloneConfig treats the octocode home as its cache
    /// root and derives the clone, lock, stage and git-home dirs.
    async fn run_clone(
        &self,
        query: &gh_clone_repo::GhCloneRepoQuery,
        request: Result<&RequestContext, ProviderError>,
        context: &ExecutionContext,
        paths: &PathPolicy,
    ) -> DomainResult {
        let request = match request {
            Ok(request) => request,
            Err(error) => return provider_error(error),
        };
        let config = CloneConfig::persistent(self.home.clone())
            .with_limits(&self.clone_limits)
            .with_network_timeout(self.timeout);
        let clone_context = CloneContext {
            config: &config,
            endpoint: self.provider.transport.endpoint(),
            credential: request.resolved_credential(),
            cancellation: context,
            deadline: context.deadline,
            path_policy: paths,
            git: &self.git,
        };
        match gh_clone_repo::run(&self.provider, query, Ok(request), &clone_context).await {
            Ok(result) => {
                DomainResult::payload(serde_json::to_value(result).unwrap_or_default(), None)
            }
            Err(CloneFailure::Provider(error)) => provider_error(error),
            Err(CloneFailure::Clone(error)) => {
                // Only a timed-out git transfer can succeed unchanged.
                let upstream = Upstream {
                    retryable: error.code == ErrorCode::Timeout,
                    ..Upstream::default()
                };
                DomainResult::provider(
                    error.code,
                    error.message,
                    error.hints,
                    error.next.map(|next| *next),
                    upstream,
                )
            }
        }
    }

    async fn execute_history_item_resolved(
        &self,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        security: &ContentSecurity,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        let raw_query = query;
        let mut query = match gh_get_history_item::HistoryItemRequest::from_row(query) {
            Ok(query) => query,
            Err(error) => return Ok(super::dispatch::invalid_query(&error)),
        };
        // Patch windows fill this row's share of the response page.
        query.auto_page_chars = Some(context.row_page_chars(self.auto_page_chars));
        let result = gh_get_history_item::execute(
            &self.provider.transport,
            &query,
            request_context,
            security,
        )
        .await;
        context.check()?;
        Ok(match result {
            Ok(output) => DomainResult {
                remaining: output.remaining,
                ..super::dispatch::value_result(output.data)
            },
            Err(mut error) => {
                let mut next = gh_get_history_item::recovery(error.kind, error.reason, raw_query);
                if error.reason == Some(ProviderErrorReason::RefNotFound)
                    && let Some((message, retry)) =
                        self.missing_history_ref(raw_query, request_context).await
                {
                    error.message = retry
                        .message(message.as_deref().unwrap_or(&error.message))
                        .into_boxed_str();
                    let (name, lead) = retry.lead();
                    next.get_or_insert_with(|| serde_json::json!({}))[name] = lead;
                }
                history_error(error, false, next)
            }
        })
    }

    /// A history item read at a ref that is not there, named against the
    /// default branch: a commit's `ref`, or the comparison sides GitHub's one
    /// not-found answer leaves unnamed (probed, so the message says which).
    /// The retry sets each missing ref to the default branch; a comparison
    /// whose other side already is the default branch has no useful retry.
    /// `Some(message)` replaces the provider's message before the default
    /// branch is named.
    async fn missing_history_ref(
        &self,
        query: &Value,
        context: &RequestContext,
    ) -> Option<(Option<String>, crate::tools::gh_shared::DefaultBranchRetry)> {
        let text = |name: &str| query.get(name).and_then(Value::as_str).unwrap_or_default();
        let retry = async |fields: &[&str]| {
            crate::tools::gh_shared::default_branch_retry(
                &self.provider,
                crate::id::ToolId::GhGetHistoryItem,
                query.clone(),
                fields,
                context,
            )
            .await
        };
        match text("operation") {
            "commit" => Some((None, retry(&["ref"]).await?)),
            "compare" => {
                let (owner, repo) = (text("owner"), text("repo"));
                let sides = [("base", text("base")), ("head", text("head"))];
                let missing = crate::tools::gh_shared::unresolved_refs(
                    &self.provider,
                    owner,
                    repo,
                    &sides,
                    context,
                )
                .await;
                if missing.is_empty() {
                    return None;
                }
                let retry = retry(&missing).await?;
                // The retry must still compare two refs: a side that
                // resolved and already is the default branch leaves none.
                if sides.iter().all(|(field, reference)| {
                    missing.contains(field) || *reference == retry.branch()
                }) {
                    return None;
                }
                let named = missing
                    .iter()
                    .map(|field| format!("\"{}\" ({field})", text(field)))
                    .collect::<Vec<_>>()
                    .join(" and ");
                let message = format!(
                    "Cannot compare {}...{} in {owner}/{repo}: {named} not found",
                    text("base"),
                    text("head")
                );
                Some((Some(message), retry))
            }
            _ => None,
        }
    }

    async fn execute_search_history_resolved(
        &self,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        security: &ContentSecurity,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        let query = match super::dispatch::parse_query(query) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        let result =
            gh_search_history::execute(&self.provider, query, request_context, security).await;
        context.check()?;
        Ok(match result {
            Ok(data) => super::dispatch::value_result(data),
            Err(failure) => history_error(*failure.error, true, failure.next),
        })
    }
}

/// A GitHub tool's row: its payload, or the failure it stated plus the
/// provider's status and rate-limit metadata.
fn tool_row(result: Result<ToolData, GhFailure>) -> DomainResult {
    match result {
        Ok(output) => DomainResult {
            diagnostics: output.diagnostics,
            remaining: output.remaining,
            ..DomainResult::payload(output.data, output.status)
        },
        Err(failure) => {
            let mut row = provider_row(
                &failure.error,
                failure.message,
                failure.hints,
                failure.next.map(|next| *next),
            );
            for (field, value) in failure.fields {
                row.data[field] = value;
            }
            row
        }
    }
}

/// Release-tag checks through a row's GitHub transport (see
/// [`GitHubServices::release_tags`]).
pub(crate) struct ReleaseTagCheck<'a> {
    services: &'a GitHubServices,
    context: &'a ExecutionContext,
    access: &'a ResponseAccess,
}

impl crate::providers::artifact::ReleaseTags for ReleaseTagCheck<'_> {
    fn tag_exists<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tag: &'a str,
    ) -> crate::providers::artifact::TagFuture<'a> {
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .ok()?;
            // A tag ref, not `commits/{tag}`: a branch of the same name is
            // no release tag.
            self.services
                .provider
                .transport
                .tag_exists(owner, repo, tag, &request)
                .await
                .ok()
        })
    }

    fn commit_exists<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> crate::providers::artifact::TagFuture<'a> {
        Box::pin(async move {
            let request = self
                .services
                .resolve_request(self.context, self.access)
                .await
                .ok()?;
            match self
                .services
                .provider
                .transport
                .commit_sha(owner, repo, commit, &request)
                .await
            {
                Ok(_) => Some(true),
                Err(error) if error.reason == Some(ProviderErrorReason::RefNotFound) => Some(false),
                Err(_) => None,
            }
        })
    }
}

/// A provider failure no tool worded (services setup, clone admission):
/// the provider's own message on the shared row, nothing else.
pub(super) fn provider_error(error: ProviderError) -> DomainResult {
    provider_row(
        &error,
        error.message.to_string(),
        provider_hint(&error)
            .map(str::to_owned)
            .into_iter()
            .collect(),
        None,
    )
}

/// A history failure row; the wording comes from the history tools
/// (`search` marks ghSearchHistory), `next` is ghGetHistoryItem's recovery
/// lead.
fn history_error(error: ProviderError, search: bool, next: Option<Value>) -> DomainResult {
    let (message, hint) = gh_search_history::history_failure(&error, search);
    let hints = hint.map(str::to_owned).into_iter().collect();
    provider_row(&error, message, hints, next)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod response_access_tests {
    use super::*;

    #[test]
    fn saved_response_access_requires_one_successful_credential_identity() {
        let access = ResponseAccess::default();
        assert_eq!(access.identity(), Ok(None));
        let request = |token: &str| {
            Ok(RequestContext::new(
                RequestBudget::with_timeout(Duration::from_secs(5), 1024),
                Some(ResolvedCredential::new(token, CredentialSource::Home)),
            ))
        };
        access.observe("github.com", &request("one"));
        let first = access.identity().expect("one identity");
        assert!(first.is_some());
        access.observe("github.com", &request("one"));
        assert_eq!(access.identity(), Ok(first));
        access.observe("github.com", &request("two"));
        assert_eq!(access.identity(), Err(()));
        let failed = ResponseAccess::default();
        failed.observe(
            "github.com",
            &Err(ProviderError::new(
                ProviderErrorKind::CredentialStoreUnavailable,
                "fixture",
            )),
        );
        failed.observe("github.com", &request("one"));
        assert_eq!(failed.identity(), Err(()));
    }
}
