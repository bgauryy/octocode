//! Per-tool execution: each GitHub tool's typed query run through the shared provider into its row.
use super::GitHubServices;
use super::rows::{history_error, history_search_error, provider_error, tool_row};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::{
    id::GitHubTool,
    policy::path::PathPolicy,
    response::outcome::{DomainResult, Upstream},
    security::ContentSecurity,
    tools::{
        gh_clone_repo::{self, CloneConfig, CloneContext, CloneFailure},
        gh_get_file_content, gh_get_history_item, gh_search_code, gh_search_history,
        gh_search_repo, gh_structure,
    },
};
use octocode_contract_types::{
    ErrorCode, GhCloneRepoQuery, GhGetFileContentQuery, GhSearchCodeQuery, GhSearchRepoQuery,
    GhStructureQuery,
};
use octocode_github::{ProviderError, RequestContext};
use serde_json::Value;

impl GitHubServices {
    pub(super) async fn execute_resolved(
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
                match crate::runtime::dispatch::parse_query::<$type>(query) {
                    Ok(query) => query,
                    Err(row) => return Ok(*row),
                }
            };
        }
        let request = request.map_err(Clone::clone);
        let row = match tool {
            GitHubTool::GhSearchRepo => {
                let query = typed!(GhSearchRepoQuery);
                tool_row(gh_search_repo::run(&self.provider, &query, request).await)
            }
            GitHubTool::GhSearchCode => {
                let query = typed!(GhSearchCodeQuery);
                tool_row(gh_search_code::run(&self.provider, &query, request, security).await)
            }
            GitHubTool::GhStructure => {
                let query = typed!(GhStructureQuery);
                tool_row(gh_structure::run(&self.provider, &query, request, &self.home).await)
            }
            GitHubTool::GhGetFileContent => {
                let query = typed!(GhGetFileContentQuery);
                let shaping = gh_get_file_content::Shaping {
                    security: security.clone(),
                    memo: self.sanitized_views.clone(),
                    views: self.source_views.clone(),
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
                    Ok(read) => tool_row(Ok(read)),
                    Err(failure) => tool_row(Err(failure)),
                }
            }
            GitHubTool::GhCloneRepo => {
                let query = typed!(GhCloneRepoQuery);
                self.run_clone(&query, request, context, paths).await
            }
            GitHubTool::GhGetHistoryItem => match request {
                Ok(request) => {
                    let mut item = match gh_get_history_item::HistoryItemRequest::from_row(query) {
                        Ok(item) => item,
                        Err(error) => return Ok(crate::runtime::dispatch::invalid_query(&error)),
                    };
                    // Patch windows fill this row's share of the response page.
                    item.auto_page_chars = Some(context.row_page_chars(self.auto_page_chars));
                    match gh_get_history_item::run(&self.provider, &item, query, request, security)
                        .await
                    {
                        // The row states its own `status` (an empty page).
                        Ok(output) => DomainResult {
                            remaining: output.remaining,
                            ..crate::runtime::dispatch::value_result(output.data)
                        },
                        Err(failure) => tool_row(Err(failure)),
                    }
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

    /// Clone a fresh checkout with the caller's access and runtime policy.
    async fn run_clone(
        &self,
        query: &GhCloneRepoQuery,
        request: Result<&RequestContext, ProviderError>,
        context: &ExecutionContext,
        paths: &PathPolicy,
    ) -> DomainResult {
        let request = match request {
            Ok(request) => request,
            Err(error) => return provider_error(error),
        };
        let config = CloneConfig::persistent(self.home.clone()).with_network_timeout(self.timeout);
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
                    error.calls.map(|calls| *calls),
                    upstream,
                )
            }
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
        let query = match crate::runtime::dispatch::parse_query(query) {
            Ok(query) => query,
            Err(row) => return Ok(*row),
        };
        let result =
            gh_search_history::execute(&self.provider, query, request_context, security).await;
        context.check()?;
        Ok(match result {
            Ok(data) => crate::runtime::dispatch::value_result(data),
            Err(failure) => history_search_error(failure),
        })
    }
}
