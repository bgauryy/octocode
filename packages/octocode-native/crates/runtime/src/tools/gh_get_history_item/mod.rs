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
//! `issue` and `commit_compare` shape each operation; `filter`, `inventory` and
//! `patch` own changed files and patch windows, `window` provider-batch paging, `pr_menu`,
//! `patch_hop` and `promotion` every `next.*`, and `graphql` the PR fast path.
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason,
    RequestContext,
};
use crate::security::scan::ContentScan;
use crate::tools::num::usize_of;
use crate::tools::result::remove_nulls;
use serde_json::Value;
use std::path::Path;

mod commit_compare;
mod filter;
mod graphql;
mod inventory;
mod issue;
mod output;
mod patch;
mod patch_hop;
mod pr_menu;
mod pr_sections;
mod promotion;
mod pull_request;
mod recovery;
mod util;
mod window;

/// Items per page of a provider collection (comments, reviews, commits,
/// patch file pages) when `pageSize` is omitted: the contract default the
/// issue, commit and compare variants declare. The pull-request variant
/// declares none (an omitted patch-free inventory sizes itself to the
/// response page) and pages its collections by the same default.
pub(crate) const DEFAULT_PAGE_SIZE: usize =
    crate::tools::id::query_limits::gh_get_history_item::issue::PAGE_SIZE_DEFAULT;
/// Largest page of a provider collection (GitHub's `per_page` maximum).
const MAX_COLLECTION_PAGE: usize = 100;
const DEFAULT_TEXT_WINDOW: usize = 12_000;

pub use crate::contracts::tool_types::GhGetHistoryItemQuery;
pub(crate) use output::Output;
pub use recovery::attach_recovery;

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
    /// The sections the row reads, as the nested selector every handler
    /// reads (`body`, `files`, `patches{mode,files,ranges}`,
    /// `comments{discussion,reviewInline,includeBots}`, `reviews`,
    /// `commits{includeFiles}`); built from `sections`, `patchRanges` and
    /// `includeBots` ([`selection`]). Runtime-only, never on the wire.
    content: Option<Value>,
    /// Commit/compare rows that read patch text (`sections:["patches"]`).
    include_diff: bool,
    /// The response page this row's output fits: the call's explicit page
    /// (`responseLength`) or the configured automatic page, shared by the
    /// rows of one call.
    pub auto_page_chars: Option<usize>,
    /// Commit/compare `include` scope (paths or globs); pull requests
    /// filter their changed files by `include` ([`filter::InventoryFilter`]).
    pub(super) file_scope: Vec<String>,
    /// File text at the pull request's head that widens `matchString` hunks
    /// past GitHub's diff context (read once per file and call).
    head_sources: Option<std::sync::Arc<patch::HeadSources>>,
}

impl HistoryItemRequest {
    /// Parses a validated `ghGetHistoryItem` row.
    pub fn from_row(mut row: Value) -> Result<Self, serde_json::Error> {
        let (content, include_diff, file_scope) = selection(&mut row);
        Ok(Self {
            query: serde_json::from_value(row)?,
            content,
            include_diff,
            auto_page_chars: None,
            file_scope,
            head_sources: None,
        })
    }
    /// The `content` selector as JSON, for the shaping code's key lookups.
    pub fn content_value(&self) -> Option<Value> {
        self.content.clone()
    }
    /// Commit/compare rows that read patch text.
    pub fn include_diff(&self) -> bool {
        self.include_diff
    }
}

/// The sections a row reads, as the nested selector the handlers share
/// (internal keys `body`, `files`, `patches`, `comments`, `reviews`,
/// `commits`):
/// pull-request and issue `sections`/`patchRanges`/`includeBots` become
/// `content`; commit/compare `sections:["patches"]` reads patch text and
/// `include` scopes the files. A commit with a `base` is the comparison
/// base...ref. Returns `(content, include_diff, file_scope)`.
fn selection(row: &mut Value) -> (Option<Value>, bool, Vec<String>) {
    let Some(fields) = row.as_object_mut() else {
        return (None, false, Vec::new());
    };
    let strings = |value: Option<&Value>| {
        value
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect::<Vec<_>>()
    };
    let sections = strings(fields.get("sections"));
    let operation = fields
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match operation.as_str() {
        "pullRequest" | "issue" => {
            let mut content = serde_json::Map::new();
            let mut comments = serde_json::Map::new();
            for section in &sections {
                match section.as_str() {
                    "body" => {
                        content.insert("body".into(), Value::Bool(true));
                    }
                    "files" => {
                        content.insert("files".into(), Value::Bool(true));
                    }
                    "patches" => {
                        content
                            .entry("patches")
                            .or_insert_with(|| serde_json::json!({"mode":"all"}));
                    }
                    "comments" => {
                        comments.insert("discussion".into(), Value::Bool(true));
                    }
                    "reviewComments" => {
                        comments.insert("reviewInline".into(), Value::Bool(true));
                    }
                    "reviews" => {
                        content.insert("reviews".into(), Value::Bool(true));
                    }
                    "commits" => {
                        content
                            .entry("commits")
                            .or_insert_with(|| serde_json::json!({}));
                    }
                    "commitFiles" => {
                        content.insert("commits".into(), serde_json::json!({"includeFiles": true}));
                    }
                    _ => {}
                }
            }
            if !comments.is_empty() {
                if fields.get("includeBots").and_then(Value::as_bool) == Some(true) {
                    comments.insert("includeBots".into(), Value::Bool(true));
                }
                content.insert("comments".into(), Value::Object(comments));
            }
            // `patchRanges` select patch lines per file; with them, the exact
            // paths in `include` are selected files too (globs need a scan).
            if let Some(ranges) = fields
                .get("patchRanges")
                .filter(|ranges| ranges.as_array().is_some_and(|ranges| !ranges.is_empty()))
            {
                let files = strings(fields.get("include"))
                    .into_iter()
                    .filter(|path| !path.contains(['*', '?', '[', '{']))
                    .collect::<Vec<_>>();
                let mut patches = serde_json::json!({"mode":"selected","ranges":ranges});
                if !files.is_empty() {
                    patches["files"] = serde_json::json!(files);
                }
                content.insert("patches".into(), patches);
            }
            // `matchString` on a pull request filters its patches; with no
            // section selected it searches every patch.
            if operation == "pullRequest"
                && content.is_empty()
                && fields.get("matchString").and_then(Value::as_str).is_some()
            {
                content.insert("patches".into(), serde_json::json!({"mode":"all"}));
            }
            (
                (!content.is_empty()).then_some(Value::Object(content)),
                false,
                Vec::new(),
            )
        }
        "commit" | "compare" => {
            let include_diff = sections.iter().any(|section| section == "patches");
            let scope = strings(fields.get("include"));
            if operation == "commit"
                && let Some(base) = fields.remove("base")
            {
                fields.insert("operation".into(), Value::from("compare"));
                if let Some(head) = fields.remove("ref") {
                    fields.insert("head".into(), head);
                }
                fields.insert("base".into(), base);
                fields.remove("include");
                fields.entry("page").or_insert_with(|| Value::from(1));
            }
            (None, include_diff, scope)
        }
        _ => (None, false, Vec::new()),
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
            .unwrap_or(DEFAULT_PAGE_SIZE)
            .clamp(1, MAX_COLLECTION_PAGE)
    }
    /// Whether a pull request narrows its changed files (`include`,
    /// `status`, `minChanges`).
    /// Pull-request `patchRanges` (selected patch lines per file).
    pub fn patch_ranges(&self) -> &[crate::contracts::tool_types::HiPatchRange] {
        match self {
            Self::PullRequest { patch_ranges, .. } => patch_ranges,
            _ => &[],
        }
    }
    pub fn has_file_filter(&self) -> bool {
        matches!(
            self,
            Self::PullRequest { include, status, min_changes, .. }
                if include.is_some() || status.is_some() || min_changes.is_some()
        )
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

    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Commit { path, .. } | Self::Compare { path, .. } => {
                path.as_ref().map(|path| path.as_str())
            }
            _ => None,
        }
    }
    /// The text window offset (`offset`, characters).
    pub fn char_offset(&self) -> Option<usize> {
        match self {
            Self::PullRequest { offset, .. } | Self::Issue { offset, .. } => offset.map(usize_of),
            Self::Commit { offset, .. } | Self::Compare { offset, .. } => {
                offset.as_ref().map(|offset| usize_of(offset.0))
            }
        }
    }
    /// The text window length (`length`, characters).
    pub fn char_length(&self) -> Option<usize> {
        match self {
            Self::PullRequest { length, .. } | Self::Issue { length, .. } => {
                length.map(|length| usize_of(length.get()))
            }
            Self::Commit { length, .. } | Self::Compare { length, .. } => {
                length.as_ref().map(|length| usize_of(length.0.get()))
            }
        }
    }
    pub fn match_string(&self) -> Option<&str> {
        match self {
            Self::PullRequest { match_string, .. } => match_string.as_deref(),
            _ => None,
        }
    }
    /// Lines kept around each `matchString` hit in a patch (`contextLines`).
    pub fn match_context(&self) -> Option<usize> {
        match self {
            Self::PullRequest { context_lines, .. } => {
                context_lines.and_then(|lines| usize::try_from(lines).ok())
            }
            _ => None,
        }
    }
    /// Whether the query reads a later page of a pull request (a file, commit,
    /// review or comment page after the first, or a text window that names
    /// its offset): the caller already holds the item header and menu from
    /// page one.
    pub fn later_page(&self) -> bool {
        let after_first = |page: Option<usize>| page.is_some_and(|page| page > 1);
        matches!(self, Self::PullRequest { .. })
            && (after_first(self.file_page())
                || after_first(self.comment_page())
                || after_first(self.commit_page())
                || after_first(self.review_page())
                || self.char_offset().is_some())
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
            // A direct read of a SHA GitHub does not have (422 "No commit
            // found for SHA") is a missing commit, not an invalid query.
            if error.kind == ProviderErrorKind::Validation
                && error.status == Some(422)
                && error.message.starts_with("No commit found")
            {
                error.kind = ProviderErrorKind::NotFound;
                error.reason = Some(ProviderErrorReason::RefNotFound);
                error.message =
                    "Commit not found - verify the ref/SHA exists in this repository".into();
            }
            match error.kind {
                // The number names an issue, or the missing commit: the
                // message already says so.
                ProviderErrorKind::NotFound
                    if matches!(
                        error.reason,
                        Some(
                            ProviderErrorReason::PullRequestIsIssue
                                | ProviderErrorReason::RefNotFound
                        )
                    ) => {}
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
            "mainGoal":"Read the pull request.","reasoning":"False selectors must stay off."
        }))
        .expect("false selectors are valid booleans");
        let wants = pull_request::content_wants(&query);
        assert!(
            !wants.body && !wants.files && !wants.discussion && !wants.reviews && !wants.commits
        );
    }

    /// `sections`, `patchRanges` and `includeBots` select the internal
    /// content model each handler reads.
    #[test]
    fn sections_select_the_content_model() {
        let row = |fields: Value| {
            HistoryItemRequest::from_row(super::util::merge(
                json!({"operation":"pullRequest","mainGoal":"g","reasoning":"r",
                    "owner":"o","repo":"r","number":1}),
                fields,
            ))
            .expect("pull request row")
        };
        let read = row(json!({
            "sections":["body","files","patches","comments","reviewComments","reviews"],
            "include":["src/**"],"status":["modified"],"includeBots":true
        }));
        assert_eq!(
            read.content_value(),
            Some(
                json!({"body":true,"files":true,"patches":{"mode":"all"},"reviews":true,
                "comments":{"discussion":true,"reviewInline":true,"includeBots":true}})
            )
        );
        let ranged = row(json!({
            "include":["src/a.rs","src/*.rs"],
            "patchRanges":[{"file":"src/a.rs","additions":[1,2]}]
        }));
        assert_eq!(
            ranged.content_value(),
            Some(json!({"patches":{"mode":"selected",
                "ranges":[{"file":"src/a.rs","additions":[1,2]}],"files":["src/a.rs"]}}))
        );
        assert_eq!(
            row(json!({"matchString":"needle"})).content_value(),
            Some(json!({"patches":{"mode":"all"}}))
        );
        // Commit sections:["patches"] reads diffs; base folds compare in.
        let commit = HistoryItemRequest::from_row(json!({
            "operation":"commit","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
            "ref":"head-sha","base":"base-sha","sections":["patches"],"include":["src/*.rs"]
        }))
        .expect("commit with base");
        assert_eq!(commit.operation(), ItemOperation::Compare);
        assert_eq!(commit.base(), Some("base-sha"));
        assert_eq!(commit.head(), Some("head-sha"));
        assert!(commit.include_diff());
        assert_eq!(commit.file_scope, ["src/*.rs"]);
        let issue = HistoryItemRequest::from_row(json!({
            "operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":2,
            "sections":["comments"]
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
