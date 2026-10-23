//! A repository's directory listing and submodule entries.
use bytes::Bytes;
use reqwest::header::{HeaderValue, IF_NONE_MATCH};

use super::payload::ContentPayload;
use super::{CachedContent, GitHubProvider, Retention, Submodule, is_full_sha};
use crate::{ProviderError, ProviderErrorKind, RequestContext, RequestSpec};

impl GitHubProvider {
    /// The submodule at `path` of `owner/repo` at commit `reference`, from
    /// its own contents entry; `None` when the path is no submodule.
    pub async fn submodule(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<Option<Submodule>, ProviderError> {
        let mut url = self
            .transport
            .endpoint()
            .rest(&["repos", owner, repo, "contents", path])?;
        url.query_pairs_mut().append_pair("ref", reference);
        let page = self
            .transport
            .execute(RequestSpec::get(url), context)
            .await?;
        let payload: ContentPayload = serde_json::from_slice(&page.body).map_err(|_| {
            ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub content response")
        })?;
        Ok(
            (payload.kind.as_deref() == Some("submodule")).then_some(Submodule {
                url: payload.submodule_git_url,
                commit: payload.sha,
            }),
        )
    }

    pub async fn repository_contents(
        &self,
        owner: &str,
        repo: &str,
        path: &str,
        reference: &str,
        context: &RequestContext,
    ) -> Result<crate::ContentsListing, ProviderError> {
        let partition = self.transport.cache_partition(context, None)?;
        let key = crate::cache_key::fields_key("github-tree", &[owner, repo, path, reference]);
        // A listing at a full commit SHA is immutable: a cached one is served
        // without asking GitHub again, like a file read at one.
        let pinned = is_full_sha(reference);
        let cached = self.transport.cache.get(&partition, &key).await;
        if let Some(value) = cached
            .as_ref()
            .filter(|value| pinned || value.etag.is_none())
        {
            return crate::tree::parse_contents_listing(&value.bytes);
        }
        let mut segments = vec!["repos", owner, repo, "contents"];
        if !path.is_empty() && path != "." {
            segments.push(path);
        }
        let mut url = self.transport.endpoint().rest(&segments)?;
        url.query_pairs_mut().append_pair("ref", reference);
        let mut spec = RequestSpec::get(url);
        if let Some(etag) = cached.as_ref().and_then(|value| value.etag.as_ref()) {
            spec.headers.insert(
                IF_NONE_MATCH,
                HeaderValue::from_str(etag).map_err(|_| {
                    ProviderError::new(ProviderErrorKind::Validation, "invalid cached ETag")
                })?,
            );
        }
        let page = self.transport.execute(spec, context).await?;
        let body = if page.status == 304 {
            cached
                .ok_or_else(|| {
                    ProviderError::new(
                        ProviderErrorKind::Decode,
                        "GitHub returned 304 without cached tree content",
                    )
                })?
                .bytes
        } else {
            Bytes::copy_from_slice(&page.body)
        };
        let listing = crate::tree::parse_contents_listing(&body)?;
        if page.status != 304 {
            let etag = page
                .headers
                .get("etag")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            self.transport
                .cache
                .put(
                    &partition,
                    key,
                    CachedContent {
                        etag,
                        bytes: body,
                        resolved_ref: reference.to_owned(),
                        has_more: false,
                    },
                    if pinned {
                        Retention::Pinned
                    } else {
                        Retention::Revalidate
                    },
                )
                .await;
        }
        Ok(listing)
    }
}
