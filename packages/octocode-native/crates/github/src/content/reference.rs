//! A ref's commit SHA: the memoized single-flight resolution every read of
//! a movable ref shares, and the transport's one-round-trip lookup.
use bytes::Bytes;
use reqwest::header::{ACCEPT, HeaderValue};
use serde::Deserialize;
use std::time::Duration;

use super::{CachedContent, ContentRequest, GitHubProvider, Retention, is_full_sha};
use crate::{
    GitHubTransport, Partition, ProviderError, ProviderErrorKind, RequestContext, RequestSpec,
};

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
            request.force_refresh,
            context,
        )
        .await
    }

    /// Resolve `reference` (branch, tag, short SHA, or the default branch when
    /// `None`) to a lowercase 40-hex commit SHA in one round trip: the
    /// `vnd.github.sha` media type returns just the SHA (40 B instead of the
    /// full commit with patches), and `HEAD` resolves the default branch
    /// without a separate `GET /repos/{o}/{r}`. Movable refs are memoized for
    /// [`REF_MEMO_TTL`] in the credential partition so a batch of reads on
    /// one ref resolves it once; `force_refresh` bypasses the memo.
    pub async fn resolve_reference(
        &self,
        owner: &str,
        repo: &str,
        reference: Option<&str>,
        force_refresh: bool,
        context: &RequestContext,
    ) -> Result<String, ProviderError> {
        if let Some(reference) = reference
            && is_full_sha(reference)
        {
            return Ok(reference.to_ascii_lowercase());
        }
        let reference = reference.unwrap_or("HEAD");
        let partition = self.transport.cache_partition(context, None)?;
        let key = ref_memo_key(owner, repo, reference);
        // Single flight: a concurrent batch on one ref waits (within its own
        // budget) for the first resolution and then reads it from the memo.
        let ticket = ref_flights().join((partition.clone(), key.clone()));
        let _turn = context.budget.wait(ticket.turn()).await?;
        if !force_refresh
            && let Some(sha) = self
                .transport
                .cache
                .get(&partition, &key)
                .await
                .and_then(|value| fresh_ref_memo(&value))
        {
            return Ok(sha);
        }
        let sha = self
            .transport
            .commit_sha(owner, repo, reference, context)
            .await?;
        self.transport
            .cache
            .put(
                &partition,
                key,
                CachedContent {
                    etag: None,
                    bytes: Bytes::copy_from_slice(sha.as_bytes()),
                    resolved_ref: String::new(),
                    has_more: false,
                },
                Retention::Expires(REF_MEMO_TTL),
            )
            .await;
        Ok(sha)
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
        // A full SHA that resolved once always resolves to itself: cache the
        // hit (commit-pinned namespace, partitioned by credential).
        let cached = if is_full_sha(reference) {
            let partition = self.cache_partition(context, None)?;
            let key = format!("github-commit:sha:{owner}/{repo}/{reference}");
            if let Some(hit) = self.cache.get(&partition, &key).await
                && let Ok(sha) = std::str::from_utf8(&hit.bytes)
            {
                return Ok(sha.to_owned());
            }
            Some((partition, key))
        } else {
            None
        };
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.sha"),
        );
        let page = self
            .execute(spec, context)
            .await
            .map_err(classify_ref_failure)?;
        let sha = parse_commit_sha(&page.body)?;
        if let Some((partition, key)) = cached {
            self.cache
                .put(
                    &partition,
                    key,
                    CachedContent {
                        etag: None,
                        bytes: sha.clone().into_bytes().into(),
                        resolved_ref: String::new(),
                        has_more: false,
                    },
                    Retention::Pinned,
                )
                .await;
        }
        Ok(sha)
    }
}

/// Type a ref-resolution failure: a bare 404 is the repository (missing,
/// private, or hidden from the token); the transport already typed a missing
/// ref ("No commit found") as `RefNotFound`.
fn classify_ref_failure(error: ProviderError) -> ProviderError {
    match error.status {
        Some(404) if error.reason.is_none() => {
            let mut error = error.with_reason(crate::ProviderErrorReason::RepositoryNotFound);
            // GitHub links its commit docs; the failure is the repository.
            error.documentation_url =
                Some("https://docs.github.com/rest/repos/repos#get-a-repository".into());
            error
        }
        _ => error,
    }
}
#[derive(Deserialize)]
struct CommitPayload {
    sha: String,
}
/// Seconds a movable ref → SHA resolution is reused. Short enough that a push
/// shows up on the next research step; long enough to cover one batch.
const REF_MEMO_TTL: Duration = Duration::from_secs(60);
/// The process's in-flight ref resolutions, one per `(credential, ref)`.
fn ref_flights() -> &'static crate::KeyedFlight<(Partition, String), ()> {
    static FLIGHTS: crate::KeyedFlight<(Partition, String), ()> = crate::KeyedFlight::transient();
    &FLIGHTS
}
fn ref_memo_key(owner: &str, repo: &str, reference: &str) -> String {
    // Owner/repo names are case-insensitive on GitHub; refs are not.
    crate::cache_key::fields_key(
        "github-ref",
        &[
            owner.to_ascii_lowercase().as_str(),
            repo.to_ascii_lowercase().as_str(),
            reference,
        ],
    )
}
/// The SHA a ref memo entry holds; the cache expires it after
/// [`REF_MEMO_TTL`].
fn fresh_ref_memo(value: &CachedContent) -> Option<String> {
    let sha = std::str::from_utf8(&value.bytes).ok()?;
    is_full_sha(sha).then(|| sha.to_ascii_lowercase())
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
