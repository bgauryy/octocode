//! One pooled HTTP client; credential acquisition stays inside admitted blocking work.
use super::{
    ExecutionContext, ExecutionError, FailureKind,
    dispatch::{DomainResult, Upstream},
    github_cache::GitHubContentCache,
};
use crate::{
    config::ConfigOutput,
    policy::path::PathPolicy,
    providers::github::*,
    security::ContentSecurity,
    tools::{
        gh_clone_repo::{self, CloneConfig, CloneContext, CloneFailure, SystemGit},
        gh_get_file_content, gh_get_history_item, gh_search_code, gh_search_history,
        gh_search_repo,
        gh_shared::{GITHUB_AUTH_RECOVERY_HINT, GhFailure, provider_hint},
        gh_structure,
        id::{GitHubTool, ToolId},
        local_fetch::LocalFetchRegex,
        result::ToolData,
    },
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};

const ANONYMOUS_RATE_LIMIT_HINT: &str =
    "Wait for the rate-limit reset, or run octocode auth login for a higher quota.";

fn provider_recovery_hint(kind: ProviderErrorKind) -> &'static str {
    match kind {
        ProviderErrorKind::Authentication => GITHUB_AUTH_RECOVERY_HINT,
        ProviderErrorKind::Permission => "Verify token scopes and repository access.",
        ProviderErrorKind::NotFound => "Verify owner/repo/ref and the requested identifier.",
        ProviderErrorKind::Validation => "Correct the invalid GitHub query fields.",
        ProviderErrorKind::RateLimited => {
            "Wait for Retry-After or the rate-limit reset before retrying."
        }
        ProviderErrorKind::Transport | ProviderErrorKind::Timeout | ProviderErrorKind::Server => {
            "Retry the request; if it persists, verify network and GitHub availability."
        }
        ProviderErrorKind::Cancelled => "Retry only if the operation is still needed.",
        ProviderErrorKind::ResponseTooLarge => "Narrow the requested GitHub scope.",
        ProviderErrorKind::RedirectDenied => {
            "Use the canonical allowed GitHub host and repository."
        }
        ProviderErrorKind::Decode => {
            "Retry once; report a provider response incompatibility if it persists."
        }
        ProviderErrorKind::Configuration | ProviderErrorKind::CredentialStoreUnavailable => {
            "Correct GitHub authentication and provider configuration."
        }
        ProviderErrorKind::Unavailable => {
            "GitHub blocks this resource for legal reasons; retrying will not help."
        }
        ProviderErrorKind::HttpStatus => {
            "Check httpStatus and the message; this status is not a network failure."
        }
    }
}

/// Every GitHub provider failure row: the shared provider row
/// ([`DomainResult::provider`]) with the kind's one hint when the arm
/// supplied none, plus the debug-only request id and documentation link.
fn provider_row(
    error: &ProviderError,
    message: impl Into<String>,
    mut hints: Vec<String>,
    next: Option<Value>,
    kind: FailureKind,
) -> DomainResult {
    let code = serde_json::to_value(error.kind)
        .ok()
        .and_then(|code| code.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into());
    if hints.is_empty() {
        hints.push(provider_recovery_hint(error.kind).to_owned());
    }
    let upstream = Upstream {
        retryable: error.retryable,
        http_status: error.status,
        rate_limit: error.rate_limit.as_ref().map(rate_limit_block),
    };
    let mut row = DomainResult::provider(code, message, hints, next, kind, upstream);
    if let Some(request_id) = &error.request_id {
        row.data["requestId"] = json!(request_id);
    }
    if let Some(documentation_url) = &error.documentation_url {
        row.data["documentationUrl"] = json!(documentation_url);
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

/// Authenticating raises only the primary quota, so only an anonymous
/// caller whose primary bucket is spent (`rateLimit.remaining == 0`) is told
/// to log in; a secondary limit or circuit cooldown keeps its wait hint.
fn advise_login_for_anonymous_quota(data: &mut Value, anonymous: bool) {
    if anonymous
        && data["errorCode"] == "rateLimited"
        && data.pointer("/rateLimit/remaining") == Some(&json!(0))
    {
        data["hints"] = json!([ANONYMOUS_RATE_LIMIT_HINT]);
    }
}

pub(super) struct GitHubServices {
    credentials: Authentication,
    provider: GitHubProvider<StaticCredentialResolver, GitHubContentCache>,
    timeout: Duration,
    home: PathBuf,
    /// Resolved `storage.cloneCache.*` limits for ghCloneRepo.
    clone_limits: crate::config::CloneCacheConfig,
    /// `output.pagination.defaultCharLength`: patch pages are sized to fit it.
    auto_page_chars: usize,
    /// Sanitized full views of recently paged files (scoped to this runtime's
    /// single security policy), so each `next.continue` skips a full rescan.
    sanitized_views: crate::security::scan::SanitizedViewMemo,
}

impl GitHubServices {
    pub fn new(
        config: Arc<ConfigOutput>,
        home: PathBuf,
        cache: GitHubContentCache,
    ) -> Result<Self, ProviderError> {
        let endpoint =
            GitHubEndpoint::new(url::Url::parse(&config.resolved.github.api_url).map_err(
                |_| ProviderError::new(ProviderErrorKind::Configuration, "Invalid GitHub API URL"),
            )?)?;
        let mut transport = GitHubTransport::new(
            endpoint,
            Arc::new(StaticCredentialResolver::anonymous()),
            RetryPolicy {
                max_attempts: (config.resolved.network.max_retries as u8).saturating_add(1),
                ..Default::default()
            },
        )?;
        transport.graphql_enabled = config.resolved.github.graphql_enabled;
        transport.cache = Arc::new(cache.clone());
        // Same persistence switch as the response cache: cross-process
        // rate-limit facts live under ~/.octocode/tmp/ratelimit.
        transport.set_rate_limit_state_dir(
            crate::config::is_persistent_storage_enabled(&config.resolved)
                .then(|| home.join("tmp").join("ratelimit")),
        );
        let timeout = Duration::from_secs_f64(config.resolved.network.timeout / 1000.0);
        let clone_limits = config.resolved.storage.clone_cache.clone();
        let auto_page_chars = config.resolved.output.pagination.default_char_length as usize;
        let credentials = Authentication::new(config);
        Ok(Self {
            credentials,
            provider: GitHubProvider { transport, cache },
            timeout,
            home,
            clone_limits,
            auto_page_chars,
            sanitized_views: crate::security::scan::SanitizedViewMemo::new(),
        })
    }

    pub fn execute_query(
        &self,
        tool: ToolId,
        query: &Value,
        context: &ExecutionContext,
        security: &ContentSecurity,
        regex: &LocalFetchRegex,
        handle: &tokio::runtime::Handle,
        paths: &PathPolicy,
    ) -> Result<DomainResult, ExecutionError> {
        let Some(tool) = tool.github() else {
            return Err(ExecutionError::WorkerFailed);
        };
        context.check()?;
        let request_context = self.request_context(context, handle);
        context.check()?;
        let mut result = handle.block_on(self.execute_resolved(
            tool,
            query,
            request_context.as_ref(),
            context,
            security,
            regex,
            paths,
        ))?;
        // A rejected credential is resolved again on the next row.
        if result.failure == Some(FailureKind::Authentication) {
            self.credentials
                .forget(self.provider.transport.endpoint().credential_host());
        }
        let anonymous = request_context
            .as_ref()
            .is_ok_and(|request| request.resolved_credential().is_none());
        advise_login_for_anonymous_quota(&mut result.data, anonymous);
        Ok(result)
    }

    fn request_context(
        &self,
        context: &ExecutionContext,
        handle: &tokio::runtime::Handle,
    ) -> Result<RequestContext, ProviderError> {
        handle.block_on(self.resolve_request(context))
    }

    /// The request budget and resolved credential for one row.
    async fn resolve_request(
        &self,
        context: &ExecutionContext,
    ) -> Result<RequestContext, ProviderError> {
        let host = self
            .provider
            .transport
            .endpoint()
            .credential_host()
            .to_owned();
        let mut budget = RequestContext::with_timeout(self.timeout, 16 * 1024 * 1024);
        budget.deadline = context.deadline.min(budget.deadline);
        budget.cancellation = context.cancellation.clone();
        let credential = self
            .credentials
            .resolve(&host, AuthMode::Request, &budget)
            .await?
            .map(|selection| selection.credential);
        let mut request_context = RequestContext::with_resolved_credential(
            self.timeout,
            budget.max_body_bytes,
            credential,
        );
        request_context.deadline = budget.deadline;
        request_context.cancellation = budget.cancellation;
        Ok(request_context)
    }

    /// artifactSearch's upstream release-tag checks for one row, through
    /// this transport: the configured API URL, credential and budget.
    pub(crate) fn release_tags<'a>(&'a self, context: &'a ExecutionContext) -> ReleaseTagCheck<'a> {
        ReleaseTagCheck {
            services: self,
            context,
        }
    }

    async fn execute_resolved(
        &self,
        tool: GitHubTool,
        query: &Value,
        request: Result<&RequestContext, &ProviderError>,
        context: &ExecutionContext,
        security: &ContentSecurity,
        regex: &LocalFetchRegex,
        paths: &PathPolicy,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        // Generated query types carry the meta fields, so every tool parses
        // the validated row as-is.
        macro_rules! typed {
            ($type:ty) => {
                match super::dispatch::parse_query::<$type>(query.clone()) {
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
                let security =
                    crate::security::scan::MemoizedScan::new(security, &self.sanitized_views);
                match gh_get_file_content::run(
                    &self.provider,
                    &query,
                    request,
                    Some(self.auto_page_chars),
                    &security,
                    context,
                    regex,
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
                Err(error) => history_error(error, false),
            },
            GitHubTool::GhSearchHistory => match request {
                Ok(request) => {
                    self.execute_search_history_resolved(query, request, context, security)
                        .await?
                }
                Err(error) => history_error(error, true),
            },
        };
        context.check()?;
        Ok(row)
    }

    /// ghCloneRepo runs on git alone; CloneConfig treats the octocode home
    /// as its cache root and derives the clone, lock, stage and git-home dirs.
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
        let git = SystemGit::default();
        let clone_context = CloneContext {
            config: &config,
            endpoint: self.provider.transport.endpoint(),
            credential: request.resolved_credential(),
            resolved_default_branch: None,
            cancellation: context,
            deadline: context.deadline,
            path_policy: paths,
            git: &git,
        };
        match gh_clone_repo::run(&self.provider, query, Ok(request), &clone_context).await {
            Ok(result) => {
                DomainResult::payload(serde_json::to_value(result).unwrap_or_default(), None)
            }
            Err(CloneFailure::Provider(error)) => provider_error(error),
            Err(CloneFailure::Clone(error)) => {
                let kind = super::exit::failure_kind(&error.code);
                // Only a timed-out git transfer can succeed unchanged.
                let upstream = Upstream {
                    retryable: error.code == "timeout",
                    ..Upstream::default()
                };
                DomainResult::provider(error.code, error.message, error.hints, None, kind, upstream)
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
        let mut query = match gh_get_history_item::HistoryItemRequest::from_row(query.clone()) {
            Ok(query) => query,
            Err(error) => return Ok(super::dispatch::invalid_query(&error)),
        };
        // Patch windows fill this row's share of the response page.
        query.auto_page_chars = Some(context.response_window.unwrap_or(self.auto_page_chars));
        let result = gh_get_history_item::execute(
            &self.provider.transport,
            &query,
            request_context,
            security,
        )
        .await;
        context.check()?;
        Ok(match result {
            Ok(data) => super::dispatch::value_result(data),
            Err(error) => {
                let reason = error.reason;
                let mut result = history_error(error, false);
                gh_get_history_item::attach_recovery(&mut result.data, reason, raw_query);
                result
            }
        })
    }

    async fn execute_search_history_resolved(
        &self,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        security: &ContentSecurity,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        let query = match super::dispatch::parse_query(query.clone()) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        let result =
            gh_search_history::execute(&self.provider.transport, query, request_context, security)
                .await;
        context.check()?;
        Ok(match result {
            Ok(data) => super::dispatch::value_result(data),
            Err(error) => history_error(error, true),
        })
    }
}

/// A GitHub tool's row: its payload, or the failure it stated plus the
/// provider's status and rate-limit metadata.
fn tool_row(result: Result<ToolData, GhFailure>) -> DomainResult {
    match result {
        Ok(output) => DomainResult {
            diagnostics: output.diagnostics,
            ..DomainResult::payload(output.data, output.status)
        },
        Err(failure) => {
            let kind = failure_kind(failure.error.kind);
            let mut row = provider_row(
                &failure.error,
                failure.message,
                failure.hints,
                failure.next,
                kind,
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
}

impl crate::providers::artifact::ReleaseTags for ReleaseTagCheck<'_> {
    fn exists<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        tag: &'a str,
    ) -> crate::providers::artifact::TagFuture<'a> {
        Box::pin(async move {
            let request = self.services.resolve_request(self.context).await.ok()?;
            match self
                .services
                .provider
                .transport
                .commit_sha(owner, repo, tag, &request)
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
        failure_kind(error.kind),
    )
}

fn failure_kind(kind: ProviderErrorKind) -> FailureKind {
    match kind {
        ProviderErrorKind::NotFound => FailureKind::NotFound,
        ProviderErrorKind::Authentication => FailureKind::Authentication,
        ProviderErrorKind::Permission => FailureKind::Permission,
        ProviderErrorKind::RateLimited => FailureKind::RateLimited,
        _ => FailureKind::Execution,
    }
}

/// A history failure row; the wording comes from the history tools
/// (`search` marks ghSearchHistory).
fn history_error(error: ProviderError, search: bool) -> DomainResult {
    let (message, hint) = gh_search_history::history_failure(&error, search);
    let hints = hint.map(str::to_owned).into_iter().collect();
    provider_row(&error, message, hints, None, failure_kind(error.kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search_row(error: ProviderError) -> DomainResult {
        tool_row(Err(crate::tools::gh_shared::search_failure(
            error,
            "Lower page to reach deeper results.",
        )))
    }

    fn file_row(error: ProviderError, query: Value) -> DomainResult {
        let query: gh_get_file_content::GhGetFileContentQuery =
            serde_json::from_value(query).expect("file query");
        tool_row(Err(gh_get_file_content::errors::failure(
            error, &query, None,
        )))
    }

    #[test]
    fn authentication_errors_explain_canonical_login_and_environment_precedence() {
        let error = || ProviderError {
            kind: ProviderErrorKind::Authentication,
            message: "Bad credentials".into(),
            status: Some(401),
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            retryable: false,
            reason: None,
        };
        for result in [
            search_row(error()),
            history_error(error(), true),
            history_error(error(), false),
            file_row(error(), json!({"owner":"a","repo":"b","path":"x"})),
            provider_error(error()),
        ] {
            let rendered = result.data.to_string();
            assert!(rendered.contains("octocode auth login"), "{rendered}");
            assert!(rendered.contains("GH_TOKEN"), "{rendered}");
            assert!(rendered.contains("invalid env token"), "{rendered}");
            assert!(!rendered.contains("octocode login"), "{rendered}");
        }
    }

    #[test]
    fn legal_block_is_not_rendered_as_a_network_failure() {
        let error = || ProviderError {
            kind: ProviderErrorKind::Unavailable,
            message: "GitHub resource blocked for legal reasons (HTTP 451)".into(),
            status: Some(451),
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            retryable: false,
            reason: None,
        };
        for result in [
            search_row(error()),
            history_error(error(), true),
            history_error(error(), false),
            file_row(error(), json!({"owner":"a","repo":"b","path":"x"})),
            provider_error(error()),
        ] {
            let text = result.data["error"].as_str().unwrap_or_default();
            assert!(text.contains("legal reasons"), "{}", result.data);
            assert!(result.data.get("retryable").is_none(), "{}", result.data);
            assert_eq!(result.data["errorCode"], "unavailable");
            let rendered = result.data.to_string();
            assert!(
                !rendered.contains("Network connection failed"),
                "{rendered}"
            );
            assert!(!rendered.contains("Retry the request"), "{rendered}");
            assert!(!rendered.contains("internet connection"), "{rendered}");
        }
    }

    #[test]
    fn search_validation_keeps_github_detail_and_tool_specific_window_hint() {
        let error = |message: &str, reason| ProviderError {
            kind: ProviderErrorKind::Validation,
            message: message.into(),
            status: Some(422),
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            retryable: false,
            reason,
        };
        let result = search_row(error("\"abc\" is not a numeric value", None));
        assert_eq!(
            result.data["error"],
            "Invalid search query or request parameters: \"abc\" is not a numeric value"
        );
        let generic = search_row(error("Validation Failed", None));
        assert_eq!(
            generic.data["error"],
            "Invalid search query or request parameters"
        );
        // Each search tool names its own deeper-result filters.
        let window = Some(ProviderErrorReason::SearchWindowExceeded);
        let paged = search_row(error("window", window));
        assert_eq!(
            paged.data["hints"][0], "Lower page to reach deeper results.",
            "{}",
            paged.data
        );
    }

    #[test]
    fn permission_errors_keep_the_provider_reason_across_github_tools() {
        let reason = "Resource protected by organization SAML SSO authorization";
        let error = || ProviderError {
            kind: ProviderErrorKind::Permission,
            message: reason.into(),
            status: Some(403),
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            retryable: false,
            reason: None,
        };
        for result in [
            search_row(error()),
            history_error(error(), true),
            history_error(error(), false),
            file_row(error(), json!({"owner":"a","repo":"b","path":"src/lib.rs"})),
        ] {
            assert_eq!(result.data["error"], reason);
            assert_eq!(result.data["httpStatus"], 403);
            assert_eq!(result.failure, Some(FailureKind::Permission));
        }
    }

    /// Regression: a provider failure whose body decoded into a structured
    /// error (here modelled as a Validation 422 carrying rate-limit metadata)
    /// must still shape `data.error` as a plain string. Before the fix
    /// `history_error` nested the diagnostic object under `error`, producing
    /// `results.0.data.error: Expected string` (`outputContractViolation`) and
    /// masking the real 422.
    #[test]
    fn history_error_shapes_error_as_string() {
        let error = ProviderError {
            kind: ProviderErrorKind::Validation,
            message: "Validation Failed: repository rename not followed".into(),
            status: Some(422),
            request_id: None,
            documentation_url: None,
            rate_limit: Some(RateLimit {
                remaining: Some(11),
                reset_epoch_seconds: Some(1_700_000_000),
                retry_after_seconds: None,
                resource: None,
            }),
            retryable: false,
            reason: None,
        };

        let result = history_error(error, true);
        let data = &result.data;

        assert_eq!(result.status, Some("error"));
        // The contract requires a string here; a nested object regresses it.
        assert!(
            data["error"].is_string(),
            "data.error must be a string, got {}",
            data["error"]
        );
        // GitHub's own 422 detail names the bad input.
        assert_eq!(
            data["error"].as_str(),
            Some("Invalid search query or request parameters: repository rename not followed"),
        );
        // One row shape: each fact once, no `status`/`type`/
        // `rateLimitRemaining`/`scopesSuggestion` siblings.
        assert_eq!(
            data,
            &json!({
                "error": "Invalid search query or request parameters: repository rename not followed",
                "errorCode": "invalidInput",
                "httpStatus": 422,
                "rateLimit": {"remaining": 11, "resetEpochSeconds": 1_700_000_000_u64},
                "hints": ["Correct the invalid GitHub query fields."]
            })
        );
    }

    /// A ghGetFileContent request with an explicit ref that GitHub rejects
    /// ("No commit found for the ref …", canned 404 here) must name the
    /// missing branch/tag/SHA and hint at checking the ref instead of the
    /// generic "Invalid search query or request parameters" message.
    #[test]
    fn file_error_names_the_missing_ref_for_explicit_branches() {
        let error = ProviderError {
            kind: ProviderErrorKind::NotFound,
            message: "No commit found for the ref no-such-branch".into(),
            status: Some(404),
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            retryable: false,
            reason: None,
        };
        let query = json!({
            "owner": "a",
            "repo": "b",
            "path": "src/lib.rs",
            "ref": "no-such-branch"
        });

        let result = file_row(error, query.clone());
        let data = &result.data;

        assert_eq!(result.status, Some("error"));
        assert_eq!(result.failure, Some(FailureKind::NotFound));
        assert_eq!(data["errorCode"], json!("notFound"));
        assert_eq!(data["httpStatus"], json!(404));
        assert!(data.get("retryable").is_none(), "{data}");
        assert_eq!(
            data["error"].as_str(),
            Some("Branch, tag, or SHA not found for a/b: \"no-such-branch\"")
        );
        // The error names the ref; the hint leads to the ref listing.
        let hint = data["hints"][0].as_str().expect("ref hint");
        assert!(hint.contains("viewStructure"), "{hint}");

        // The commits-endpoint flavor (422 Validation) maps the same way.
        let error = ProviderError {
            kind: ProviderErrorKind::Validation,
            message: "No commit found for SHA: no-such-branch".into(),
            status: Some(422),
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            retryable: false,
            reason: None,
        };
        let result = file_row(error, query.clone());
        assert_eq!(
            result.data["error"].as_str(),
            Some("Branch, tag, or SHA not found for a/b: \"no-such-branch\"")
        );

        // Without an explicit ref the existing not-found shaping is unchanged.
        let error = ProviderError {
            kind: ProviderErrorKind::NotFound,
            message: "Not Found".into(),
            status: Some(404),
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            retryable: false,
            reason: None,
        };
        let result = file_row(error, json!({"owner":"a","repo":"b","path":"src/lib.rs"}));
        assert_eq!(
            result.data["error"].as_str(),
            Some("Repository, resource, or path not found")
        );
    }

    #[test]
    fn file_error_directory_and_binary_get_accurate_recovery() {
        let query = json!({"owner":"a","repo":"b","path":"src","ref":"main"});
        let error = ProviderError::new(
            ProviderErrorKind::Validation,
            "Path \"src\" is a directory, not a file; list it with ghStructure.",
        )
        .with_reason(ProviderErrorReason::PathIsDirectory);
        let result = file_row(error, query.clone());
        let data = &result.data;
        assert!(
            data["error"]
                .as_str()
                .unwrap_or_default()
                .contains("is a directory"),
            "{data}"
        );
        assert_eq!(
            data["next"]["viewTree"]["query"]["queries"][0]["path"],
            "src"
        );
        assert_eq!(
            data["next"]["viewTree"]["query"]["queries"][0]["ref"],
            "main"
        );

        let error = ProviderError::new(
            ProviderErrorKind::Validation,
            "Binary file (1234 bytes, blob abc); ghGetFileContent returns text only.",
        )
        .with_reason(ProviderErrorReason::BinaryFile);
        let result = file_row(error, json!({"owner":"a","repo":"b","path":"x.png"}));
        let hint = result.data["hints"][0].as_str().unwrap_or_default();
        assert!(!hint.contains("Retry once"), "{hint}");
        assert!(hint.contains("Binary"), "{hint}");
        // D7: the caller's request cannot be served (exit 2), with the
        // file's size and blob SHA, never a decode/execution failure.
        assert_eq!(result.data["errorCode"], "invalidInput", "{}", result.data);
        let message = result.data["error"].as_str().unwrap_or_default();
        assert!(
            message.contains("1234 bytes") && message.contains("blob abc"),
            "{message}"
        );
        assert!(result.data.get("next").is_none(), "{}", result.data);
    }

    /// Recovery keys on the typed reason: the same text without the reason
    /// gets no directory continuation.
    #[test]
    fn directory_recovery_keys_on_the_typed_reason_not_the_message() {
        let query = json!({"owner":"a","repo":"b","path":"src","ref":"main"});
        let untyped = ProviderError::new(ProviderErrorKind::Validation, "src is a directory");
        assert!(
            file_row(untyped, query.clone()).data["next"]
                .get("viewTree")
                .is_none()
        );
    }

    fn rate_limited(remaining: Option<u64>, reset: Option<u64>, resource: &str) -> ProviderError {
        let mut error = ProviderError::new(ProviderErrorKind::RateLimited, "slow down");
        error.status = Some(403);
        error.retryable = true;
        error.rate_limit = Some(RateLimit {
            remaining,
            reset_epoch_seconds: reset,
            retry_after_seconds: Some(60),
            resource: Some(resource.into()),
        });
        error
    }

    /// E6/D3: a rate-limited row states each fact once, in seconds, with
    /// unknown members absent, the same on every GitHub tool.
    #[test]
    fn rate_limit_rows_have_one_shape_on_every_github_tool() {
        let primary = rate_limited(Some(0), Some(1_700_000_000), "core");
        let secondary = rate_limited(None, None, "search");
        let hint = "Wait for Retry-After or the rate-limit reset before retrying.";
        for (error, rate_limit) in [
            (
                primary,
                json!({"resource": "core", "remaining": 0, "resetEpochSeconds": 1_700_000_000_u64, "retryAfterSeconds": 60}),
            ),
            (
                secondary,
                json!({"resource": "search", "retryAfterSeconds": 60}),
            ),
        ] {
            let expected = json!({
                "error": "slow down",
                "errorCode": "rateLimited",
                "retryable": true,
                "httpStatus": 403,
                "rateLimit": rate_limit,
                "hints": [hint]
            });
            for result in [
                search_row(error.clone()),
                history_error(error.clone(), true),
                history_error(error.clone(), false),
                provider_error(error.clone()),
            ] {
                assert_eq!(result.data, expected);
                let keys: Vec<&str> = result
                    .data
                    .as_object()
                    .expect("row")
                    .keys()
                    .map(String::as_str)
                    .collect();
                assert_eq!(
                    keys,
                    [
                        "error",
                        "errorCode",
                        "retryable",
                        "httpStatus",
                        "rateLimit",
                        "hints"
                    ]
                );
                assert_eq!(result.failure, Some(FailureKind::RateLimited));
            }
            // A file read adds only the file's identity.
            let file = file_row(error.clone(), json!({"owner":"a","repo":"b","path":"x"}));
            let mut with_identity = expected.clone();
            with_identity["owner"] = json!("a");
            with_identity["repo"] = json!("b");
            with_identity["path"] = json!("x");
            assert_eq!(file.data, with_identity);
        }
    }

    /// Only an anonymous caller whose primary quota is spent is told that
    /// logging in helps; a secondary limit or an authenticated caller keeps
    /// the wait hint.
    #[test]
    fn login_advice_needs_an_anonymous_caller_and_a_spent_primary_quota() {
        let advised = |error: ProviderError, anonymous: bool| {
            let mut data = history_error(error, true).data;
            advise_login_for_anonymous_quota(&mut data, anonymous);
            data["hints"][0]
                .as_str()
                .unwrap_or_default()
                .contains("auth login")
        };
        let primary = || rate_limited(Some(0), Some(1_700_000_000), "core");
        let secondary = || rate_limited(None, None, "search");
        let secondary_with_quota = || rate_limited(Some(21), Some(1_700_000_000), "search");
        assert!(advised(primary(), true));
        assert!(!advised(primary(), false));
        assert!(!advised(secondary(), true));
        assert!(!advised(secondary_with_quota(), true));
    }

    /// N7: a provider/transport row states `retryable:true` when a retry can
    /// help and omits the key otherwise (absence = do not retry unchanged).
    #[test]
    fn provider_rows_state_retryable_only_when_true() {
        for kind in [
            ProviderErrorKind::Transport,
            ProviderErrorKind::Timeout,
            ProviderErrorKind::Server,
            ProviderErrorKind::NotFound,
            ProviderErrorKind::Configuration,
        ] {
            let error = ProviderError::new(kind, "failed");
            let expected = error.retryable;
            for result in [
                search_row(error.clone()),
                history_error(error.clone(), false),
                file_row(error.clone(), json!({"owner":"a","repo":"b","path":"x"})),
                provider_error(error.clone()),
            ] {
                assert_eq!(
                    result.data.get("retryable"),
                    expected.then_some(&json!(true)),
                    "{kind:?}: {}",
                    result.data
                );
                assert!(result.data.get("provider").is_none(), "{}", result.data);
            }
        }
    }

    /// ghGetHistoryItem is not a search endpoint: a bogus commit SHA (GitHub
    /// 422 "No commit found for SHA: …") must produce a commit-not-found
    /// message, not a search-syntax one.
    #[test]
    fn history_item_bogus_sha_is_commit_not_found_without_search_suggestion() {
        let error = ProviderError {
            kind: ProviderErrorKind::Validation,
            message: "No commit found for SHA: deadbeef1234567890".into(),
            status: Some(422),
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            retryable: false,
            reason: None,
        };

        let result = history_error(error, false);
        let data = &result.data;

        assert_eq!(result.status, Some("error"));
        assert_eq!(
            data["error"].as_str(),
            Some("Commit not found - verify the ref/SHA exists in this repository")
        );
        assert_eq!(
            data["hints"],
            json!(["Correct the invalid GitHub query fields."]),
            "non-search operations must not suggest checking search syntax: {data}"
        );

        // The search-history tool names the query, with no extra field.
        let error = ProviderError {
            kind: ProviderErrorKind::Validation,
            message: "Validation Failed".into(),
            status: Some(422),
            request_id: None,
            documentation_url: None,
            rate_limit: None,
            retryable: false,
            reason: None,
        };
        let result = history_error(error, true);
        assert_eq!(
            result.data["error"],
            "Invalid search query or request parameters"
        );
        assert!(
            result.data.get("scopesSuggestion").is_none(),
            "{}",
            result.data
        );
    }
}
