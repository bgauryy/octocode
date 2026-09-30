//! One pooled HTTP client; credential acquisition stays inside admitted blocking work.
use super::{
    ExecutionContext, ExecutionError, FailureKind, dispatch::DomainResult,
    github_cache::GitHubContentCache,
};
use crate::{
    config::ConfigOutput,
    policy::path::PathPolicy,
    providers::github::*,
    security::ContentSecurity,
    tools::{
        gh_clone_repo::{self, CloneConfig, CloneContext, SystemGit},
        gh_get_file_content, gh_get_history_item, gh_search, gh_search_history,
        id::ToolId,
        local_fetch::LocalFetchRegex,
    },
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};

/// Hints are rendered whole only up to the response guidance limit (120
/// chars); keep every GitHub recovery hint within it.
const GITHUB_AUTH_RECOVERY_HINT: &str = "Run octocode auth login or set OCTOCODE_TOKEN/GH_TOKEN/GITHUB_TOKEN; an invalid env token overrides stored login.";

/// A repository that did not resolve: GitHub answers a private repository
/// the token cannot see exactly like a missing one.
const REPOSITORY_ACCESS_HINT: &str = "The repository is missing, private, or hidden from this token; check owner/repo spelling and token access.";

fn repository_not_found_message(owner: &str, repo: &str) -> String {
    format!("Repository {owner}/{repo} not found, or private and not accessible to this token")
}

fn repository_not_found(error: &ProviderError) -> bool {
    error.reason == Some(ProviderErrorReason::RepositoryNotFound)
}

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

/// Every GitHub provider failure row: the shared error row plus the
/// provider's retry/status/rate-limit metadata and a kind-specific hint when
/// the arm supplied none.
fn provider_row(
    error: &ProviderError,
    message: impl Into<String>,
    hints: Vec<String>,
    next: Option<Value>,
    kind: FailureKind,
) -> DomainResult {
    let code = serde_json::to_value(error.kind)
        .ok()
        .and_then(|code| code.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into());
    let mut row = DomainResult::failure(code, message, hints, next, kind);
    apply_provider_error_metadata(&mut row.data, error);
    row
}

fn apply_provider_error_metadata(data: &mut Value, error: &ProviderError) {
    data["retryable"] = json!(error.retryable);
    if let Some(status) = error.status {
        data["httpStatus"] = json!(status);
    }
    if let Some(request_id) = &error.request_id {
        data["requestId"] = json!(request_id);
    }
    if let Some(documentation_url) = &error.documentation_url {
        data["documentationUrl"] = json!(documentation_url);
    }
    if let Some(rate_limit) = &error.rate_limit {
        // Public output is camelCase; the provider struct stays snake_case.
        data["rateLimit"] = json!({
            "remaining": rate_limit.remaining,
            "resetEpochSeconds": rate_limit.reset_epoch_seconds,
            "retryAfterSeconds": rate_limit.retry_after_seconds,
        });
        if let Some(resource) = &rate_limit.resource {
            data["rateLimit"]["resource"] = json!(resource);
        }
        if let Some(retry_after) = rate_limit.retry_after_seconds {
            data["retryAfterSeconds"] = json!(retry_after);
        }
    }
    if data.get("hints").is_none() {
        data["hints"] = json!([provider_recovery_hint(error.kind)]);
    }
}

pub(super) struct GitHubServices {
    credentials: Authentication,
    provider: GitHubProvider<StaticCredentialResolver, GitHubContentCache>,
    timeout: Duration,
    home: PathBuf,
    /// Resolved `cloneCache.*` limits for ghCloneRepo.
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
        // Same persistence switch as the response cache: cross-process
        // rate-limit facts live under ~/.octocode/tmp/ratelimit.
        transport.set_rate_limit_state_dir(
            crate::config::is_persistent_storage_enabled(&config.resolved)
                .then(|| home.join("tmp").join("ratelimit")),
        );
        let timeout = Duration::from_secs_f64(config.resolved.network.timeout / 1000.0);
        let clone_limits = config.resolved.clone_cache.clone();
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

    #[allow(clippy::too_many_arguments)]
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
        context.check()?;
        let request_context = match self.request_context(context, handle) {
            Ok(value) => value,
            Err(error) => {
                return Ok(match tool {
                    ToolId::GhGetFileContent => file_error(error, query),
                    ToolId::GhSearchRepo | ToolId::GhSearchCode | ToolId::GhStructure => {
                        search_error(tool, error)
                    }
                    ToolId::GhSearchHistory => history_error(error, true),
                    ToolId::GhGetHistoryItem => history_error(error, false),
                    ToolId::GhCloneRepo
                    | ToolId::ArtifactSearch
                    | ToolId::LocalSearch
                    | ToolId::LocalFetch
                    | ToolId::StructureSearch
                    | ToolId::AstSearch
                    | ToolId::AstTopology
                    | ToolId::AstRewrite
                    | ToolId::LspSearch
                    | ToolId::Clasify => provider_error(error),
                });
            }
        };
        context.check()?;
        handle.block_on(self.execute_resolved(
            tool,
            query,
            &request_context,
            context,
            security,
            regex,
            paths,
        ))
    }

    fn request_context(
        &self,
        context: &ExecutionContext,
        handle: &tokio::runtime::Handle,
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
        let credential = handle
            .block_on(self.credentials.resolve(&host, AuthMode::Request, &budget))?
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

    #[allow(clippy::too_many_arguments)]
    async fn execute_resolved(
        &self,
        tool: ToolId,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        security: &ContentSecurity,
        regex: &LocalFetchRegex,
        paths: &PathPolicy,
    ) -> Result<DomainResult, ExecutionError> {
        // Generated query types carry the meta fields, so every tool parses
        // the validated row as-is.
        match tool {
            ToolId::GhGetFileContent => {
                self.execute_file_resolved(query, request_context, context, security, regex)
                    .await
            }
            ToolId::GhGetHistoryItem => {
                self.execute_history_item_resolved(query, request_context, context, security)
                    .await
            }
            ToolId::GhSearchRepo | ToolId::GhSearchCode | ToolId::GhStructure => {
                self.execute_search_resolved(tool, query, request_context, context, security)
                    .await
            }
            ToolId::GhSearchHistory => {
                self.execute_search_history_resolved(query, request_context, context, security)
                    .await
            }
            ToolId::GhCloneRepo => {
                self.execute_clone_resolved(query, request_context, context, paths)
                    .await
            }
            ToolId::ArtifactSearch
            | ToolId::LocalSearch
            | ToolId::LocalFetch
            | ToolId::StructureSearch
            | ToolId::AstSearch
            | ToolId::AstTopology
            | ToolId::AstRewrite
            | ToolId::LspSearch
            | ToolId::Clasify => Err(ExecutionError::WorkerFailed),
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
        query.auto_page_chars = Some(self.auto_page_chars);
        // History items are mutable; bypass ConditionalCache intentionally.
        // See gh_get_history_item module-level doc for the full rationale.
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
                match reason {
                    Some(ProviderErrorReason::IssueIsPullRequest) => {
                        attach_pull_request_recovery(&mut result.data, raw_query);
                    }
                    Some(ProviderErrorReason::PullRequestIsIssue) => {
                        attach_issue_recovery(&mut result.data, raw_query);
                    }
                    _ => {}
                }
                result
            }
        })
    }

    async fn execute_search_resolved(
        &self,
        tool: ToolId,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        security: &ContentSecurity,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        macro_rules! parsed {
            ($type:ty) => {
                match super::dispatch::parse_query::<$type>(query.clone()) {
                    Ok(query) => query,
                    Err(row) => return Ok(*row),
                }
            };
        }
        let result = match tool {
            ToolId::GhSearchCode => {
                let query = parsed!(gh_search::GhSearchCodeQuery);
                gh_search::execute_code(&self.provider, &query, request_context, security).await
            }
            ToolId::GhSearchRepo => {
                let query = parsed!(gh_search::GhSearchRepoQuery);
                gh_search::execute_repositories(&self.provider, &query, request_context).await
            }
            ToolId::GhStructure => {
                let query = parsed!(gh_search::GhStructureQuery);
                gh_search::execute_structure(&self.provider, &query, request_context, &self.home)
                    .await
            }
            ToolId::GhGetFileContent
            | ToolId::GhSearchHistory
            | ToolId::GhGetHistoryItem
            | ToolId::GhCloneRepo
            | ToolId::ArtifactSearch
            | ToolId::LocalSearch
            | ToolId::LocalFetch
            | ToolId::StructureSearch
            | ToolId::AstSearch
            | ToolId::AstTopology
            | ToolId::AstRewrite
            | ToolId::LspSearch
            | ToolId::Clasify => return Err(ExecutionError::WorkerFailed),
        };
        context.check()?;
        Ok(match result {
            Ok(output) => DomainResult {
                diagnostics: output.diagnostics,
                ..DomainResult::payload(output.data, output.status)
            },
            Err(error) => search_error(tool, error),
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
        let query: gh_search_history::GhSearchHistoryQuery =
            match super::dispatch::parse_query(query.clone()) {
                Ok(query) => query,
                Err(row) => return Ok(*row),
            };
        // History search results are mutable; bypass ConditionalCache intentionally.
        // See gh_search_history module-level doc for the full rationale.
        let result =
            gh_search_history::execute(&self.provider.transport, &query, request_context, security)
                .await;
        context.check()?;
        Ok(match result {
            Ok(data) => super::dispatch::value_result(data),
            Err(error) => history_error(error, true),
        })
    }

    async fn execute_clone_resolved(
        &self,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        paths: &PathPolicy,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        let query: gh_clone_repo::GhCloneRepoQuery =
            match super::dispatch::parse_query(query.clone()) {
                Ok(query) => query,
                Err(row) => return Ok(*row),
            };
        // Validate owner/repo/sparse-path BEFORE any network call. Otherwise a
        // traversal-shaped owner (e.g. "../x") reaches repository_metadata and
        // is reported as repositoryNotFound (echoing raw input) instead of the
        // correct clone.input.invalid.
        if let Err(error) = gh_clone_repo::validate_query(&query) {
            return Ok(DomainResult::failure(
                error.code,
                error.message,
                error.hints,
                None,
                FailureKind::Execution,
            ));
        }
        let metadata = if query.branch.is_none() {
            match self
                .provider
                .transport
                .repository_metadata(&query.owner, &query.repo, request_context)
                .await
            {
                Ok(value) => Some(value),
                // A missing repository must be reported as such; proceeding
                // without metadata would surface the internal-sounding
                // clone.defaultBranchUnavailable failure instead.
                Err(error) if error.kind == ProviderErrorKind::NotFound => {
                    let error = gh_clone_repo::repository_not_found(&query);
                    return Ok(DomainResult::failure(
                        error.code,
                        error.message,
                        error.hints,
                        None,
                        FailureKind::NotFound,
                    ));
                }
                Err(error) => return Ok(provider_error(error)),
            }
        } else {
            None
        };
        let default_branch = metadata.as_ref().map(|value| value.default_branch.as_str());
        // CloneConfig treats cache_home as the octocode home and derives
        // tmp/clone, tmp/clone-locks, tmp/clone-tmp, and tmp/git-home itself.
        let config = CloneConfig::persistent(self.home.clone()).with_limits(&self.clone_limits);
        let git = SystemGit::default();
        let clone_context = CloneContext {
            config: &config,
            endpoint: self.provider.transport.endpoint(),
            credential: request_context.resolved_credential(),
            resolved_default_branch: default_branch.or(query.branch.as_deref()),
            cancellation: context,
            deadline: context.deadline,
            path_policy: paths,
            git: &git,
        };
        match gh_clone_repo::execute_clone(&query, &clone_context) {
            Ok(result) => Ok(DomainResult::payload(
                serde_json::to_value(result).map_err(|_| ExecutionError::WorkerFailed)?,
                None,
            )),
            Err(error) => Ok(DomainResult::failure(
                error.code,
                error.message,
                error.hints,
                None,
                FailureKind::Execution,
            )),
        }
    }

    async fn execute_file_resolved(
        &self,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        security: &ContentSecurity,
        regex: &LocalFetchRegex,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        let query: gh_get_file_content::GhGetFileContentQuery =
            match super::dispatch::parse_query(query.clone()) {
                Ok(query) => query,
                Err(row) => return Ok(*row),
            };
        let result = gh_get_file_content::execute(
            &self.provider,
            &query,
            request_context,
            None,
            &crate::security::scan::MemoizedScan::new(security, &self.sanitized_views),
            context,
            regex,
        )
        .await;
        context.check()?;
        match result {
            Ok(result) => {
                let status = if result
                    .files
                    .iter()
                    .all(|file| file.content.status == "error")
                {
                    Some("error")
                } else if result
                    .files
                    .iter()
                    .all(|file| file.content.status == "empty")
                {
                    Some("empty")
                } else {
                    None
                };
                let cache =
                    !result.files.is_empty() && result.files.iter().all(|file| file.from_cache);
                let mut data =
                    serde_json::to_value(result).map_err(|_| ExecutionError::WorkerFailed)?;
                if status == Some("empty") && !super::response::is_partial(&data) {
                    data["hints"] =
                        json!(["Verify owner/repo/branch/path, or remove matchString."]);
                }
                for file in data
                    .get_mut("files")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    // A caller-supplied full SHA is not restated.
                    if file["commitSha"].as_str() == query.branch.as_deref()
                        && let Some(map) = file.as_object_mut()
                    {
                        map.remove("commitSha");
                    }
                }
                Ok(DomainResult {
                    cache,
                    ..DomainResult::payload(data, status)
                })
            }
            Err(error) => Ok(file_error(
                error,
                &serde_json::to_value(query).map_err(|_| ExecutionError::WorkerFailed)?,
            )),
        }
    }
}

fn file_error(error: ProviderError, query: &Value) -> DomainResult {
    let owner = query["owner"].as_str().unwrap_or_default();
    let repo = query["repo"].as_str().unwrap_or_default();
    // GitHub reports an unknown ref as "No commit found for SHA: <ref>" (422 on
    // the commits endpoint used for ref resolution) or "No commit found for
    // the ref <ref>" (404 on the contents endpoint). Both mean the requested
    // branch/tag/SHA does not exist — not a malformed query — so name the ref
    // instead of the generic validation message.
    if let Some(reference) = query["branch"].as_str().filter(|value| !value.is_empty())
        && error.message.starts_with("No commit found")
    {
        let mut row = provider_row(
            &error,
            format!("Branch, tag, or SHA not found for {owner}/{repo}: \"{reference}\""),
            vec![format!(
                "Verify the ref \"{reference}\" exists (branch, tag, or full commit SHA), or omit branch to use the default branch."
            )],
            None,
            FailureKind::NotFound,
        );
        attach_file_identity(&mut row.data, owner, repo, query);
        return row;
    }
    let message = match error.kind {
        ProviderErrorKind::Authentication => "GitHub authentication required".into(),
        ProviderErrorKind::Permission => error.message.to_string(),
        ProviderErrorKind::NotFound if repository_not_found(&error) => {
            repository_not_found_message(owner, repo)
        }
        ProviderErrorKind::NotFound => "Repository, resource, or path not found".into(),
        // Provider-local validation (no HTTP status) carries a specific,
        // actionable message (directory/symlink/submodule path, bad name).
        ProviderErrorKind::Validation if error.status.is_none() => error.message.to_string(),
        ProviderErrorKind::Validation => "Invalid search query or request parameters".into(),
        ProviderErrorKind::Server if matches!(error.status, Some(502..=504)) => {
            "GitHub API temporarily unavailable".into()
        }
        ProviderErrorKind::Transport => "Network connection failed".into(),
        ProviderErrorKind::Timeout => "Request timeout".into(),
        _ if error.message.as_ref() == BINARY_FILE_MESSAGE => {
            "Binary file detected. Cannot display as text - download directly from GitHub".into()
        }
        _ => error.message.to_string(),
    };
    let requested = query["path"].as_str().unwrap_or_default();
    let (hints, next): (Vec<String>, Option<Value>) = if error.message.as_ref()
        == BINARY_FILE_MESSAGE
    {
        (
            vec!["Binary content cannot be returned as text; retrying will not help. Use ghCloneRepo for a local copy.".into()],
            None,
        )
    } else if error.kind == ProviderErrorKind::Validation
        && error.status.is_none()
        && error.reason == Some(ProviderErrorReason::PathIsDirectory)
    {
        let mut tree = tree_recovery(owner, repo, requested, query);
        // The provider confirmed this path is a directory: listing it is exact.
        tree["confidence"] = json!("exact");
        (
            vec![
                "The path is a directory; list its entries with the viewTree continuation.".into(),
            ],
            Some(json!({ "viewTree": tree })),
        )
    } else if repository_not_found(&error) {
        // Listing a tree of the same repository cannot recover.
        (vec![REPOSITORY_ACCESS_HINT.into()], None)
    } else if error.kind == ProviderErrorKind::NotFound {
        let parent = std::path::Path::new(requested)
            .parent()
            .map(|path| path.to_string_lossy().into_owned())
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| ".".into());
        (
            vec!["Check the path's exact case (no leading slash) and the branch; list the parent directory with next.viewTree.".into()],
            Some(json!({ "viewTree": tree_recovery(owner, repo, &parent, query) })),
        )
    } else if error.kind == ProviderErrorKind::Authentication {
        (vec![GITHUB_AUTH_RECOVERY_HINT.into()], None)
    } else {
        (Vec::new(), None)
    };
    let mut row = provider_row(&error, message, hints, next, failure_kind(error.kind));
    attach_file_identity(&mut row.data, owner, repo, query);
    row
}

/// ghGetFileContent error rows name the file they could not read.
fn attach_file_identity(data: &mut Value, owner: &str, repo: &str, query: &Value) {
    data["owner"] = json!(owner);
    data["repo"] = json!(repo);
    data["path"] = query["path"].clone();
}

const BINARY_FILE_MESSAGE: &str = "binary files are not supported";

/// An issue number that GitHub reports as a pull request: rerun the same read
/// as operation:"pullRequest". Issue content selections (body, discussion
/// comments) are a subset of the pull-request ones, so they carry over.
fn attach_pull_request_recovery(data: &mut Value, query: &Value) {
    let mut next = serde_json::Map::new();
    for field in ["owner", "repo", "number", "content"] {
        if let Some(value) = query.get(field).filter(|value| !value.is_null()) {
            next.insert(field.into(), value.clone());
        }
    }
    next.insert("operation".into(), json!("pullRequest"));
    data["hints"] = json!(["This number is a pull request; run the readPullRequest continuation."]);
    data["next"] = json!({"readPullRequest": {
        "tool": "ghGetHistoryItem",
        "confidence": "exact",
        "query": next,
    }});
}

/// A pull-request number that is an issue: read it as operation:"issue".
/// Only the issue's own selections (body, discussion comments) carry over.
fn attach_issue_recovery(data: &mut Value, query: &Value) {
    let mut next = serde_json::Map::new();
    for field in ["owner", "repo", "number"] {
        if let Some(value) = query.get(field).filter(|value| !value.is_null()) {
            next.insert(field.into(), value.clone());
        }
    }
    next.insert("operation".into(), json!("issue"));
    data["hints"] = json!(["This number is an issue; run the readIssue continuation."]);
    data["next"] = json!({"readIssue": {
        "tool": "ghGetHistoryItem",
        "confidence": "exact",
        "query": next,
    }});
}

/// Advisory ghStructure query for ghGetFileContent recovery. The output
/// contract validates it against the ghStructure continuation schema, which
/// requires the paginated defaulted fields: stamp the contract defaults (fresh
/// page 1 — this starts a new bounded query, not a next-page of the fetch).
fn tree_recovery(owner: &str, repo: &str, path: &str, query: &Value) -> Value {
    let mut tree = json!({
        "tool": "ghStructure",
        "query": {
            "owner": owner,
            "repo": repo,
            "path": path,
            "page": 1,
            "pageSize": 100,
            "debug": false
        },
        "confidence": "low"
    });
    if let Some(branch) = query["branch"].as_str() {
        tree["query"]["branch"] = json!(branch);
    }

    tree
}

pub(super) fn provider_error(error: ProviderError) -> DomainResult {
    let mut row = provider_row(
        &error,
        error.message.to_string(),
        Vec::new(),
        None,
        failure_kind(error.kind),
    );
    row.data["provider"] = json!(error);
    row
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

/// Error formatter for ghSearchRepo, ghSearchCode, and ghStructure. Like
/// `history_error`, the output contract requires `data.error` to be a plain
/// string; this variant carries a search-specific message/hint set, so keep it
/// separate.
fn search_error(tool: ToolId, error: ProviderError) -> DomainResult {
    let message = match error.kind {
        ProviderErrorKind::Authentication => "GitHub authentication required".to_owned(),
        ProviderErrorKind::Permission => error.message.to_string(),
        ProviderErrorKind::NotFound if repository_not_found(&error) => {
            "Repository not found, or private and not accessible to this token".to_owned()
        }
        // A typed missing ref names the ref it could not resolve.
        ProviderErrorKind::NotFound if error.reason == Some(ProviderErrorReason::RefNotFound) => {
            error.message.to_string()
        }
        ProviderErrorKind::NotFound => "Repository or resource not found".to_owned(),
        ProviderErrorKind::RateLimited => error.message.to_string(),
        // Keep GitHub's own 422 detail (e.g. `"abc" is not a numeric value`,
        // `The search is longer than 256 characters`): it names the bad input.
        ProviderErrorKind::Validation if error.status == Some(422) => {
            let detail = error.message.trim();
            let detail = detail.strip_prefix("Validation Failed: ").unwrap_or(detail);
            if detail.is_empty() || detail.eq_ignore_ascii_case("Validation Failed") {
                "Invalid search query or request parameters".to_owned()
            } else {
                format!("Invalid search query or request parameters: {detail}")
            }
        }
        ProviderErrorKind::Server if matches!(error.status, Some(502..=504)) => {
            "GitHub API temporarily unavailable".to_owned()
        }
        ProviderErrorKind::Transport => "Network connection failed".to_owned(),
        ProviderErrorKind::Timeout => "Request timeout".to_owned(),
        _ => error.message.to_string(),
    };
    let hint = if error.kind == ProviderErrorKind::Authentication {
        Some(GITHUB_AUTH_RECOVERY_HINT)
    } else if repository_not_found(&error) {
        Some(REPOSITORY_ACCESS_HINT)
    } else if error.reason == Some(ProviderErrorReason::RefNotFound) {
        Some("Verify the branch, tag, or SHA exists, or omit branch to use the default branch.")
    } else if error.kind == ProviderErrorKind::RateLimited {
        Some("Wait for Retry-After or the rate-limit reset; authenticate for a higher quota.")
    } else if error.kind == ProviderErrorKind::Validation
        && error.reason == Some(ProviderErrorReason::SearchWindowExceeded)
    {
        Some(if tool == ToolId::GhSearchRepo {
            "Lower page, or narrow with keywords, stars, created, or updated to reach deeper results."
        } else {
            "Lower page, or narrow with path, extension, or filename to reach deeper results."
        })
    } else {
        None
    };
    let hints = hint.map(str::to_owned).into_iter().collect();
    provider_row(&error, message, hints, None, failure_kind(error.kind))
}

/// `search` distinguishes the search-endpoint tool (ghSearchHistory) from the
/// direct-fetch tool (ghGetHistoryItem): only search failures should carry the
/// "Check search syntax" scopesSuggestion, and a direct fetch of a bogus
/// commit SHA (GitHub 422 "No commit found for SHA: …") is a not-found
/// condition, not a query-syntax problem.
fn history_error(error: ProviderError, search: bool) -> DomainResult {
    let (message, suggestion) = match error.kind {
        ProviderErrorKind::Authentication => (
            "GitHub authentication required",
            Some(GITHUB_AUTH_RECOVERY_HINT),
        ),
        ProviderErrorKind::Permission => (
            error.message.as_ref(),
            Some("Check repository permissions or authentication"),
        ),
        ProviderErrorKind::NotFound if repository_not_found(&error) => (
            "Repository not found, or private and not accessible to this token",
            None,
        ),
        ProviderErrorKind::NotFound
            if error.reason == Some(ProviderErrorReason::PullRequestIsIssue) =>
        {
            (error.message.as_ref(), None)
        }
        ProviderErrorKind::NotFound => ("Repository, resource, or path not found", None),
        ProviderErrorKind::RateLimited => (
            error.message.as_ref(),
            Some("Set GITHUB_TOKEN for higher rate limits (5000/hour vs 60/hour)"),
        ),
        ProviderErrorKind::Validation if error.status == Some(422) && search => (
            "Invalid search query or request parameters",
            Some("Check search syntax and parameter values"),
        ),
        ProviderErrorKind::Validation
            if error.status == Some(422) && error.message.starts_with("No commit found") =>
        {
            (
                "Commit not found - verify the ref/SHA exists in this repository",
                None,
            )
        }
        ProviderErrorKind::Validation if error.status == Some(422) => {
            ("Invalid request parameters", Some("Check parameter values"))
        }
        ProviderErrorKind::Server if matches!(error.status, Some(502..=504)) => (
            "GitHub API temporarily unavailable",
            Some("Retry the request after a short delay"),
        ),
        ProviderErrorKind::Transport => (
            "Network connection failed",
            Some("Check internet connection and GitHub API status"),
        ),
        ProviderErrorKind::Timeout => (
            "Request timeout",
            Some("Retry the request or check network connectivity"),
        ),
        _ => (error.message.as_ref(), None),
    };
    let kind = if error.status.is_some() {
        "http"
    } else if matches!(
        error.kind,
        ProviderErrorKind::Transport | ProviderErrorKind::Timeout
    ) {
        "network"
    } else {
        "unknown"
    };
    // The canonical output contract requires `data.error` to be a plain
    // string. Keep the diagnostic signal (type/status/rate-limit) as sibling
    // top-level fields rather than nesting an object under `error`; a nested
    // object here trips `outputContractViolation` and masks the real provider
    // failure (e.g. a search 422 on a renamed repository).
    let hints = if error.kind == ProviderErrorKind::Authentication {
        vec![GITHUB_AUTH_RECOVERY_HINT.to_owned()]
    } else if repository_not_found(&error) {
        vec![REPOSITORY_ACCESS_HINT.to_owned()]
    } else {
        Vec::new()
    };
    let mut row = provider_row(&error, message, hints, None, failure_kind(error.kind));
    let data = &mut row.data;
    data["type"] = json!(kind);
    if let Some(status) = error.status {
        data["status"] = json!(status);
    }
    if let Some(suggestion) = suggestion {
        data["scopesSuggestion"] = json!(suggestion);
    }
    if error.kind == ProviderErrorKind::RateLimited {
        data["rateLimitRemaining"] = json!(
            error
                .rate_limit
                .as_ref()
                .and_then(|rate| rate.remaining)
                .unwrap_or(0)
        );
    }
    if let Some(rate) = &error.rate_limit {
        if let Some(value) = rate.remaining
            && error.kind != ProviderErrorKind::RateLimited
        {
            data["rateLimitRemaining"] = json!(value);
        }
        if let Some(value) = rate.reset_epoch_seconds {
            data["rateLimitReset"] = json!(value.saturating_mul(1000));
        }
        if let Some(value) = rate.retry_after_seconds {
            data["retryAfter"] = json!(value);
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

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
            search_error(ToolId::GhSearchCode, error()),
            history_error(error(), true),
            history_error(error(), false),
            file_error(error(), &json!({"owner":"a","repo":"b","path":"x"})),
            provider_error(error()),
        ] {
            let rendered = result.data.to_string();
            assert!(rendered.contains("octocode auth login"), "{rendered}");
            assert!(rendered.contains("OCTOCODE_TOKEN"), "{rendered}");
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
            search_error(ToolId::GhSearchCode, error()),
            history_error(error(), true),
            history_error(error(), false),
            file_error(error(), &json!({"owner":"a","repo":"b","path":"x"})),
            provider_error(error()),
        ] {
            let text = result.data["error"].as_str().unwrap_or_default();
            assert!(text.contains("legal reasons"), "{}", result.data);
            assert_eq!(result.data["retryable"], false);
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
        let result = search_error(
            ToolId::GhSearchRepo,
            error("\"abc\" is not a numeric value", None),
        );
        assert_eq!(
            result.data["error"],
            "Invalid search query or request parameters: \"abc\" is not a numeric value"
        );
        let generic = search_error(ToolId::GhSearchCode, error("Validation Failed", None));
        assert_eq!(
            generic.data["error"],
            "Invalid search query or request parameters"
        );
        let window = Some(ProviderErrorReason::SearchWindowExceeded);
        let repo = search_error(ToolId::GhSearchRepo, error("window", window));
        assert!(
            !repo.data["hints"][0]
                .as_str()
                .unwrap()
                .contains("extension")
        );
        let code = search_error(ToolId::GhSearchCode, error("window", window));
        assert!(
            code.data["hints"][0]
                .as_str()
                .unwrap()
                .contains("extension")
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
            search_error(ToolId::GhSearchCode, error()),
            history_error(error(), true),
            history_error(error(), false),
            file_error(
                error(),
                &json!({"owner":"a","repo":"b","path":"src/lib.rs"}),
            ),
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
        assert_eq!(
            data["error"].as_str(),
            Some("Invalid search query or request parameters"),
        );
        // Diagnostic signal is preserved as sibling fields, not nested.
        assert_eq!(data["status"], json!(422));
        assert_eq!(data["httpStatus"], json!(422));
        assert_eq!(data["errorCode"], json!("validation"));
        assert_eq!(data["retryable"], false);
        assert_eq!(data["type"], json!("http"));
        assert!(data["scopesSuggestion"].is_string());
        assert_eq!(data["rateLimitRemaining"], json!(11));
        assert!(data["hints"][0].is_string());
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
            "branch": "no-such-branch"
        });

        let result = file_error(error, &query);
        let data = &result.data;

        assert_eq!(result.status, Some("error"));
        assert_eq!(result.failure, Some(FailureKind::NotFound));
        assert_eq!(data["errorCode"], json!("notFound"));
        assert_eq!(data["httpStatus"], json!(404));
        assert_eq!(data["retryable"], false);
        assert_eq!(
            data["error"].as_str(),
            Some("Branch, tag, or SHA not found for a/b: \"no-such-branch\"")
        );
        let hint = data["hints"][0].as_str().expect("ref hint");
        assert!(hint.contains("no-such-branch"), "{hint}");

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
        let result = file_error(error, &query);
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
        let result = file_error(error, &json!({"owner":"a","repo":"b","path":"src/lib.rs"}));
        assert_eq!(
            result.data["error"].as_str(),
            Some("Repository, resource, or path not found")
        );
    }

    #[test]
    fn file_error_directory_and_binary_get_accurate_recovery() {
        let query = json!({"owner":"a","repo":"b","path":"src","branch":"main"});
        let error = ProviderError::new(
            ProviderErrorKind::Validation,
            "Path \"src\" is a directory, not a file; list it with ghStructure.",
        )
        .with_reason(ProviderErrorReason::PathIsDirectory);
        let result = file_error(error, &query);
        let data = &result.data;
        assert!(
            data["error"]
                .as_str()
                .unwrap_or_default()
                .contains("is a directory"),
            "{data}"
        );
        assert_eq!(data["next"]["viewTree"]["query"]["path"], "src");
        assert_eq!(data["next"]["viewTree"]["query"]["branch"], "main");

        let error = ProviderError::new(ProviderErrorKind::Decode, "binary files are not supported");
        let result = file_error(error, &json!({"owner":"a","repo":"b","path":"x.png"}));
        let hint = result.data["hints"][0].as_str().unwrap_or_default();
        assert!(!hint.contains("Retry once"), "{hint}");
        assert!(hint.contains("Binary"), "{hint}");
    }

    /// Recovery keys on the typed reason: the same text without the reason
    /// gets no directory continuation.
    #[test]
    fn directory_recovery_keys_on_the_typed_reason_not_the_message() {
        let query = json!({"owner":"a","repo":"b","path":"src","branch":"main"});
        let untyped = ProviderError::new(ProviderErrorKind::Validation, "src is a directory");
        assert!(
            file_error(untyped, &query).data["next"]
                .get("viewTree")
                .is_none()
        );
    }

    #[test]
    fn rate_limit_metadata_is_camel_case() {
        let mut error = ProviderError::new(ProviderErrorKind::RateLimited, "slow down");
        error.rate_limit = Some(RateLimit {
            remaining: Some(0),
            reset_epoch_seconds: Some(1_700_000_000),
            retry_after_seconds: Some(30),
            resource: Some("core".into()),
        });
        let mut data = json!({});
        apply_provider_error_metadata(&mut data, &error);
        assert_eq!(data["rateLimit"]["resetEpochSeconds"], 1_700_000_000);
        assert_eq!(data["rateLimit"]["retryAfterSeconds"], 30);
        assert!(data["rateLimit"].get("reset_epoch_seconds").is_none());
    }

    /// ghGetHistoryItem is not a search endpoint: a bogus commit SHA (GitHub
    /// 422 "No commit found for SHA: …") must produce a commit-not-found
    /// message without the "Check search syntax" scopesSuggestion.
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
        assert!(
            data.get("scopesSuggestion").is_none(),
            "non-search operations must not suggest checking search syntax: {data}"
        );

        // The search-history tool keeps the search-syntax suggestion.
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
            result.data["scopesSuggestion"].as_str(),
            Some("Check search syntax and parameter values")
        );
    }
}
