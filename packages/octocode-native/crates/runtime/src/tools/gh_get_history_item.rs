//! GitHub history *item fetch* — a single pull request, issue, commit, or
//! comparison by number / ref.
//!
//! History items are mutable (comments are added, reviews change, commits
//! land), so every REST read revalidates its stored copy (an ETag
//! conditional request: a 304 costs no body and no primary rate limit);
//! a read pinned to a commit SHA is immutable.
//!
//! Layout: this module owns the public query, validation, dispatch and the
//! response boundary (sanitize, size cap). `pull_request`/`pr_sections`,
//! `issue`, `commit` and `compare` shape each operation (`file_page` the
//! changed-file page the last two share); `filter`, `inventory` and `patch`
//! own changed files and patch windows, `window` provider-batch paging,
//! `sections` the sections a read returns, `gh_get_history_item_query` the
//! operation-independent query views,
//! `axis` the pagination axes, `remaining` what a page leaves, `pr_menu`,
//! `patch_hop` and `promotion` every `next.*`, and `graphql` the PR fast path.
use crate::response::result::{ToolData, remove_nulls};
use crate::security::scan::{ContentScan, sanitize_provider_json, sanitize_provider_text};
use octocode_github::{
    GitHubProvider, GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason,
    RequestContext,
};
use serde::Deserialize as _;
use serde_json::Value;
use std::path::Path;

/// The path the content scanner sees for history-item provider text.
const SCAN_PATH: &str = "github-history-item";

#[cfg(test)]
mod anchor_tests;
mod axis;
mod commit;
mod compare;
mod file_page;
mod filter;
mod graphql;
#[cfg(test)]
mod graphql_tests;
mod inventory;
mod issue;
mod matcher;
mod output;
mod patch;
mod patch_hop;
mod pr_finish;
mod pr_menu;
mod pr_sections;
mod promotion;
mod pull_request;
mod recovery;
mod remaining;
mod sections;
#[cfg(test)]
mod tests;
mod text;
mod util;
mod window;

/// Items per provider collection page when saved cursor state has no
/// `pageSize`. Fresh patch-free inventory pages instead size themselves to
/// the response budget; their continuations save the resulting grid.
pub(crate) const DEFAULT_PAGE_SIZE: usize =
    crate::id::gh_get_history_item_policy::COLLECTION_PAGE_SIZE;
/// Characters per body, comment or review window
/// (contract `ghGetHistoryItem.semanticPolicy.textWindowChars`).
const DEFAULT_TEXT_WINDOW: usize = crate::id::gh_get_history_item_policy::TEXT_WINDOW_CHARS;

use octocode_contract_types::GhGetHistoryItemQuery;
pub(crate) use output::Output;
use sections::Section;

/// The operation discriminant of a [`GhGetHistoryItemQuery`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemOperation {
    PullRequest,
    Issue,
    Commit,
    Compare,
}

/// A validated wire query plus runtime-only execution settings.
#[derive(Clone, Debug)]
pub struct HistoryItemRequest {
    pub query: GhGetHistoryItemQuery,
    /// The sections the row reads, resolved once from the typed query
    /// ([`Sections::resolve`]).
    sections: Sections,
    /// The response page this row's output fits: the call's explicit page
    /// (`responseChars`) or the configured automatic page, shared by the
    /// rows of one call.
    pub auto_page_chars: Option<usize>,
    /// File text at the pull request's head that widens `matchString` hunks
    /// past GitHub's diff context (read once per file and call).
    head_sources: Option<std::sync::Arc<patch::hunks::HeadSources>>,
    /// `query` as a wire row, serialized once: every continuation starts
    /// from it ([`pr_menu::base_public_query`]).
    public: Value,
    matcher: Option<std::sync::Arc<matcher::Matcher>>,
}

impl HistoryItemRequest {
    /// Parses a validated `ghGetHistoryItem` row (owned or borrowed: the
    /// typed query is read from it without a copy).
    pub fn from_row(row: impl std::borrow::Borrow<Value>) -> Result<Self, serde_json::Error> {
        let query = GhGetHistoryItemQuery::deserialize(row.borrow())?;
        Ok(Self {
            matcher: matcher::Matcher::of(&query)?,
            sections: Sections::resolve(&query),
            public: serde_json::to_value(&query)?,
            query,
            auto_page_chars: None,
            head_sources: None,
        })
    }
    /// The sections the row reads.
    pub(crate) fn sections(&self) -> Sections {
        self.sections
    }
    /// Commit/compare (and pull-request) rows that read patch text.
    pub(crate) fn include_diff(&self) -> bool {
        self.sections.has(Section::Patches)
    }
}

/// One section of a history item (`body`, `files`, `patches`, `comments`,
/// `reviewComments`, `reviews`, `commits`, `commitFiles`).
pub(crate) use sections::Sections;

impl HistoryItemRequest {
    /// This comparison with `baseRef`/`headRef` replaced (resolved commits), for
    /// continuations that must read the same two commits. A ref the wire
    /// contract rejects (an empty one) is a validation error, never a
    /// silent fall back to the unpinned ref.
    pub(super) fn with_compare_refs(&self, base: &str, head: &str) -> Result<Self, ProviderError> {
        let mut row = self.public.clone();
        row["baseRef"] = serde_json::json!(base);
        row["headRef"] = serde_json::json!(head);
        let mut pinned = Self::from_row(row).map_err(|error| validation(&error.to_string()))?;
        pinned.auto_page_chars = self.auto_page_chars;
        pinned.head_sources = self.head_sources.clone();
        Ok(pinned)
    }
}

impl std::ops::Deref for HistoryItemRequest {
    type Target = GhGetHistoryItemQuery;
    fn deref(&self) -> &GhGetHistoryItemQuery {
        &self.query
    }
}

pub(crate) mod gh_get_history_item_query;

/// Read one history item: its row (`data`, sanitized and within the
/// response budget) and the notes of what its pages leave to read.
pub async fn execute(
    provider: &GitHubProvider,
    query: &HistoryItemRequest,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<ToolData, ProviderError> {
    let result = execute_inner(provider, query, context).await;
    let mut output = match result {
        Ok(output) => output,
        Err(mut error) => {
            match error.kind {
                // A commit read of a SHA GitHub does not have (the transport
                // types its "No commit found" as a missing ref).
                ProviderErrorKind::NotFound
                    if error.reason == Some(ProviderErrorReason::RefNotFound)
                        && gh_get_history_item_query::operation(query) == ItemOperation::Commit =>
                {
                    error.message = format!(
                        "Commit \"{}\" not found in {}/{}",
                        gh_get_history_item_query::reference(query).unwrap_or_default(),
                        gh_get_history_item_query::owner(query),
                        gh_get_history_item_query::repo(query)
                    )
                    .into_boxed_str();
                }
                // The number names the other kind of item: the message
                // already says so.
                ProviderErrorKind::NotFound
                    if matches!(
                        error.reason,
                        Some(
                            ProviderErrorReason::PullRequestIsIssue
                                | ProviderErrorReason::IssueIsPullRequest
                        )
                    ) => {}
                // GitHub answers a missing item, ref, or repository with a
                // bare 404, and an unresolvable comparison ref with "No commit
                // found for SHA" even for a tag or branch: name what was asked
                // for, as refs.
                ProviderErrorKind::NotFound => {
                    error.message = not_found_message(query, error.reason).into_boxed_str();
                    if matches!(
                        gh_get_history_item_query::operation(query),
                        ItemOperation::Compare
                    ) && error.reason.is_none()
                    {
                        error.reason = Some(ProviderErrorReason::RefNotFound);
                    }
                }
                _ => {}
            }
            error.message = sanitize_provider_text(security, &error.message, Path::new(SCAN_PATH))?
                .into_boxed_str();
            return Err(error);
        }
    };
    if let Some(matcher) = &query.matcher {
        matcher.result()?;
    }
    sanitize_provider_json(security, &mut output.data, Path::new(SCAN_PATH))?;
    remove_nulls(&mut output.data);
    crate::tools::gh_shared::finish_calls(&mut output.data);
    enforce_response_limit(&output.data, context.budget.max_body_bytes)?;
    Ok(output)
}

/// Read one history item through `provider`, as every GitHub tool runs:
/// its row, or the failure the tool states ([`recovery::recover`]: the
/// shared wording, the recovery read, a missing ref's default-branch
/// retry). `row` is the query's wire row, the base of every recovery lead.
pub(crate) async fn run(
    provider: &GitHubProvider,
    query: &HistoryItemRequest,
    row: &Value,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<ToolData, crate::tools::gh_shared::GhFailure> {
    match execute(provider, query, context, security).await {
        Ok(output) => Ok(output),
        Err(error) => Err(recovery::recover(provider, row, error, context).await),
    }
}

/// The message of a not-found item read: the repository, number, or refs
/// the query named (a bare GitHub 404 cannot tell which part is missing).
fn not_found_message(query: &HistoryItemRequest, reason: Option<ProviderErrorReason>) -> String {
    let (owner, repo) = (
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
    );
    if reason == Some(ProviderErrorReason::RepositoryNotFound) {
        return format!(
            "Repository {owner}/{repo} not found, or private and not accessible to this token"
        );
    }
    let or_repository = "or the repository is missing or private to this token";
    match gh_get_history_item_query::operation(query) {
        ItemOperation::PullRequest => format!(
            "Pull request #{} not found in {owner}/{repo}, {or_repository}",
            gh_get_history_item_query::number(query).unwrap_or_default()
        ),
        ItemOperation::Issue => format!(
            "Issue #{} not found in {owner}/{repo}, {or_repository}",
            gh_get_history_item_query::number(query).unwrap_or_default()
        ),
        ItemOperation::Commit => format!(
            "Commit \"{}\" not found in {owner}/{repo}, {or_repository}",
            gh_get_history_item_query::reference(query).unwrap_or_default()
        ),
        ItemOperation::Compare => format!(
            "Cannot compare {}...{} in {owner}/{repo}: a ref (branch, tag, or SHA) was not found, {or_repository}",
            gh_get_history_item_query::base(query).unwrap_or_default(),
            gh_get_history_item_query::head(query).unwrap_or_default()
        ),
    }
}

fn enforce_response_limit(value: &Value, max_body_bytes: usize) -> Result<(), ProviderError> {
    let size = octocode_engine::json_size::bytes(value).unwrap_or(usize::MAX);
    if size > max_body_bytes {
        return Err(ProviderError::new(
            ProviderErrorKind::ResponseTooLarge,
            format!("GitHub history item response exceeds {max_body_bytes} bytes"),
        ));
    }
    Ok(())
}

async fn execute_inner(
    provider: &GitHubProvider,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    context.check()?;
    let transport = &provider.transport;
    match gh_get_history_item_query::operation(query) {
        ItemOperation::PullRequest => {
            match pull_request::pull_request(transport, query, context).await {
                Err(error) if error.status == Some(404) => {
                    Err(issue::name_issue_number(transport, query, context, error).await)
                }
                result => result,
            }
        }
        ItemOperation::Issue => issue::issue(transport, query, context).await,
        ItemOperation::Commit => commit::commit(provider, query, context).await,
        ItemOperation::Compare => compare::compare(provider, query, context).await,
    }
}

fn validation(message: &str) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Validation, message)
}

async fn fetch(
    transport: &GitHubTransport,
    segments: &[&str],
    query: &[(&str, String)],
    context: &RequestContext,
) -> Result<(Value, bool), ProviderError> {
    context.check()?;
    let response = transport.history_item(segments, query, context).await?;
    Ok((response.value, response.has_more))
}
