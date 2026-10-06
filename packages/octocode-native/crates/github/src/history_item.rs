use super::{
    CachedContent, CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind,
    RequestContext, RequestSpec,
};
use reqwest::header::{HeaderValue, IF_NONE_MATCH};
use serde_json::Value;
use sha2::{Digest, Sha256};
#[derive(Clone, Debug)]
pub struct HistoryItemResponse {
    pub value: Value,
    pub has_more: bool,
}
impl<R: CredentialResolver> GitHubTransport<R> {
    /// One history read (issue, pull request, commit, their pages).
    pub async fn history_item(
        &self,
        segments: &[&str],
        query: &[(&str, String)],
        context: &RequestContext,
    ) -> Result<HistoryItemResponse, ProviderError> {
        let mut url = self.endpoint().rest(segments)?;
        {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in query {
                pairs.append_pair(k, v);
            }
        }
        let pinned =
            matches!(segments, ["repos", _, _, "commits", sha] if super::content::is_full_sha(sha));
        let (value, has_more) = self
            .revalidated_json(RequestSpec::get(url), pinned, context, "history item")
            .await?;
        Ok(HistoryItemResponse { value, has_more })
    }

    /// [`Self::revalidated_get`] decoded as JSON; `what` names the response
    /// in the decode error.
    pub(crate) async fn revalidated_json<T: serde::de::DeserializeOwned>(
        &self,
        spec: RequestSpec,
        pinned: bool,
        context: &RequestContext,
        what: &str,
    ) -> Result<(T, bool), ProviderError> {
        let (body, has_more) = self.revalidated_get(spec, pinned, context).await?;
        let value = serde_json::from_slice(&body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                format!("invalid GitHub {what} response"),
            )
        })?;
        Ok((value, has_more))
    }

    /// GET through the conditional cache. Issues, pull requests and their
    /// lists change, so a stored body is served only after GitHub confirms
    /// it (304, no rate-limit cost). A `pinned` read (a commit addressed by
    /// its full SHA) cannot change and is served from the cache directly.
    /// Returns the body and whether a next page exists.
    pub(crate) async fn revalidated_get(
        &self,
        mut spec: RequestSpec,
        pinned: bool,
        context: &RequestContext,
    ) -> Result<(Vec<u8>, bool), ProviderError> {
        let partition = self.cache_partition(context, None).await?;
        let key = {
            let mut digest = Sha256::new();
            digest.update(spec.url.as_str().as_bytes());
            digest.update([0]);
            if let Some(accept) = spec.headers.get(reqwest::header::ACCEPT) {
                digest.update(accept.as_bytes());
            }
            let namespace = if pinned {
                "github-commit"
            } else {
                "github-history"
            };
            format!("{namespace}:{}", hex::encode(digest.finalize()))
        };
        let cached = self.cache.get(&partition, &key).await;
        if pinned && let Some(cached) = cached.as_ref().filter(|value| value.etag.is_none()) {
            return Ok((cached.bytes.clone(), !cached.resolved_ref.is_empty()));
        }
        if let Some(etag) = cached.as_ref().and_then(|value| value.etag.as_deref()) {
            spec.headers.insert(
                IF_NONE_MATCH,
                HeaderValue::from_str(etag).map_err(|_| {
                    ProviderError::new(ProviderErrorKind::Validation, "invalid cached ETag")
                })?,
            );
        }
        let response = self.execute(spec, context).await?;
        if response.status == 304 {
            let cached = cached.ok_or_else(|| {
                ProviderError::new(
                    ProviderErrorKind::Decode,
                    "GitHub returned 304 without a cached response",
                )
            })?;
            return Ok((cached.bytes, !cached.resolved_ref.is_empty()));
        }
        let has_more = response.next.is_some();
        let body = response.body.to_vec();
        let etag = response
            .headers
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        // Without an ETag a mutable read cannot be revalidated: not stored.
        if pinned || etag.is_some() {
            self.cache
                .put(
                    &partition,
                    key,
                    CachedContent {
                        etag,
                        bytes: body.clone(),
                        // Non-empty marks a page with a next page.
                        resolved_ref: response.next.map(String::from).unwrap_or_default(),
                    },
                )
                .await;
        }
        Ok((body, has_more))
    }
}
