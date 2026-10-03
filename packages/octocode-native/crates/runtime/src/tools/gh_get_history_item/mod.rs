//! GitHub history *item fetch* — a single pull request, issue, commit, or
//! comparison by number / ref.
//!
//! **Cache bypass is intentional.**  Like `gh_search_history`, this tool calls
//! `transport` directly instead of going through the `GitHubProvider` cache
//! wrapper.  History items are mutable (comments are added, reviews change,
//! commits land): serving a cached snapshot would produce incorrect data.  The
//! GitHub API's own rate-limit and conditional-request machinery is used
//! implicitly through the transport layer.
//!
//! Layout: this module owns the public query, validation, dispatch and the
//! response boundary (sanitize, size cap). `pull_request`/`pr_sections`,
//! `issue` and `commit_compare` shape each operation; `files` owns changed
//! files and patch windows, `window` provider-batch paging, `continuations`
//! every `next.*`, and `graphql` the PR fast path.
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason,
    RequestContext,
};
use crate::security::scan::ContentScan;
use crate::tools::result::remove_nulls;
use serde_json::Value;
use std::path::Path;

mod commit_compare;
mod continuations;
mod files;
mod graphql;
mod issue;
mod pr_sections;
mod pull_request;
mod util;
mod window;

/// Items per page of a provider collection (comments, reviews, commits,
/// patch file pages) when `pageSize` is omitted: the contract default the
/// issue, commit and compare variants declare. The pull-request variant
/// declares none (an omitted patch-free inventory sizes itself to the
/// response page) and pages its collections by the same default.
pub(crate) fn default_page_size() -> usize {
    crate::contracts::query_schema_number(
        crate::tools::id::ToolId::GhGetHistoryItem,
        Some("issue"),
        "pageSize",
        "default",
    )
    .and_then(|size| usize::try_from(size).ok())
    .unwrap_or(1)
}
/// Largest page of a provider collection (GitHub's `per_page` maximum).
const MAX_COLLECTION_PAGE: usize = 100;
const DEFAULT_TEXT_WINDOW: usize = 12_000;

pub use crate::contracts::tool_types::GhGetHistoryItemQuery;

/// The operation discriminant of a [`GhGetHistoryItemQuery`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemOperation {
    PullRequest,
    Issue,
    Commit,
    Compare,
}

/// A validated wire query plus runtime-only execution settings.
#[derive(Clone, Debug)]
pub struct HistoryItemRequest {
    pub query: GhGetHistoryItemQuery,
    /// The row's `content` selector. typify projects the contract's
    /// `content.patches` anyOf as flattened optional subtypes that share
    /// `mode`, which loses selectors on a round trip, so the validated row
    /// stays authoritative for this one field.
    content: Option<Value>,
    /// Effective automatic response page (`output.pagination.defaultCharLength`).
    pub auto_page_chars: Option<usize>,
    /// The call's explicit response page (`responseCharLength`), if any.
    pub response_page: Option<usize>,
    /// Rows of this call that read patches: they share one patch budget.
    pub patch_rows: usize,
    /// Commit/compare `files` scope (paths or globs); pull requests carry
    /// theirs as `fileFilter.paths`.
    pub(super) file_scope: Vec<String>,
}

/// Runtime-only row key carrying the call's patch-reading row count. It is
/// added after validation and removed before parsing, so it never reaches
/// the wire contract or a continuation.
pub const PATCH_ROWS_KEY: &str = "\u{0}patchRows";
/// Runtime-only row key carrying the call's explicit response page
/// (`responseCharLength`), which then sizes default patch windows in place
/// of the configured automatic page. Same lifecycle as [`PATCH_ROWS_KEY`].
pub const RESPONSE_PAGE_KEY: &str = "\u{0}responsePage";

/// Whether a validated row reads patch text.
fn reads_patches(row: &Value) -> bool {
    let patches_selected = row
        .pointer("/content/patches/mode")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode != "none");
    let included = row
        .get("include")
        .and_then(Value::as_array)
        .is_some_and(|include| include.iter().any(|item| item == "patches"));
    match row.get("operation").and_then(Value::as_str) {
        Some("pullRequest") => patches_selected || included || row.get("matchString").is_some(),
        Some("commit" | "compare") => {
            included || row.get("includeDiff").and_then(Value::as_bool) == Some(true)
        }
        _ => false,
    }
}

/// Whether a validated row reads patch text without an explicit window.
fn reads_default_patch_window(row: &Value) -> bool {
    row.get("charLength").is_none() && reads_patches(row)
}

/// One patch budget per call: when two or more rows read patches with the
/// default window, each row is stamped with that count so the windows split
/// one response page instead of each taking a whole one, which would push
/// a multi-row read into response pagination. An explicit response page
/// (`responseCharLength`) is stamped on every patch-reading row, so a walk
/// that asks for larger pages gets larger patch windows (fewer hops) and an
/// explicit `charLength` is clamped to that page, not the configured one.
/// `None` leaves rows as-is.
pub fn share_patch_budget(rows: &[Value], response_page: Option<usize>) -> Option<Vec<Value>> {
    let count = rows
        .iter()
        .filter(|row| reads_default_patch_window(row))
        .count();
    let paged = response_page.is_some() && rows.iter().any(reads_patches);
    (count > 1 || paged).then(|| {
        rows.iter()
            .map(|row| {
                let mut row = row.clone();
                let shared = count > 1 && reads_default_patch_window(&row);
                let stamped = response_page.filter(|_| reads_patches(&row));
                if let Some(fields) = row.as_object_mut() {
                    if shared {
                        fields.insert(PATCH_ROWS_KEY.into(), Value::from(count));
                    }
                    if let Some(page) = stamped {
                        fields.insert(RESPONSE_PAGE_KEY.into(), Value::from(page));
                    }
                }
                row
            })
            .collect()
    })
}

impl HistoryItemRequest {
    /// Parses a validated `ghGetHistoryItem` row.
    pub fn from_row(mut row: Value) -> Result<Self, serde_json::Error> {
        let patch_rows = row
            .as_object_mut()
            .and_then(|fields| fields.remove(PATCH_ROWS_KEY))
            .and_then(|count| count.as_u64())
            .map_or(1, |count| usize::try_from(count).unwrap_or(1).max(1));
        let response_page = row
            .as_object_mut()
            .and_then(|fields| fields.remove(RESPONSE_PAGE_KEY))
            .and_then(|page| page.as_u64())
            .and_then(|page| usize::try_from(page).ok())
            .filter(|page| *page > 0);
        let file_scope = normalize_aliases(&mut row);
        imply_patch_search(&mut row);
        let content = row.get("content").cloned();
        Ok(Self {
            query: serde_json::from_value(row)?,
            content,
            auto_page_chars: None,
            response_page,
            patch_rows,
            file_scope,
        })
    }
    /// The `content` selector as JSON, for the shaping code's key lookups.
    pub fn content_value(&self) -> Option<Value> {
        self.content.clone()
    }
}

/// The flat read selectors (`include`, `files`, `status`, commit `base`)
/// map onto the nested shapes every handler reads (`content.*`,
/// `fileFilter`, `includeDiff`, `operation:"compare"`), so the old and the
/// new spellings of one read run the same code. Returns the commit/compare
/// `files` scope, which has no nested spelling.
fn normalize_aliases(row: &mut Value) -> Vec<String> {
    let Some(fields) = row.as_object_mut() else {
        return Vec::new();
    };
    let strings = |value: Option<Value>| {
        value
            .and_then(|value| match value {
                Value::Array(items) => Some(items),
                _ => None,
            })
            .unwrap_or_default()
            .into_iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let include = strings(fields.remove("include"));
    let files = strings(fields.remove("files"));
    let status = fields.remove("status");
    let operation = fields
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match operation.as_str() {
        "pullRequest" | "issue" => {
            let had_content = fields.contains_key("content");
            let content = fields
                .entry("content")
                .or_insert_with(|| serde_json::json!({}));
            if let Some(content) = content.as_object_mut() {
                for item in &include {
                    match item.as_str() {
                        "body" => {
                            content.insert("body".into(), Value::Bool(true));
                        }
                        "files" => {
                            content.insert("changedFiles".into(), Value::Bool(true));
                        }
                        "patches" => {
                            content
                                .entry("patches")
                                .or_insert_with(|| serde_json::json!({"mode":"all"}));
                        }
                        "comments" => {
                            let comments = content
                                .entry("comments")
                                .or_insert_with(|| serde_json::json!({}));
                            if let Some(comments) = comments.as_object_mut() {
                                comments.insert("discussion".into(), Value::Bool(true));
                                if operation == "pullRequest" {
                                    comments.insert("reviewInline".into(), Value::Bool(true));
                                }
                            }
                        }
                        "reviews" => {
                            content.insert("reviews".into(), Value::Bool(true));
                        }
                        "commits" => {
                            content
                                .entry("commits")
                                .or_insert_with(|| serde_json::json!({}));
                        }
                        _ => {}
                    }
                }
            }
            if !had_content
                && fields
                    .get("content")
                    .and_then(Value::as_object)
                    .is_some_and(serde_json::Map::is_empty)
            {
                fields.remove("content");
            }
            if operation == "pullRequest" && (!files.is_empty() || status.is_some()) {
                let filter = fields
                    .entry("fileFilter")
                    .or_insert_with(|| serde_json::json!({}));
                if let Some(filter) = filter.as_object_mut() {
                    if !files.is_empty() {
                        let paths = filter
                            .entry("paths")
                            .or_insert_with(|| serde_json::json!([]));
                        if let Some(paths) = paths.as_array_mut() {
                            for file in &files {
                                if !paths.iter().any(|path| path == file.as_str()) {
                                    paths.push(Value::String(file.clone()));
                                }
                            }
                        }
                    }
                    if let Some(status) = status {
                        filter.insert("status".into(), status);
                    }
                }
            }
            Vec::new()
        }
        "commit" | "compare" => {
            if include.iter().any(|item| item == "patches") {
                fields.insert("includeDiff".into(), Value::Bool(true));
            }
            // A commit with a base is the comparison base...ref.
            if operation == "commit"
                && let Some(base) = fields.remove("base")
            {
                fields.insert("operation".into(), Value::from("compare"));
                if let Some(head) = fields.remove("ref") {
                    fields.insert("head".into(), head);
                }
                fields.insert("base".into(), base);
                fields.entry("page").or_insert_with(|| Value::from(1));
            }
            files
        }
        _ => Vec::new(),
    }
}

/// `matchString` on a pull request filters its patches (and selected
/// comments and reviews). With no content selected there is nothing for it
/// to filter, so the literal implies a search of every patch, still narrowed
/// by `fileFilter`.
fn imply_patch_search(row: &mut Value) {
    let selects_nothing = row
        .get("content")
        .is_none_or(|content| content.as_object().is_some_and(serde_json::Map::is_empty));
    if row.get("operation").and_then(Value::as_str) == Some("pullRequest")
        && row.get("matchString").and_then(Value::as_str).is_some()
        && selects_nothing
    {
        row["content"] = serde_json::json!({"patches": {"mode": "all"}});
    }
}

impl HistoryItemRequest {
    /// This comparison with `base`/`head` replaced (resolved commits), for
    /// continuations that must read the same two commits.
    pub(super) fn with_compare_refs(&self, base: &str, head: &str) -> Self {
        let mut pinned = self.clone();
        if let GhGetHistoryItemQuery::Compare {
            base: pinned_base,
            head: pinned_head,
            ..
        } = &mut pinned.query
        {
            base.clone_into(pinned_base);
            head.clone_into(pinned_head);
        }
        pinned
    }
}

impl std::ops::Deref for HistoryItemRequest {
    type Target = GhGetHistoryItemQuery;
    fn deref(&self) -> &GhGetHistoryItemQuery {
        &self.query
    }
}

fn usize_of(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// Operation-independent views over the generated wire query, in the
/// engine's `usize` units.
impl GhGetHistoryItemQuery {
    pub fn operation(&self) -> ItemOperation {
        match self {
            Self::PullRequest { .. } => ItemOperation::PullRequest,
            Self::Issue { .. } => ItemOperation::Issue,
            Self::Commit { .. } => ItemOperation::Commit,
            Self::Compare { .. } => ItemOperation::Compare,
        }
    }
    pub fn owner(&self) -> &str {
        match self {
            Self::PullRequest { owner, .. }
            | Self::Issue { owner, .. }
            | Self::Commit { owner, .. }
            | Self::Compare { owner, .. } => owner.as_str(),
        }
    }
    pub fn repo(&self) -> &str {
        match self {
            Self::PullRequest { repo, .. }
            | Self::Issue { repo, .. }
            | Self::Commit { repo, .. }
            | Self::Compare { repo, .. } => repo.as_str(),
        }
    }
    pub fn number(&self) -> Option<u64> {
        match self {
            Self::PullRequest { number, .. } | Self::Issue { number, .. } => Some(number.0.get()),
            _ => None,
        }
    }
    pub fn reference(&self) -> Option<&str> {
        match self {
            Self::Commit { ref_, .. } => Some(ref_.as_str()),
            _ => None,
        }
    }
    pub fn base(&self) -> Option<&str> {
        match self {
            Self::Compare { base, .. } => Some(base),
            _ => None,
        }
    }
    pub fn head(&self) -> Option<&str> {
        match self {
            Self::Compare { head, .. } => Some(head),
            _ => None,
        }
    }
    pub fn page(&self) -> Option<usize> {
        match self {
            Self::Compare { page, .. } => Some(usize_of(page.get())),
            _ => None,
        }
    }
    pub fn page_size(&self) -> Option<usize> {
        match self {
            Self::PullRequest { page_size, .. } => page_size.map(|size| usize_of(size.get())),
            Self::Issue { page_size, .. }
            | Self::Commit { page_size, .. }
            | Self::Compare { page_size, .. } => {
                page_size.as_ref().map(|size| usize_of(size.0.get()))
            }
        }
    }
    /// Items per page of a provider collection (comments, reviews, commits,
    /// patch file pages): `pageSize` capped at one provider batch.
    pub fn collection_page_size(&self) -> usize {
        self.page_size()
            .unwrap_or_else(default_page_size)
            .clamp(1, MAX_COLLECTION_PAGE)
    }
    /// The pull-request changed-file narrowing (`fileFilter`).
    pub fn file_filter(&self) -> Option<&crate::contracts::tool_types::PullRequestFileFilter> {
        match self {
            Self::PullRequest { file_filter, .. } => file_filter.as_ref(),
            _ => None,
        }
    }
    pub fn file_page(&self) -> Option<usize> {
        match self {
            Self::PullRequest { file_page, .. } => file_page.map(|page| usize_of(page.get())),
            Self::Commit { file_page, .. } | Self::Compare { file_page, .. } => {
                file_page.as_ref().map(|page| usize_of(page.0.get()))
            }
            Self::Issue { .. } => None,
        }
    }
    pub fn comment_page(&self) -> Option<usize> {
        match self {
            Self::PullRequest { comment_page, .. } | Self::Issue { comment_page, .. } => {
                comment_page.as_ref().map(|page| usize_of(page.0.get()))
            }
            _ => None,
        }
    }
    pub fn commit_page(&self) -> Option<usize> {
        match self {
            Self::PullRequest { commit_page, .. } => commit_page.map(|page| usize_of(page.get())),
            _ => None,
        }
    }
    pub fn review_page(&self) -> Option<usize> {
        match self {
            Self::PullRequest { review_page, .. } => review_page.map(|page| usize_of(page.get())),
            _ => None,
        }
    }
    pub fn include_diff(&self) -> bool {
        match self {
            Self::Commit { include_diff, .. } | Self::Compare { include_diff, .. } => {
                include_diff.as_ref().is_some_and(|include| include.0)
            }
            _ => false,
        }
    }
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Commit { path, .. } | Self::Compare { path, .. } => {
                path.as_ref().map(|path| path.as_str())
            }
            _ => None,
        }
    }
    pub fn char_offset(&self) -> Option<usize> {
        match self {
            Self::PullRequest { char_offset, .. } | Self::Issue { char_offset, .. } => {
                char_offset.map(usize_of)
            }
            Self::Commit { char_offset, .. } | Self::Compare { char_offset, .. } => {
                char_offset.as_ref().map(|offset| usize_of(offset.0))
            }
        }
    }
    pub fn char_length(&self) -> Option<usize> {
        match self {
            Self::PullRequest { char_length, .. } | Self::Issue { char_length, .. } => {
                char_length.map(|length| usize_of(length.get()))
            }
            Self::Commit { char_length, .. } | Self::Compare { char_length, .. } => {
                char_length.as_ref().map(|length| usize_of(length.0.get()))
            }
        }
    }
    pub fn match_string(&self) -> Option<&str> {
        match self {
            Self::PullRequest { match_string, .. } => match_string.as_deref(),
            _ => None,
        }
    }
    /// Lines kept around each `matchString` hit in a patch (`matchContext`).
    pub fn match_context(&self) -> Option<usize> {
        match self {
            Self::PullRequest { match_context, .. } => {
                match_context.and_then(|lines| usize::try_from(lines).ok())
            }
            _ => None,
        }
    }
    pub fn comment_body_offset(&self) -> Option<usize> {
        match self {
            Self::PullRequest {
                comment_body_offset,
                ..
            } => comment_body_offset.map(usize_of),
            _ => None,
        }
    }
    /// Whether the query reads a later page of a pull request (a file, commit,
    /// review or comment page after the first, or a text window past the
    /// start): the caller already holds the item header and menu from page one.
    pub fn later_page(&self) -> bool {
        let after_first = |page: Option<usize>| page.is_some_and(|page| page > 1);
        matches!(self, Self::PullRequest { .. })
            && (after_first(self.file_page())
                || after_first(self.comment_page())
                || after_first(self.commit_page())
                || after_first(self.review_page())
                || self.char_offset().is_some_and(|offset| offset > 0)
                || self.comment_body_offset().is_some_and(|offset| offset > 0))
    }
    /// `debug: true` keeps diagnostic fields a default response omits.
    pub fn debug(&self) -> bool {
        match self {
            Self::PullRequest { debug, .. } => *debug,
            _ => false,
        }
    }
    /// The pull-request text view (`"standard"` or `"none"`).
    pub fn minify(&self) -> Option<String> {
        match self {
            Self::PullRequest { minify, .. } => Some(minify.to_string()),
            _ => None,
        }
    }
}

pub async fn execute<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Value, ProviderError> {
    let result = execute_inner(transport, query, context).await;
    let mut value = match result {
        Ok(value) => value,
        Err(mut error) => {
            match error.kind {
                // The number names an issue: the message already says so.
                ProviderErrorKind::NotFound
                    if error.reason == Some(ProviderErrorReason::PullRequestIsIssue) => {}
                ProviderErrorKind::NotFound => {
                    let canonical = "Repository, resource, or path not found";
                    error.message = if matches!(query.operation(), ItemOperation::PullRequest) {
                        format!(
                            "Failed to fetch pull request #{}: {canonical}",
                            query.number().unwrap_or_default()
                        )
                        .into_boxed_str()
                    } else {
                        canonical.into()
                    };
                }
                ProviderErrorKind::RateLimited => {
                    if let Some(rate_limit) = error.rate_limit.as_mut()
                        && rate_limit.remaining.is_none()
                    {
                        rate_limit.remaining = Some(0);
                    }
                }
                _ => {}
            }
            error.message = sanitize_text(error.message.as_ref(), security)?.into_boxed_str();
            return Err(error);
        }
    };
    sanitize_all_strings(&mut value, security)?;
    remove_nulls(&mut value);
    enforce_response_limit(&value, context.max_body_bytes)?;
    Ok(value)
}

fn enforce_response_limit(value: &Value, max_body_bytes: usize) -> Result<(), ProviderError> {
    let size = serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX);
    if size > max_body_bytes {
        return Err(ProviderError::new(
            ProviderErrorKind::ResponseTooLarge,
            format!("GitHub history item response exceeds {max_body_bytes} bytes"),
        ));
    }
    Ok(())
}

async fn execute_inner<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    validate(query)?;
    check_context(context)?;
    match query.operation() {
        ItemOperation::PullRequest => {
            match pull_request::pull_request(transport, query, context).await {
                Err(error) if error.status == Some(404) => {
                    Err(issue::name_issue_number(transport, query, context, error).await)
                }
                result => result,
            }
        }
        ItemOperation::Issue => issue::issue(transport, query, context).await,
        ItemOperation::Commit => commit_compare::commit(transport, query, context).await,
        ItemOperation::Compare => commit_compare::compare(transport, query, context).await,
    }
}

fn validate(query: &HistoryItemRequest) -> Result<(), ProviderError> {
    if query.owner().is_empty() || query.repo().is_empty() {
        return Err(validation("owner and repo are required"));
    }
    match query.operation() {
        ItemOperation::PullRequest | ItemOperation::Issue if query.number().is_none() => {
            Err(validation("number is required"))
        }
        ItemOperation::Commit if query.reference().is_none_or(str::is_empty) => {
            Err(validation("ref is required"))
        }
        ItemOperation::Compare
            if query.base().is_none_or(str::is_empty) || query.head().is_none_or(str::is_empty) =>
        {
            Err(validation("base and head are required"))
        }
        _ => Ok(()),
    }
}

fn validation(message: &str) -> ProviderError {
    ProviderError::new(ProviderErrorKind::Validation, message)
}

fn check_context(context: &RequestContext) -> Result<(), ProviderError> {
    if context.cancellation.is_cancelled() {
        Err(ProviderError::new(
            ProviderErrorKind::Cancelled,
            "request cancelled",
        ))
    } else if std::time::Instant::now() >= context.deadline {
        Err(ProviderError::new(
            ProviderErrorKind::Timeout,
            "request timed out",
        ))
    } else {
        Ok(())
    }
}

async fn fetch<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    segments: &[&str],
    query: &[(&str, String)],
    context: &RequestContext,
) -> Result<(Value, bool), ProviderError> {
    check_context(context)?;
    let response = transport.history_item(segments, query, context).await?;
    Ok((response.value, response.has_more))
}

fn sanitize_text(value: &str, security: &impl ContentScan) -> Result<String, ProviderError> {
    security
        .sanitize(value, Path::new("github-history-item"))
        .map(|v| v.0)
        .map_err(|(m, _)| ProviderError::new(ProviderErrorKind::Validation, m))
}
fn sanitize_all_strings(
    value: &mut Value,
    security: &impl ContentScan,
) -> Result<(), ProviderError> {
    crate::security::sanitize_json(value, &mut |text| sanitize_text(text, security))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn false_content_selectors_do_not_request_sections() {
        let query = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest", "owner":"o", "repo":"r", "number":1,
            "mainGoal":"Read the pull request.","reasoning":"False selectors must stay off.",
            "content":{
                "body":false,"changedFiles":false,"reviews":false,
                "comments":{"discussion":false,"reviewInline":false,"includeBots":false}
            }
        }))
        .expect("false selectors are valid booleans");
        let wants = pull_request::content_wants(&query);
        assert!(
            !wants.body && !wants.files && !wants.discussion && !wants.reviews && !wants.commits
        );
    }

    /// The flat selectors and the nested shapes they replace
    /// parse to the same request, so old continuations keep working.
    #[test]
    fn flat_selectors_alias_the_nested_shapes() {
        let flat = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":1,
            "include":["body","files","patches","comments","reviews"],
            "files":["src/**"],"status":["modified"]
        }))
        .expect("flat shape");
        let nested = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":1,
            "content":{"body":true,"changedFiles":true,"patches":{"mode":"all"},
                "comments":{"discussion":true,"reviewInline":true},"reviews":true},
            "fileFilter":{"paths":["src/**"],"status":["modified"]}
        }))
        .expect("nested shape");
        assert_eq!(flat.content_value(), nested.content_value());
        assert_eq!(
            serde_json::to_value(&flat.query).ok(),
            serde_json::to_value(&nested.query).ok()
        );
        // Commit include patches = includeDiff; base folds compare in.
        let commit = HistoryItemRequest::from_row(json!({
            "operation":"commit","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "ref":"head-sha","base":"base-sha","include":["patches"],"files":["src/*.rs"]
        }))
        .expect("commit with base");
        assert_eq!(commit.operation(), ItemOperation::Compare);
        assert_eq!(commit.base(), Some("base-sha"));
        assert_eq!(commit.head(), Some("head-sha"));
        assert!(commit.include_diff());
        assert_eq!(commit.file_scope, ["src/*.rs"]);
        let issue = HistoryItemRequest::from_row(json!({
            "operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":2,
            "include":["comments"]
        }))
        .expect("issue include");
        assert_eq!(
            issue.content_value(),
            Some(json!({"comments":{"discussion":true}}))
        );
    }

    struct ReplacingScan;
    impl ContentScan for ReplacingScan {
        fn sanitize(
            &self,
            text: &str,
            _path: &Path,
        ) -> Result<(String, Vec<String>), (String, String)> {
            Ok((text.replace("secret", "[MASKED]"), Vec::new()))
        }
    }

    #[test]
    fn missing_identity_is_rejected() {
        // The wire contract requires each operation's identity.
        for row in [
            json!({"operation":"commit","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b"}),
            json!({"operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b"}),
            json!({"operation":"compare","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","base":"x"}),
        ] {
            assert!(HistoryItemRequest::from_row(row).is_err());
        }
        let committed = HistoryItemRequest::from_row(
            json!({"operation":"commit","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"x"}),
        )
        .expect("GitHub history test data should be valid");
        assert!(validate(&committed).is_ok());
        let blank = HistoryItemRequest::from_row(json!({
            "operation":"compare","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","base":"","head":"x"
        }))
        .expect("GitHub history test data should be valid");
        assert!(validate(&blank).is_err());
    }

    #[test]
    fn sanitizes_every_nested_returned_string() {
        let mut value = json!({
            "title": "secret",
            "nested": [{"body": "a secret value"}],
            "next": {"tool": "secret-tool", "query": {"path": "secret.rs"}}
        });
        sanitize_all_strings(&mut value, &ReplacingScan)
            .expect("GitHub history test data should be valid");
        assert_eq!(value["title"], "[MASKED]");
        assert_eq!(value["nested"][0]["body"], "a [MASKED] value");
        // The executable `tool` identifier survives verbatim, but the query
        // leaves are scanned: redaction fires only on a real
        // secret, so a legitimate path is untouched while a secret is masked.
        assert_eq!(value["next"]["tool"], "secret-tool");
        assert_eq!(value["next"]["query"]["path"], "[MASKED].rs");
    }

    #[test]
    fn cancellation_is_observed_before_provider_work() {
        let context = RequestContext::with_timeout(std::time::Duration::from_secs(1), 1024);
        context.cancellation.cancel();
        let error =
            check_context(&context).expect_err("GitHub history operation should fail in this test");
        assert_eq!(error.kind, ProviderErrorKind::Cancelled);
        assert_eq!(error.message.as_ref(), "request cancelled");
    }

    #[test]
    fn normalized_response_budget_is_enforced() {
        let error = enforce_response_limit(&json!({"body": "abcdefgh"}), 4)
            .expect_err("GitHub history operation should fail in this test");
        assert_eq!(error.kind, ProviderErrorKind::ResponseTooLarge);
        assert_eq!(
            error.message.as_ref(),
            "GitHub history item response exceeds 4 bytes"
        );
    }
}
