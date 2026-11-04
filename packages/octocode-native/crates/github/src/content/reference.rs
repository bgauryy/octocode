//! Resolve a ref to its commit SHA in one round trip.
use reqwest::header::{ACCEPT, HeaderValue};
use serde::Deserialize;

use super::{ContentRequest, GitHubProvider, is_full_sha};
use crate::{GitHubTransport, ProviderError, ProviderErrorKind, RequestContext, RequestSpec};

impl GitHubProvider {
    pub(super) async fn resolve_commit(
        &self,
        request: &ContentRequest,
        context: &RequestContext,
    ) -> Result<String, ProviderError> {
        self.resolve_reference(
            &request.owner,
            &request.repo,
            request.reference.as_deref(),
            context,
        )
        .await
    }

    /// Resolve `reference` (branch, tag, short SHA, or the default branch when
    /// `None`) to a lowercase 40-hex commit SHA in one round trip: the
    /// `vnd.github.sha` media type returns just the SHA (40 B instead of the
    /// full commit with patches), and `HEAD` resolves the default branch
    /// without a separate `GET /repos/{o}/{r}`.
    pub async fn resolve_reference(
        &self,
        owner: &str,
        repo: &str,
        reference: Option<&str>,
        context: &RequestContext,
    ) -> Result<String, ProviderError> {
        if let Some(reference) = reference
            && is_full_sha(reference)
        {
            return Ok(reference.to_ascii_lowercase());
        }
        let reference = reference.unwrap_or("HEAD");
        self.transport
            .commit_sha(owner, repo, reference, context)
            .await
    }
}

impl GitHubTransport {
    /// Resolve `reference` (branch, tag, short SHA, or `HEAD`) to its lowercase
    /// 40-hex commit SHA in one round trip: the `vnd.github.sha` media type
    /// returns just the SHA. A 404 means the repository itself did not
    /// resolve; GitHub answers a missing ref in an existing repository with
    /// 422 "No commit found". Both failures carry a typed reason.
    pub async fn commit_sha(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<String, ProviderError> {
        let url = self
            .endpoint()
            .rest(&["repos", owner, repo, "commits", reference])?;
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.sha"),
        );
        let page = self
            .execute(spec, context)
            .await
            .map_err(classify_ref_failure)?;
        parse_commit_sha(&page.body)
    }
}

/// Type a ref-resolution failure: a bare 404 is the repository (missing,
/// private, or hidden from the token); the transport already typed a missing
/// ref ("No commit found") as `RefNotFound`.
fn classify_ref_failure(error: ProviderError) -> ProviderError {
    match error.status {
        Some(404) if error.reason.is_none() => {
            error.with_reason(crate::ProviderErrorReason::RepositoryNotFound)
        }
        _ => error,
    }
}
#[derive(Deserialize)]
struct CommitPayload {
    sha: String,
}
/// `vnd.github.sha` answers with the bare SHA; GHES versions (and test
/// fixtures) that ignore the media type answer with the commit JSON.
fn parse_commit_sha(body: &[u8]) -> Result<String, ProviderError> {
    let text = std::str::from_utf8(body).unwrap_or_default().trim();
    let sha = if is_full_sha(text) {
        text.to_owned()
    } else {
        serde_json::from_slice::<CommitPayload>(body)
            .map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub commit response")
            })?
            .sha
    };
    if !is_full_sha(&sha) {
        return Err(ProviderError::new(
            ProviderErrorKind::Decode,
            "GitHub returned an invalid commit SHA",
        ));
    }
    Ok(sha.to_ascii_lowercase())
}
