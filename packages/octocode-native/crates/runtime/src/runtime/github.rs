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
        local_fetch::LocalFetchRegex,
    },
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

type Store = ChainedCredentialSource<PlatformCredentialStore, GhCliCredentialSource>;

pub(super) struct GitHubServices {
    credentials: Arc<ConfigCredentialResolver<Store>>,
    provider: GitHubProvider<StaticCredentialResolver, GitHubContentCache>,
    timeout: Duration,
    home: PathBuf,
    oauth_client_id: Option<String>,
    refresh_lock: std::sync::Mutex<()>,
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
        let timeout = Duration::from_secs_f64(config.resolved.network.timeout / 1000.0);
        let oauth_client_id = config
            .env_value("OCTOCODE_GITHUB_CLIENT_ID")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let credentials = Arc::new(ConfigCredentialResolver::new(
            config,
            ChainedCredentialSource::new(PlatformCredentialStore, GhCliCredentialSource),
        ));
        Ok(Self {
            credentials,
            provider: GitHubProvider { transport, cache },
            timeout,
            home,
            oauth_client_id,
            refresh_lock: std::sync::Mutex::new(()),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute_query(
        &self,
        tool: &str,
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
                return Ok(if tool == "ghGetFileContent" {
                    file_error(error, query)
                } else if tool == "ghSearch" {
                    search_error(error)
                } else {
                    history_error(error, tool == "ghSearchHistory")
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
        let credential = self
            .credentials
            .clone()
            .start_resolve(OwnedCredentialRequest {
                host: host.clone(),
                override_token: None,
            })
            .finish();
        let credential = credential?;
        context.check().map_err(|error| {
            ProviderError::new(
                if error == ExecutionError::Timeout {
                    ProviderErrorKind::Timeout
                } else {
                    ProviderErrorKind::Cancelled
                },
                "GitHub credential resolution exceeded the request budget",
            )
        })?;
        let credential = if credential
            .as_ref()
            .is_some_and(|value| value.source == CredentialSource::Storage)
        {
            // Re-read stored metadata while holding one process-wide refresh
            // section so concurrent calls cannot exchange the same refresh
            // token more than once. The second waiter observes the fresh token.
            let _refresh_guard = self
                .refresh_lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let client_id = self.oauth_client_id.as_deref().unwrap_or_else(|| {
                if host == "github.com" {
                    crate::providers::github::login::GITHUB_APP_CLIENT_ID
                } else {
                    ""
                }
            });
            match handle.block_on(
                crate::providers::github::login::resolve_stored_with_refresh(&host, client_id),
            ) {
                Ok(Some(refreshed)) => Some(refreshed),
                Ok(None) => credential,
                Err(refresh_error) => match GhCliCredentialSource.load_blocking(&host) {
                    Ok(Some(token)) => {
                        Some(ResolvedCredential::new(token, CredentialSource::Storage))
                    }
                    _ => {
                        return Err(refresh_error);
                    }
                },
            }
        } else {
            credential
        };
        let mut request_context =
            RequestContext::with_resolved_credential(self.timeout, 16 * 1024 * 1024, credential);
        request_context.deadline = context.deadline.min(Instant::now() + self.timeout);
        request_context.cancellation = context.cancellation.clone();
        Ok(request_context)
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_resolved(
        &self,
        tool: &str,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        security: &ContentSecurity,
        regex: &LocalFetchRegex,
        paths: &PathPolicy,
    ) -> Result<DomainResult, ExecutionError> {
        let mut execution_query = query.clone();
        if let Some(object) = execution_query.as_object_mut() {
            object.remove("goal");
            object.remove("reasoning");
            object.remove("debug");
        }
        match tool {
            "ghGetFileContent" => {
                self.execute_file_resolved(
                    &execution_query,
                    request_context,
                    context,
                    security,
                    regex,
                )
                .await
            }
            "ghGetHistoryItem" => {
                self.execute_history_item_resolved(
                    &execution_query,
                    request_context,
                    context,
                    security,
                )
                .await
            }
            "ghSearch" => {
                self.execute_search_resolved(&execution_query, request_context, context, security)
                    .await
            }
            "ghSearchHistory" => {
                self.execute_search_history_resolved(
                    &execution_query,
                    request_context,
                    context,
                    security,
                )
                .await
            }
            "ghCloneRepo" => {
                self.execute_clone_resolved(&execution_query, request_context, context, paths)
                    .await
            }
            _ => Err(ExecutionError::WorkerFailed),
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
        let query: gh_get_history_item::GhGetHistoryItemQuery =
            serde_json::from_value(query.clone()).map_err(|_| ExecutionError::WorkerFailed)?;
        let result = gh_get_history_item::execute(
            &self.provider.transport,
            &query,
            request_context,
            security,
        )
        .await;
        context.check()?;
        Ok(match result {
            Ok(data) => DomainResult {
                diagnostics: Default::default(),
                status: match data.get("status").and_then(Value::as_str) {
                    Some("empty") => Some("empty"),
                    Some("error") => Some("error"),
                    _ => None,
                },
                data,
                cache: false,
                source_digest: None,
                failure: None,
            },
            Err(error) => history_error(error, false),
        })
    }

    async fn execute_search_resolved(
        &self,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        security: &ContentSecurity,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        let query: gh_search::GhSearchQuery =
            serde_json::from_value(query.clone()).map_err(|_| ExecutionError::WorkerFailed)?;
        let result = gh_search::execute(
            &self.provider,
            &query,
            request_context,
            security,
            &self.home,
        )
        .await;
        context.check()?;
        Ok(match result {
            Ok(output) => DomainResult {
                diagnostics: output.diagnostics,
                status: output.status,
                data: output.data,
                cache: false,
                source_digest: None,
                failure: (output.status == Some("error")).then_some(FailureKind::Execution),
            },
            Err(error) => search_error(error),
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
            serde_json::from_value(query.clone()).map_err(|_| ExecutionError::WorkerFailed)?;
        let result =
            gh_search_history::execute(&self.provider.transport, &query, request_context, security)
                .await;
        context.check()?;
        Ok(match result {
            Ok(data) => DomainResult {
                diagnostics: Default::default(),
                status: match data.get("status").and_then(Value::as_str) {
                    Some("empty") => Some("empty"),
                    Some("error") => Some("error"),
                    _ => None,
                },
                data,
                cache: false,
                source_digest: None,
                failure: None,
            },
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
            serde_json::from_value(query.clone()).map_err(|_| ExecutionError::WorkerFailed)?;
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
                    let owner = &query.owner;
                    let repo = &query.repo;
                    return Ok(DomainResult {
                        diagnostics: Default::default(),
                        data: json!({
                            "error": format!("Repository not found: {owner}/{repo}"),
                            "errorCode": "clone.repositoryNotFound",
                            "hints": [format!(
                                "Verify the owner/repo spelling and that {owner}/{repo} exists and is accessible with your credentials."
                            )],
                        }),
                        status: Some("error"),
                        source_digest: None,
                        cache: false,
                        failure: Some(FailureKind::NotFound),
                    });
                }
                Err(_) => None,
            }
        } else {
            None
        };
        let default_branch = metadata.as_ref().map(|value| value.default_branch.as_str());
        // CloneConfig treats cache_home as the octocode home and derives
        // tmp/clone, tmp/clone-locks, tmp/clone-tmp, and tmp/git-home itself.
        let config = CloneConfig::persistent(self.home.clone());
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
            Ok(result) => Ok(DomainResult {
                diagnostics: Default::default(),
                data: serde_json::to_value(result).map_err(|_| ExecutionError::WorkerFailed)?,
                status: None,
                source_digest: None,
                cache: false,
                failure: None,
            }),
            Err(error) => Ok(DomainResult {
                diagnostics: Default::default(),
                data: json!({"error":error.message,"errorCode":error.code}),
                status: Some("error"),
                source_digest: None,
                cache: false,
                failure: Some(FailureKind::Execution),
            }),
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
            serde_json::from_value(query.clone()).map_err(|_| ExecutionError::WorkerFailed)?;
        let result = gh_get_file_content::execute(
            &self.provider,
            &query,
            request_context,
            None,
            security,
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
                    if file["resolvedBranch"].as_str() == query.branch.as_deref()
                        && let Some(map) = file.as_object_mut()
                    {
                        map.remove("resolvedBranch");
                    }
                }
                Ok(DomainResult {
                    diagnostics: Default::default(),
                    data,
                    status,
                    source_digest: None,
                    failure: (status == Some("error")).then_some(FailureKind::Execution),
                    cache,
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
        let data = json!({
            "owner": owner,
            "repo": repo,
            "path": query["path"],
            "error": format!("Branch, tag, or SHA not found for {owner}/{repo}: \"{reference}\""),
            "hints": [format!(
                "Verify the ref \"{reference}\" exists (branch, tag, or full commit SHA), or omit branch to use the default branch."
            )],
        });
        return DomainResult {
            diagnostics: Default::default(),
            data,
            status: Some("error"),
            source_digest: None,
            cache: false,
            failure: Some(FailureKind::NotFound),
        };
    }
    let message = match error.kind {
        ProviderErrorKind::Authentication => "GitHub authentication required".into(),
        ProviderErrorKind::Permission => "Access forbidden - insufficient permissions".into(),
        ProviderErrorKind::NotFound => "Repository, resource, or path not found".into(),
        ProviderErrorKind::Validation => "Invalid search query or request parameters".into(),
        ProviderErrorKind::Server if matches!(error.status, Some(502..=504)) => {
            "GitHub API temporarily unavailable".into()
        }
        ProviderErrorKind::Transport => "Network connection failed".into(),
        ProviderErrorKind::Timeout => "Request timeout".into(),
        _ if error.message.as_ref() == "binary files are not supported" => {
            "Binary file detected. Cannot display as text - download directly from GitHub".into()
        }
        _ => error.message.to_string(),
    };
    let mut data = json!({"owner":owner,"repo":repo,"path":query["path"],"error":message});
    if error.kind == ProviderErrorKind::Authentication {
        data["hints"] = json!(["octocode login, or set GITHUB_TOKEN / GH_TOKEN"]);
    }
    if error.kind == ProviderErrorKind::NotFound {
        data["hints"] = json!([format!(
            "verify the path (exact case, no leading slash) and branch; use ghSearch with operation:\"tree\", owner:\"{owner}\", repo:\"{repo}\""
        )]);
        let requested = query["path"].as_str().unwrap_or_default();
        let parent = std::path::Path::new(requested)
            .parent()
            .map(|path| path.to_string_lossy().into_owned())
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| ".".into());
        // The output contract validates this recovery hint against the ghSearch
        // tree-continuation schema, which requires the paginated defaulted
        // fields. Stamp the contract defaults (fresh page 1 — this is an
        // advisory "start a new bounded query", not a next-page of the fetch).
        let mut tree = json!({
            "tool": "ghSearch",
            "query": {
                "operation": "tree",
                "owner": owner,
                "repo": repo,
                "path": parent,
                "page": 1,
                "pageSize": 100,
                "debug": false
            },
            "confidence": "low"
        });
        if let Some(branch) = query["branch"].as_str() {
            tree["query"]["branch"] = json!(branch);
        }
        data["next"] = json!({ "viewTree": tree });
    }
    DomainResult {
        diagnostics: Default::default(),
        data,
        status: Some("error"),
        source_digest: None,
        cache: false,
        failure: Some(failure_kind(error.kind)),
    }
}

pub(super) fn provider_error(error: ProviderError) -> DomainResult {
    let failure = failure_kind(error.kind);
    DomainResult {
        diagnostics: Default::default(),
        data: json!({"error":error.message,"errorCode":error.kind,"provider":error}),
        status: Some("error"),
        source_digest: None,
        failure: Some(failure),
        cache: false,
    }
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

/// Error formatter for ghSearch. Like `history_error`, the output contract
/// requires `data.error` to be a plain string; this variant carries a
/// ghSearch-specific message/hint set, so keep it separate.
fn search_error(error: ProviderError) -> DomainResult {
    let failure = failure_kind(error.kind);
    let message = match error.kind {
        ProviderErrorKind::Authentication => "GitHub authentication required".to_owned(),
        ProviderErrorKind::Permission => "Access forbidden — insufficient permissions".to_owned(),
        ProviderErrorKind::NotFound => "Repository or resource not found".to_owned(),
        ProviderErrorKind::RateLimited => error.message.to_string(),
        ProviderErrorKind::Validation if error.status == Some(422) => {
            "Invalid search query or request parameters".to_owned()
        }
        ProviderErrorKind::Server if matches!(error.status, Some(502..=504)) => {
            "GitHub API temporarily unavailable".to_owned()
        }
        ProviderErrorKind::Transport => "Network connection failed".to_owned(),
        ProviderErrorKind::Timeout => "Request timeout".to_owned(),
        _ => error.message.to_string(),
    };
    let error_code =
        serde_json::to_value(error.kind).unwrap_or(serde_json::Value::String("unknown".into()));
    let mut data = json!({"error": message, "errorCode": error_code});
    if error.kind == ProviderErrorKind::Authentication {
        data["hints"] = json!(["octocode login, or set GITHUB_TOKEN / GH_TOKEN"]);
    } else if error.kind == ProviderErrorKind::RateLimited {
        data["hints"] = json!(["Set GITHUB_TOKEN for higher rate limits (5000/hour vs 60/hour)"]);
    }
    DomainResult {
        diagnostics: Default::default(),
        data,
        status: Some("error"),
        source_digest: None,
        cache: false,
        failure: Some(failure),
    }
}

/// `search` distinguishes the search-endpoint tool (ghSearchHistory) from the
/// direct-fetch tool (ghGetHistoryItem): only search failures should carry the
/// "Check search syntax" scopesSuggestion, and a direct fetch of a bogus
/// commit SHA (GitHub 422 "No commit found for SHA: …") is a not-found
/// condition, not a query-syntax problem.
fn history_error(error: ProviderError, search: bool) -> DomainResult {
    let failure = failure_kind(error.kind);
    let (message, suggestion) = match error.kind {
        ProviderErrorKind::Authentication => (
            "GitHub authentication required",
            Some("octocode login, or set GITHUB_TOKEN / GH_TOKEN"),
        ),
        ProviderErrorKind::Permission => (
            "Access forbidden - insufficient permissions",
            Some("Check repository permissions or authentication"),
        ),
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
        ProviderErrorKind::Validation if error.status == Some(422) => (
            "Invalid request parameters",
            Some("Check parameter values"),
        ),
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
    let mut data = json!({"type":kind,"error":message});
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
    if let Some(rate) = error.rate_limit {
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
    if error.kind == ProviderErrorKind::Authentication {
        data["hints"] = json!(["octocode login, or set GITHUB_TOKEN / GH_TOKEN"]);
    }
    DomainResult {
        diagnostics: Default::default(),
        data,
        status: Some("error"),
        source_digest: None,
        cache: false,
        failure: Some(failure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            }),
            retryable: false,
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
        assert_eq!(data["type"], json!("http"));
        assert!(data["scopesSuggestion"].is_string());
        assert_eq!(data["rateLimitRemaining"], json!(11));
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
        };
        let result = file_error(error, &json!({"owner":"a","repo":"b","path":"src/lib.rs"}));
        assert_eq!(
            result.data["error"].as_str(),
            Some("Repository, resource, or path not found")
        );
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
        };
        let result = history_error(error, true);
        assert_eq!(
            result.data["scopesSuggestion"].as_str(),
            Some("Check search syntax and parameter values")
        );
    }
}
