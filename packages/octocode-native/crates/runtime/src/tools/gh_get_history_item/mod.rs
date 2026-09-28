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
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, RequestContext,
};
use crate::tools::local_fetch::ContentScan;
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

const DEFAULT_PAGE_SIZE: usize = 30;
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
}

impl HistoryItemRequest {
    /// Parses a validated `ghGetHistoryItem` row.
    pub fn from_row(row: Value) -> Result<Self, serde_json::Error> {
        let content = row.get("content").cloned();
        Ok(Self {
            query: serde_json::from_value(row)?,
            content,
            auto_page_chars: None,
        })
    }
    /// The `content` selector as JSON, for the shaping code's key lookups.
    pub fn content_value(&self) -> Option<Value> {
        self.content.clone()
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
            Self::PullRequest { page_size, .. }
            | Self::Issue { page_size, .. }
            | Self::Commit { page_size, .. }
            | Self::Compare { page_size, .. } => {
                page_size.as_ref().map(|size| usize_of(size.0.get()))
            }
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
            Self::Commit { include_diff, .. } | Self::Compare { include_diff, .. } => *include_diff,
            _ => false,
        }
    }
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Commit { path, .. } | Self::Compare { path, .. } => path.as_deref(),
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
    pub fn comment_body_offset(&self) -> Option<usize> {
        match self {
            Self::PullRequest {
                comment_body_offset,
                ..
            } => comment_body_offset.map(usize_of),
            _ => None,
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
        ItemOperation::PullRequest => pull_request::pull_request(transport, query, context).await,
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
            "goal":"Read the pull request.","reasoning":"False selectors must stay off.",
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
            json!({"operation":"commit","goal": "test", "reasoning":"test","owner":"a","repo":"b"}),
            json!({"operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b"}),
            json!({"operation":"compare","goal": "test", "reasoning":"test","owner":"a","repo":"b","base":"x"}),
        ] {
            assert!(HistoryItemRequest::from_row(row).is_err());
        }
        let committed = HistoryItemRequest::from_row(
            json!({"operation":"commit","goal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"x"}),
        )
        .expect("GitHub history test data should be valid");
        assert!(validate(&committed).is_ok());
        let blank = HistoryItemRequest::from_row(json!({
            "operation":"compare","goal": "test", "reasoning":"test","owner":"a","repo":"b","base":"","head":"x"
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
