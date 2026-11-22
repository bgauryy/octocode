//! Per-tool execution: each GitHub tool's typed query run through the shared provider into its row.
use super::GitHubServices;
use super::rows::{history_error, provider_error, tool_row};
use octocode_contract_types::{
    ErrorCode, GhCloneRepoQuery, GhGetFileContentQuery, GhSearchCodeQuery, GhSearchRepoQuery,
    GhStructureQuery,
};
use octocode_github::{ProviderError, RequestContext};
use octocode_runtime_base::execution::{ExecutionContext, ExecutionError};
use octocode_runtime_base::{
    id::GitHubTool,
    policy::path::PathPolicy,
    response::outcome::{DomainResult, Upstream},
    security::ContentSecurity,
};
use octocode_tool_github::gh_clone_repo;
use octocode_tool_github::{
    gh_clone_repo::CloneFailure, gh_get_file_content, gh_get_history_item, gh_search_code,
    gh_search_history, gh_search_repo, gh_structure,
};
use octocode_tools_shared::gh_git::{self, GitConfig, GitSetup};
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
                let withholds = |path: &str| paths.withholds_repository_file(path);
                // A page reads the files of the rows its share of the
                // response page holds.
                let window = Some(context.row_page_chars(self.auto_page_chars));
                tool_row(
                    gh_search_code::run(
                        &self.provider,
                        &query,
                        request,
                        security,
                        &withholds,
                        window,
                    )
                    .await,
                )
            }
            GitHubTool::GhStructure => {
                let query = typed!(GhStructureQuery);
                if !self.persistent && materializes(&query) {
                    return Ok(materialize_refused());
                }
                // A materialize reads blobs through git when it can.
                let blobs = match (&request, materializes(&query)) {
                    (Ok(request), true) if self.persistent => gh_git::blobs::BlobSource::new(
                        self.clone_setup_for(request, context, paths),
                    ),
                    _ => None,
                };
                tool_row(
                    gh_structure::run(&self.provider, &query, request, &self.home, blobs.as_ref())
                        .await,
                )
            }
            GitHubTool::GhGetFileContent => {
                if let Some(path) = query["path"].as_str()
                    && paths.withholds_repository_file(path)
                {
                    return Ok(withheld_file(path));
                }
                let query = typed!(GhGetFileContentQuery);
                let shaping = gh_get_file_content::Shaping {
                    security: security.clone(),
                    context: context.clone(),
                };
                match gh_get_file_content::run(
                    &self.provider,
                    &query,
                    request,
                    Some(context.row_page_chars(self.auto_page_chars)),
                    shaping,
                    // The bytes of a symlink are its target's: a target the
                    // policy withholds is refused as a read of it is, before
                    // any of its bytes are asked for.
                    &|target| paths.withholds_repository_file(target),
                )
                .await
                {
                    Ok(Ok(read)) => tool_row(Ok(read)),
                    Ok(Err(target)) => return Ok(withheld_file(&target)),
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
                    // Head text that widens a patch is file text: the policy
                    // decides which files it may read.
                    item.paths = Some(std::sync::Arc::new(paths.clone()));
                    match gh_get_history_item::run(&self.provider, &item, query, request, security)
                        .await
                    {
                        // The row states its own `status` (an empty page).
                        Ok(output) => crate::runtime::dispatch::value_result(output.data),
                        Err(failure) => tool_row(Err(failure)),
                    }
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
        let mut row = row;
        drop_withheld_reads(&mut row.data, paths);
        Ok(row)
    }

    /// The git side of one row: this row's credential, limiter, deadline and
    /// cancellation, under the runtime's git and path policy.
    fn clone_setup_for(
        &self,
        request: &RequestContext,
        context: &ExecutionContext,
        paths: &PathPolicy,
    ) -> std::sync::Arc<GitSetup> {
        let config = GitConfig::persistent(self.home.clone()).with_network_timeout(self.timeout);
        std::sync::Arc::new(GitSetup {
            config,
            endpoint: self.provider.transport.endpoint().clone(),
            git_limiter: self.provider.transport.git_limiter().for_row(request),
            credential: request.resolved_credential().cloned(),
            cancellation: std::sync::Arc::new(context.clone()),
            deadline: octocode_runtime_base::execution::cancel::deadline_of(context),
            path_policy: paths.clone(),
            git: self.git.clone(),
        })
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
        let setup = self.clone_setup_for(request, context, paths);
        match gh_clone_repo::run(&self.provider, query, Ok(request), &setup).await {
            Ok(mut result) => {
                result.location.lsp_trust_fetched_code = self.lsp_trust_fetched_code;
                DomainResult::payload(serde_json::to_value(result).unwrap_or_default(), None)
            }
            Err(CloneFailure::Provider(error)) => provider_error(error),
            Err(CloneFailure::Clone(error)) => {
                // Only a timed-out git transfer can succeed unchanged.
                let upstream = Upstream {
                    retryable: error.code == ErrorCode::Timeout,
                    http_status: error.http_status,
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
            Err(failure) => tool_row(Err(failure)),
        })
    }
}

/// Whether a listing asks to write its files under the home.
fn materializes(query: &GhStructureQuery) -> bool {
    matches!(query, GhStructureQuery::Tree(tree) if tree.materialize == Some(true))
}

/// The refusal of a repository file the path policy withholds, worded and
/// coded as a local read's, before any request.
fn withheld_file(path: &str) -> DomainResult {
    DomainResult::provider(
        ErrorCode::PathPolicyDenied,
        octocode_runtime_base::policy::discovery::withheld_message(
            path,
            &octocode_runtime_base::policy::discovery::repository_path(path),
        ),
        Vec::new(),
        None,
        Upstream::default(),
    )
}

/// Every ghGetFileContent lead in `data` (in `next`, `related` or `calls`, at
/// any depth, whichever tool emitted it) to a file the path policy withholds
/// under the current settings is dropped, since that read would be refused,
/// and one warning says how many query rows went. A lead left with no query
/// row goes whole, and so does a container (`next`, `related`, `calls`) the
/// pass leaves empty.
fn drop_withheld_reads(data: &mut Value, paths: &PathPolicy) {
    let dropped = drop_reads_in(data, paths);
    if dropped > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            data,
            [format!(
                "{dropped} ghGetFileContent read row(s) were dropped from leads: the security path policy withholds their files under the user's configuration."
            )],
        );
    }
}

/// Query rows dropped from `value` and its children ([`drop_withheld_reads`]).
fn drop_reads_in(value: &mut Value, paths: &PathPolicy) -> usize {
    let mut dropped = 0;
    let mut keep = |item: &mut Value| {
        let (count, emptied) = withheld_read(item, paths);
        dropped += count;
        if emptied {
            return false;
        }
        let inner = drop_reads_in(item, paths);
        dropped += inner;
        // Only a container this pass emptied goes; one empty before stays.
        inner == 0 || !is_empty_container(item)
    };
    match value {
        Value::Array(items) => items.retain_mut(&mut keep),
        Value::Object(fields) => fields.retain(|_, item| keep(item)),
        _ => {}
    }
    dropped
}

fn is_empty_container(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.is_empty(),
        Value::Object(fields) => fields.is_empty(),
        _ => false,
    }
}

/// For a ghGetFileContent call, drops its query rows whose path is withheld:
/// how many, and whether none is left.
fn withheld_read(call: &mut Value, paths: &PathPolicy) -> (usize, bool) {
    if call.get("tool").and_then(Value::as_str)
        != Some(octocode_runtime_base::id::ToolId::GhGetFileContent.as_str())
    {
        return (0, false);
    }
    let Some(rows) = call
        .pointer_mut("/input/queries")
        .and_then(Value::as_array_mut)
    else {
        return (0, false);
    };
    let before = rows.len();
    rows.retain(|row| {
        !row.get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| paths.withholds_repository_file(path))
    });
    let dropped = before - rows.len();
    (dropped, dropped > 0 && rows.is_empty())
}

/// Materialize is an execution prerequisite on storage, as clone is, refused
/// before any provider work. The same listing without it needs no storage.
fn materialize_refused() -> DomainResult {
    DomainResult::provider(
        ErrorCode::PersistentStorageDisabled,
        "materialize writes files under the home, and storage.mode is memory. List without materialize and read files with ghGetFileContent, or set storage.mode=\"persistent\" or OCTOCODE_STORAGE_MODE=persistent.".to_owned(),
        Vec::new(),
        None,
        Upstream::default(),
    )
}

#[cfg(test)]
mod lead_tests;
