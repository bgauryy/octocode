//! A repository's directory listing and submodule entries.

use super::payload::ContentPayload;
use super::{GitHubProvider, Submodule};
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
        let mut segments = vec!["repos", owner, repo, "contents"];
        if !path.is_empty() && path != "." {
            segments.push(path);
        }
        let mut url = self.transport.endpoint().rest(&segments)?;
        url.query_pairs_mut().append_pair("ref", reference);
        let page = self
            .transport
            .execute(RequestSpec::get(url), context)
            .await?;
        crate::tree::parse_contents_listing(&page.body)
    }
}
