//! One pooled HTTP client; credential acquisition stays inside admitted blocking work.
use super::{
    ExecutionContext, ExecutionError, FailureKind, dispatch::DomainResult,
    github_cache::GitHubContentCache,
};
use crate::{
    config::{self, ConfigOutput},
    policy::path::PathPolicy,
    providers::github::*,
    security::ContentSecurity,
    tools::{
        gh_clone_repo::{self, CloneConfig, CloneContext, SystemGit},
        gh_get_file_content, gh_get_history_item, gh_search, gh_search_history,
        local_fetch::LocalFetchRegex,
    },
};
use futures_util::{StreamExt, TryStreamExt, stream};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

type Store = ChainedCredentialSource<
    ChainedCredentialSource<PlatformCredentialStore, LegacyCredentialStore>,
    GhCliCredentialSource,
>;

pub(super) struct GitHubServices {
    credentials: Arc<ConfigCredentialResolver<Store>>,
    provider: GitHubProvider<StaticCredentialResolver, GitHubContentCache>,
    timeout: Duration,
    home: PathBuf,
    persistent: bool,
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
        let graphql_enabled = config.resolved.github.graphql_enabled;
        let transport = GitHubTransport::new(
            endpoint,
            Arc::new(StaticCredentialResolver::anonymous()),
            RetryPolicy {
                max_attempts: (config.resolved.network.max_retries as u8).saturating_add(1),
                ..Default::default()
            },
        )?
        .with_graphql_enabled(graphql_enabled);
        let timeout = Duration::from_secs_f64(config.resolved.network.timeout / 1000.0);
        let persistent = config::is_persistent_storage_enabled(&config.resolved);
        let credentials = Arc::new(ConfigCredentialResolver::new(
            config,
            ChainedCredentialSource::new(
                ChainedCredentialSource::new(
                    PlatformCredentialStore,
                    LegacyCredentialStore::new(home.clone()),
                ),
                GhCliCredentialSource,
            ),
        ));
        Ok(Self {
            credentials,
            provider: GitHubProvider { transport, cache },
            timeout,
            home,
            persistent,
        })
    }

    pub fn execute_queries(
        &self,
        tool: &str,
        queries: &[Value],
        context: &ExecutionContext,
        security: &ContentSecurity,
        regex: &LocalFetchRegex,
        handle: &tokio::runtime::Handle,
        paths: &PathPolicy,
    ) -> Result<Vec<DomainResult>, ExecutionError> {
        context.check()?;
        let credential = self
            .credentials
            .clone()
            .start_resolve(OwnedCredentialRequest {
                host: self.provider.transport.endpoint().credential_host().into(),
                override_token: None,
            })
            .finish();
        context.check()?;
        let credential = match credential {
            Ok(value) => {
                if let Ok(origins) = LoginOrigins::from_endpoint(self.provider.transport.endpoint())
                {
                    match handle.block_on(refresh_storage_if_needed(
                        &origins,
                        GITHUB_APP_CLIENT_ID,
                        &PlatformIo,
                        value,
                    )) {
                        Ok(value) => value,
                        Err(error) => {
                            return Ok(queries
                                .iter()
                                .map(|query| {
                                    if tool == "ghGetFileContent" {
                                        file_error(error.clone(), query)
                                    } else {
                                        history_error(error.clone())
                                    }
                                })
                                .collect());
                        }
                    }
                } else {
                    value
                }
            }
            Err(error) => {
                return Ok(queries
                    .iter()
                    .map(|query| {
                        if tool == "ghGetFileContent" {
                            file_error(error.clone(), query)
                        } else {
                            history_error(error.clone())
                        }
                    })
                    .collect());
            }
        };
        let mut request_context =
            RequestContext::with_resolved_credential(self.timeout, 16 * 1024 * 1024, credential);
        request_context.deadline = context.deadline.min(Instant::now() + self.timeout);
        request_context.cancellation = context.cancellation.clone();
        // Futures stay owned by this admitted batch; dropping the stream drops
        // in-flight HTTP work. Credential acquisition runs once per batch.
        handle.block_on(
            stream::iter(queries)
                .map(|query| {
                    self.execute_resolved(
                        tool,
                        query,
                        &request_context,
                        context,
                        security,
                        regex,
                        paths,
                    )
                })
                .buffered(3)
                .try_collect(),
        )
    }

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
        match tool {
            "ghGetFileContent" => {
                self.execute_file_resolved(query, request_context, context, security, regex)
                    .await
            }
            "ghGetHistoryItem" => {
                self.execute_history_item_resolved(query, request_context, context, security)
                    .await
            }
            "ghSearch" => {
                self.execute_search_resolved(query, request_context, context, security, paths)
                    .await
            }
            "ghSearchHistory" => {
                self.execute_search_history_resolved(query, request_context, context, security)
                    .await
            }
            "ghCloneRepo" => {
                self.execute_clone_resolved(query, request_context, context, paths)
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
            Err(error) => history_error(error),
        })
    }

    async fn execute_search_resolved(
        &self,
        query: &Value,
        request_context: &RequestContext,
        context: &ExecutionContext,
        security: &ContentSecurity,
        paths: &PathPolicy,
    ) -> Result<DomainResult, ExecutionError> {
        context.check()?;
        let query: gh_search::GhSearchQuery =
            serde_json::from_value(query.clone()).map_err(|_| ExecutionError::WorkerFailed)?;
        let result = gh_search::execute(
            &self.provider,
            &query,
            request_context,
            security,
            &gh_search::MaterializeEnv {
                home: &self.home,
                persistent: self.persistent,
                paths,
            },
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
            Err(error) => history_error(error),
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
            Err(error) => history_error(error),
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
            self.provider
                .transport
                .repository_metadata(&query.owner, &query.repo, request_context)
                .await
                .ok()
        } else {
            None
        };
        let default_branch = metadata.as_ref().map(|value| value.default_branch.as_str());
        let config = CloneConfig::persistent(self.home.join("tmp").join("clone"));
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
    let message = match error.kind {
        ProviderErrorKind::Authentication => {
            format!("GitHub authentication required — {AUTH_LOGIN_HINT}")
        }
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
    let owner = query["owner"].as_str().unwrap_or_default();
    let repo = query["repo"].as_str().unwrap_or_default();
    let mut data = json!({"owner":owner,"repo":repo,"path":query["path"],"error":message});
    if error.kind == ProviderErrorKind::NotFound {
        data["hints"] = json!([format!(
            "verify the path (exact case, no leading slash) and branch; use ghSearch with operation:\"tree\", owner:\"{owner}\", repo:\"{repo}\""
        )]);
        let mut tree_query = json!({
            "operation": "tree",
            "owner": owner,
            "repo": repo,
            "path": parent_tree_path(query["path"].as_str().unwrap_or("")),
        });
        if let Some(branch) = query
            .get("branch")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            tree_query["branch"] = json!(branch);
        }
        data["next"] = json!({
            "viewTree": {
                "tool": "ghSearch",
                "query": tree_query,
                "confidence": "low"
            }
        });
    }
    if error.kind == ProviderErrorKind::Authentication {
        data["scopesSuggestion"] = json!(AUTH_LOGIN_HINT);
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

fn parent_tree_path(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .filter(|parent| !parent.is_empty())
        .unwrap_or_else(|| ".".into())
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

fn history_error(error: ProviderError) -> DomainResult {
    let failure = failure_kind(error.kind);
    let (message, suggestion) = match error.kind {
        ProviderErrorKind::Authentication => {
            ("GitHub authentication required", Some(AUTH_LOGIN_HINT))
        }
        ProviderErrorKind::Permission => (
            "Access forbidden - insufficient permissions",
            Some("Check repository permissions or authentication"),
        ),
        ProviderErrorKind::NotFound => ("Repository, resource, or path not found", None),
        ProviderErrorKind::RateLimited => (
            error.message.as_ref(),
            Some("Set GITHUB_TOKEN for higher rate limits (5000/hour vs 60/hour)"),
        ),
        ProviderErrorKind::Validation if error.status == Some(422) => (
            "Invalid search query or request parameters",
            Some("Check search syntax and parameter values"),
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
    let mut detail = json!({"type":kind,"error":message});
    if let Some(status) = error.status {
        detail["status"] = json!(status);
    }
    if let Some(suggestion) = suggestion {
        detail["scopesSuggestion"] = json!(suggestion);
    }
    if error.kind == ProviderErrorKind::RateLimited {
        detail["rateLimitRemaining"] = json!(
            error
                .rate_limit
                .as_ref()
                .and_then(|rate| rate.remaining)
                .unwrap_or(0)
        );
    }
    if let Some(rate) = error.rate_limit {
        if let Some(value) = rate.remaining {
            if error.kind != ProviderErrorKind::RateLimited {
                detail["rateLimitRemaining"] = json!(value);
            }
        }
        if let Some(value) = rate.reset_epoch_seconds {
            detail["rateLimitReset"] = json!(value.saturating_mul(1000));
        }
        if let Some(value) = rate.retry_after_seconds {
            detail["retryAfter"] = json!(value);
        }
    }
    DomainResult {
        diagnostics: Default::default(),
        data: json!({"error":detail}),
        status: Some("error"),
        source_digest: None,
        cache: false,
        failure: Some(failure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn file_not_found_emits_view_tree_for_parent_path() {
        let result = file_error(
            ProviderError::new(ProviderErrorKind::NotFound, "missing"),
            &json!({
                "owner": "octo",
                "repo": "code",
                "path": "src/lib.rs",
                "branch": "main"
            }),
        );
        assert_eq!(result.status, Some("error"));
        assert_eq!(
            result.data["error"],
            "Repository, resource, or path not found"
        );
        assert_eq!(
            result.data["hints"][0],
            "verify the path (exact case, no leading slash) and branch; use ghSearch with operation:\"tree\", owner:\"octo\", repo:\"code\""
        );
        assert_eq!(result.data["next"]["viewTree"]["tool"], "ghSearch");
        assert_eq!(result.data["next"]["viewTree"]["confidence"], "low");
        assert_eq!(
            result.data["next"]["viewTree"]["query"],
            json!({
                "operation": "tree",
                "owner": "octo",
                "repo": "code",
                "path": "src",
                "branch": "main"
            })
        );
    }

    #[test]
    fn file_not_found_at_repo_root_uses_dot_path_without_invented_branch() {
        let result = file_error(
            ProviderError::new(ProviderErrorKind::NotFound, "missing"),
            &json!({
                "owner": "octo",
                "repo": "code",
                "path": "README.md"
            }),
        );
        assert_eq!(result.data["next"]["viewTree"]["query"]["path"], ".");
        assert!(
            result.data["next"]["viewTree"]["query"]
                .get("branch")
                .is_none()
        );
    }

    #[test]
    fn parent_tree_path_never_emits_empty_string() {
        assert_eq!(parent_tree_path("lib.rs"), ".");
        assert_eq!(parent_tree_path(""), ".");
        assert_eq!(parent_tree_path("a/b/c.ts"), "a/b");
    }
}
