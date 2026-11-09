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
        gh_clone_repo::{self, CloneConfig, CloneFailure, CloneSetup},
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
                tool_row(gh_structure::run(&self.provider, &query, request, &self.home).await)
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
        let mut row = row;
        drop_withheld_reads(&mut row.data, paths);
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
        let setup = std::sync::Arc::new(CloneSetup {
            config,
            endpoint: self.provider.transport.endpoint().clone(),
            git_limiter: self.provider.transport.git_limiter().for_row(request),
            credential: request.resolved_credential().cloned(),
            cancellation: std::sync::Arc::new(context.clone()),
            deadline: crate::execution::cancel::deadline_of(context),
            path_policy: paths.clone(),
            git: self.git.clone(),
        });
        match gh_clone_repo::run(&self.provider, query, Ok(request), &setup).await {
            Ok(result) => {
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
            Err(failure) => history_search_error(failure),
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
        crate::policy::discovery::withheld_message(
            path,
            &crate::policy::discovery::repository_path(path),
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
        crate::response::paging::push_warnings(
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
        != Some(crate::id::ToolId::GhGetFileContent.as_str())
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
mod lead_tests {
    use super::drop_withheld_reads;
    use crate::policy::path::{PathPolicy, PathPolicyConfig};
    use serde_json::{Value, json};

    fn policy(allow: bool) -> PathPolicy {
        let root = tempfile::tempdir().expect("root");
        PathPolicy::new(PathPolicyConfig {
            workspace_root: Some(root.path().to_path_buf()),
            allow_credential_files: allow,
            ..Default::default()
        })
        .expect("policy")
    }

    fn read(paths: &[&str]) -> Value {
        let rows: Vec<Value> = paths
            .iter()
            .map(|path| json!({"owner":"o","repo":"r","path":path}))
            .collect();
        json!({"kind":"related","tool":"ghGetFileContent","input":{"queries":rows}})
    }

    fn row() -> Value {
        json!({
            "files": [{"path": ".env", "related": [read(&[".env"]), read(&["src/a.rs"])]}],
            "next": {"read": read(&[".env", ".ssh/config", "src/a.rs"])},
            "calls": [read(&[".ssh/config"]),
                {"kind":"related","tool":"ghSearchCode","input":{"queries":[{"owner":"o","path":".env"}]}}]
        })
    }

    #[test]
    fn leads_to_withheld_files_are_dropped_with_one_warning() {
        let mut data = row();
        drop_withheld_reads(&mut data, &policy(false));
        let text = data.to_string();
        assert!(!text.contains("\"path\":\".ssh/config\""), "{text}");
        assert_eq!(
            data["files"][0]["related"].as_array().map(Vec::len),
            Some(1),
            "{text}"
        );
        assert_eq!(
            data["next"]["read"]["input"]["queries"],
            json!([{"owner":"o","repo":"r","path":"src/a.rs"}])
        );
        // A call of another tool is not a read of the file.
        assert_eq!(data["calls"].as_array().map(Vec::len), Some(1), "{text}");
        assert_eq!(data["warnings"].as_array().map(Vec::len), Some(1), "{text}");
        // Four query rows go, from three calls: the warning counts rows.
        assert!(
            text.contains("4 ghGetFileContent read row(s) were dropped from leads"),
            "{text}"
        );
    }

    #[test]
    fn a_container_the_pass_empties_goes_with_its_last_lead() {
        let mut data = json!({
            "files": [{"path": ".env", "related": [read(&[".env"])]}],
            "next": {"read": read(&[".env"])},
            "calls": [read(&[".ssh/config"])],
            "kept": {"empty": {}, "none": []}
        });
        drop_withheld_reads(&mut data, &policy(false));
        let text = data.to_string();
        assert!(data.get("next").is_none(), "no next:{{}}: {text}");
        assert!(data.get("calls").is_none(), "no calls:[]: {text}");
        assert!(
            data["files"][0].get("related").is_none(),
            "no related:[]: {text}"
        );
        assert_eq!(data["files"][0]["path"], ".env", "the row stays: {text}");
        // A container that was empty before the pass is not its to remove.
        assert_eq!(data["kept"], json!({"empty": {}, "none": []}), "{text}");
        assert!(
            text.contains("3 ghGetFileContent read row(s) were dropped from leads"),
            "{text}"
        );
    }

    #[test]
    fn the_opt_in_keeps_credential_name_leads_never_sensitive_directories() {
        let mut data = row();
        drop_withheld_reads(&mut data, &policy(true));
        let text = data.to_string();
        assert_eq!(
            data["files"][0]["related"].as_array().map(Vec::len),
            Some(2),
            "{text}"
        );
        assert!(!text.contains(".ssh/config"), "{text}");
        assert!(
            text.contains("2 ghGetFileContent read row(s) were dropped from leads"),
            "{text}"
        );
    }
}
