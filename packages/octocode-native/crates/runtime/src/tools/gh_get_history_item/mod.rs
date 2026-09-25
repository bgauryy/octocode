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
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GhGetHistoryItemQuery {
    pub operation: ItemOperation,
    pub owner: String,
    pub repo: String,
    pub number: Option<u64>,
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub content: Option<Value>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
    pub file_page: Option<usize>,
    pub file_batch: Option<usize>,
    pub comment_page: Option<usize>,
    pub commit_page: Option<usize>,
    pub review_page: Option<usize>,
    pub collection_pages: Option<Value>,
    pub include_diff: Option<bool>,
    pub path: Option<String>,
    pub char_offset: Option<usize>,
    pub char_length: Option<usize>,
    pub match_string: Option<String>,
    pub comment_body_offset: Option<usize>,
    pub minify: Option<String>,
    pub goal: Option<String>,
    pub reasoning: Option<String>,
    /// Effective automatic response page (`output.pagination.defaultCharLength`),
    /// set by the runtime; never part of the public query.
    #[serde(skip)]
    pub auto_page_chars: Option<usize>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemOperation {
    PullRequest,
    Issue,
    Commit,
    Compare,
}

pub async fn execute<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &GhGetHistoryItemQuery,
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
                    error.message = if matches!(query.operation, ItemOperation::PullRequest) {
                        format!(
                            "Failed to fetch pull request #{}: {canonical}",
                            query.number.unwrap_or_default()
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
    query: &GhGetHistoryItemQuery,
    context: &RequestContext,
) -> Result<Value, ProviderError> {
    validate(query)?;
    check_context(context)?;
    match query.operation {
        ItemOperation::PullRequest => pull_request::pull_request(transport, query, context).await,
        ItemOperation::Issue => issue::issue(transport, query, context).await,
        ItemOperation::Commit => commit_compare::commit(transport, query, context).await,
        ItemOperation::Compare => commit_compare::compare(transport, query, context).await,
    }
}

fn validate(query: &GhGetHistoryItemQuery) -> Result<(), ProviderError> {
    if query.owner.is_empty() || query.repo.is_empty() {
        return Err(validation("owner and repo are required"));
    }
    match query.operation {
        ItemOperation::PullRequest | ItemOperation::Issue if query.number.is_none() => {
            Err(validation("number is required"))
        }
        ItemOperation::Commit if query.reference.as_deref().is_none_or(str::is_empty) => {
            Err(validation("ref is required"))
        }
        ItemOperation::Compare
            if query.base.as_deref().is_none_or(str::is_empty)
                || query.head.as_deref().is_none_or(str::is_empty) =>
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
        let q: GhGetHistoryItemQuery =
            serde_json::from_str(r#"{"operation":"commit","owner":"a","repo":"b"}"#)
                .expect("GitHub history test data should be valid");
        assert!(validate(&q).is_err());
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
