//! GitHub history *item fetch* — a single pull request, issue, commit, or
//! comparison by number / ref.
//!
//! History items are mutable (comments are added, reviews change, commits
//! land), so every read fetches the item afresh; a read pinned to a
//! commit SHA is immutable.
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
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use octocode_runtime_base::response::result::{ToolData, remove_nulls};
use octocode_runtime_base::security::scan::{
    ContentScan, sanitize_provider_json, sanitize_provider_text,
};
use serde::Deserialize as _;
use serde_json::Value;
use std::path::Path;

/// The path the content scanner sees for history-item provider text.
const SCAN_PATH: &str = "github-history-item";

#[cfg(test)]
mod anchor_tests;
mod axis;
mod body_match;
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
pub mod output;
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
mod source;
#[cfg(test)]
mod tests;
mod text;
mod util;
mod window;

/// Items per provider collection page when saved cursor state has no
/// `pageSize`. Fresh patch-free inventory pages instead size themselves to
/// the response budget; their continuations save the resulting grid.
pub(crate) const DEFAULT_PAGE_SIZE: usize =
    octocode_runtime_base::id::gh_get_history_item_policy::COLLECTION_PAGE_SIZE;
/// Characters per body, comment or review window
/// (contract `ghGetHistoryItem.semanticPolicy.textWindowChars`).
const DEFAULT_TEXT_WINDOW: usize =
    octocode_runtime_base::id::gh_get_history_item_policy::TEXT_WINDOW_CHARS;

use octocode_contract_types::GhGetHistoryItemQuery;
pub use output::Output;
use sections::Section;
use source::Source;

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
    /// The runtime's path policy, which decides the head files a widening may
    /// read ([`Self::withholds_file`]); `None`: the default policy.
    pub paths: Option<std::sync::Arc<octocode_runtime_base::policy::path::PathPolicy>>,
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
            paths: None,
        })
    }
    /// Whether the path policy withholds the text of the repository file
    /// `path` from this row ([`octocode_runtime_base::policy::path::PathPolicy::withholds_repository_file`]),
    /// under the default policy when the runtime set none.
    pub(crate) fn withholds_file(&self, path: &str) -> bool {
        match &self.paths {
            Some(paths) => paths.withholds_repository_file(path),
            None => octocode_runtime_base::policy::discovery::is_sensitive_repository_file(path),
        }
    }
    /// Whether the policy withholds the text of a changed file, by its path
    /// or a rename's previous path ([`octocode_runtime_base::policy::path::PathPolicy::withheld_text`]).
    pub(crate) fn withheld_text(&self, path: &str, previous: Option<&str>) -> bool {
        match &self.paths {
            Some(paths) => paths.withheld_text(path, previous, None),
            None => {
                octocode_runtime_base::policy::path::withheld_text_by_default(path, previous, None)
            }
        }
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
        pinned.paths = self.paths.clone();
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
    security: &(impl ContentScan + Sync),
) -> Result<ToolData, ProviderError> {
    let withholds = |path: &str, previous: Option<&str>| query.withheld_text(path, previous);
    let source = Source::new(&provider.transport, security).withholding(&withholds);
    let result = execute_inner(provider, &source, query, context).await;
    let mut output = match result {
        Ok(output) => output,
        Err(mut error) => {
            match error.kind {
                // A commit read of a SHA GitHub does not have (the transport
                // types its "No commit found" as a missing ref).
                ProviderErrorKind::NotFound
                    if error.reason == Some(ProviderErrorReason::RefNotFound)
                        && let gh_get_history_item_query::Subject::Commit(reference) =
                            gh_get_history_item_query::subject(query) =>
                {
                    error.message = format!(
                        "Commit \"{reference}\" not found in {}/{}",
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
                ProviderErrorKind::NotFound if bare_not_found(&error, query) => {
                    let proved = context.repository_resolves(
                        gh_get_history_item_query::owner(query),
                        gh_get_history_item_query::repo(query),
                    ) || context
                        .read_full_name(
                            gh_get_history_item_query::owner(query),
                            gh_get_history_item_query::repo(query),
                        )
                        .is_some();
                    error.message = not_found_message(query, error.reason, proved).into_boxed_str();
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
    // The item's leads were built as it was fetched, under the name the
    // query gave: a repository renamed since is named as it is now, on the
    // row and in each of them.
    let asked = (
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
    );
    let renamed = octocode_tools_shared::gh_shared::name_canonically(
        provider,
        &mut output.data,
        asked.0,
        asked.1,
        context,
    )
    .await?;
    let (owner, repo) = renamed
        .as_ref()
        .map_or(asked, |(owner, repo)| (owner.as_str(), repo.as_str()));
    let identify = |row: &mut Value| {
        row["owner"] = Value::from(owner);
        row["repo"] = Value::from(repo);
    };
    let collection = match gh_get_history_item_query::operation(query) {
        ItemOperation::PullRequest => Some("pullRequests"),
        ItemOperation::Issue => Some("issues"),
        ItemOperation::Commit | ItemOperation::Compare => None,
    };
    if let Some(collection) = collection {
        if let Some(rows) = output
            .data
            .get_mut(collection)
            .and_then(Value::as_array_mut)
        {
            rows.iter_mut().for_each(identify);
        }
    } else {
        identify(&mut output.data);
    }
    // The row is redacted here, before the output gate sees it: mark it,
    // as the gate marks what it redacts.
    let added = sanitize_provider_json(security, &mut output.data, Path::new(SCAN_PATH))?;
    if added + source.redacted() > 0
        && octocode_runtime_base::security::placeholders_in(&output.data) > 0
    {
        octocode_runtime_base::security::mark_redacted(&mut output.data);
    }
    publish_section_pages(&mut output.data);
    remove_nulls(&mut output.data);
    octocode_runtime_base::response::result::finish_calls(&mut output.data);
    enforce_response_limit(&output.data, context.budget.max_body_bytes)?;
    Ok(output)
}

/// Every row's `sectionPagination` as the caller reads it: each block states
/// where its section's page stands, and the call that reads on rides the
/// row's `next.<section>`. What shaping kept in a block (its cursor, a
/// provider limit, a terminal mark) leaves it: a terminal mark is restated
/// on the row ([`window::lift_page_limits`]). Rows: the commit or comparison
/// itself, each pull request and its commits, each issue.
fn publish_section_pages(data: &mut Value) {
    publish_row_pages(data);
    for collection in ["pullRequests", "issues"] {
        for row in data
            .get_mut(collection)
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            publish_row_pages(row);
            for commit in row
                .get_mut("commits")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                publish_row_pages(commit);
            }
        }
    }
}

fn publish_row_pages(row: &mut Value) {
    let (mut calls, mut limited) = (Vec::new(), Vec::new());
    for (section, block) in row
        .get_mut("sectionPagination")
        .and_then(Value::as_object_mut)
        .into_iter()
        .flatten()
    {
        let Some(fields) = block.as_object_mut() else {
            continue;
        };
        if let Some(call) = fields
            .remove("next")
            .and_then(|mut next| next.get_mut("nextPage").map(Value::take))
        {
            calls.push((section.clone(), call));
        }
        let limit = fields.remove("providerLimit");
        if fields.remove("terminalLimit") == Some(Value::Bool(true)) {
            limited.push(serde_json::json!({"terminalLimit": true, "providerLimit": limit}));
        }
        for cursor in ["nextPage", "nextOffset"] {
            fields.remove(cursor);
        }
    }
    for mut page in limited {
        window::lift_page_limits(row, &mut page);
    }
    for (section, call) in calls {
        octocode_runtime_base::response::collection::relocate_walk(
            row,
            octocode_runtime_base::id::ToolId::GhGetHistoryItem,
            &section,
            call,
        );
    }
}

/// Read one history item through `provider`, as every GitHub tool runs:
/// its row, or the failure the tool states ([`recovery::recover`]: the
/// shared wording, the recovery read, a missing ref's default-branch
/// retry). `row` is the query's wire row, the base of every recovery lead.
pub async fn run(
    provider: &GitHubProvider,
    query: &HistoryItemRequest,
    row: &Value,
    context: &RequestContext,
    security: &(impl ContentScan + Sync),
) -> Result<ToolData, octocode_tools_shared::gh_shared::GhFailure> {
    match execute(provider, query, context, security).await {
        Ok(output) => Ok(output),
        Err(error) => Err(recovery::recover(provider, row, error, context).await),
    }
}

/// The clause a not-found item message ends with while the repository
/// itself is unproven: a bare GitHub 404 cannot say which part is missing.
pub(crate) const OR_REPOSITORY: &str = "or the repository is missing or private to this token";

/// Whether `error` is the bare 404 of an item read, which
/// [`not_found_message`] words: not a commit's missing ref or a number that
/// names the other kind of item, whose messages already say what is missing.
fn bare_not_found(error: &ProviderError, query: &HistoryItemRequest) -> bool {
    error.kind == ProviderErrorKind::NotFound
        && !matches!(
            error.reason,
            Some(ProviderErrorReason::PullRequestIsIssue | ProviderErrorReason::IssueIsPullRequest)
        )
        && !(error.reason == Some(ProviderErrorReason::RefNotFound)
            && gh_get_history_item_query::operation(query) == ItemOperation::Commit)
}

/// The message of a not-found item read: the repository, number, or refs
/// the query named (a bare GitHub 404 cannot tell which part is missing).
/// `repository_proved`: GitHub answered a read of the repository, so only
/// the item can be missing and the [`OR_REPOSITORY`] clause is left out.
fn not_found_message(
    query: &HistoryItemRequest,
    reason: Option<ProviderErrorReason>,
    repository_proved: bool,
) -> String {
    let (owner, repo) = (
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
    );
    if reason == Some(ProviderErrorReason::RepositoryNotFound) {
        return format!(
            "Repository {owner}/{repo} not found, or private and not accessible to this token"
        );
    }
    use gh_get_history_item_query::Subject;
    let item = match gh_get_history_item_query::subject(query) {
        Subject::PullRequest(number) => {
            format!("Pull request #{number} not found in {owner}/{repo}")
        }
        Subject::Issue(number) => format!("Issue #{number} not found in {owner}/{repo}"),
        Subject::Commit(reference) => {
            format!("Commit \"{reference}\" not found in {owner}/{repo}")
        }
        Subject::Compare { base, head } => format!(
            "Cannot compare {base}...{head} in {owner}/{repo}: a ref (branch, tag, or SHA) was not found"
        ),
    };
    if repository_proved {
        item
    } else {
        format!("{item}, {OR_REPOSITORY}")
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
    source: &Source<'_>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    context.check()?;
    match gh_get_history_item_query::operation(query) {
        ItemOperation::PullRequest => {
            match pull_request::pull_request(source, query, context).await {
                Err(error) if error.status == Some(404) => {
                    Err(issue::name_issue_number(source, query, context, error).await)
                }
                result => result,
            }
        }
        ItemOperation::Issue => issue::issue(source, query, context).await,
        ItemOperation::Commit => commit::commit(provider, source, query, context).await,
        ItemOperation::Compare => compare::compare(provider, source, query, context).await,
    }
}

fn validation(message: &str) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Validation, message)
}

/// One provider read of the history item: its payload with every `body`
/// already redacted ([`Source::redact_bodies`]) and whether more pages follow.
async fn fetch(
    source: &Source<'_>,
    segments: &[&str],
    query: &[(&str, String)],
    context: &RequestContext,
) -> Result<(Value, bool), ProviderError> {
    let (value, has_more, _) = fetch_linked(source, segments, query, context).await?;
    Ok((value, has_more))
}

/// [`fetch`] with the last page number GitHub's `Link` header names, when it
/// names one: the list's batch count at this `per_page`.
async fn fetch_linked(
    source: &Source<'_>,
    segments: &[&str],
    query: &[(&str, String)],
    context: &RequestContext,
) -> Result<(Value, bool, Option<usize>), ProviderError> {
    context.check()?;
    let mut response = source.history_item(segments, query, context).await?;
    source.redact_bodies(&mut response.value)?;
    Ok((response.value, response.has_more, response.last_page))
}
