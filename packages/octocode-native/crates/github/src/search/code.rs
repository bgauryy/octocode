//! Code and repository searches: their requests and the stored-page split.

use super::{
    CodeSearchPage, CodeSearchRequest, Lookup, RepositorySearchPage, RepositorySearchRequest,
};
use crate::{GitHubTransport, ProviderError, RequestContext, RequestSpec};

impl GitHubTransport {
    /// A code search that serves a stored page first. Production searches
    /// split it into [`Self::cached_code_search`] and
    /// [`Self::search_code_uncached`]; the tests read the whole path at once.
    #[cfg(test)]
    pub(crate) async fn search_code(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
    ) -> Result<CodeSearchPage, ProviderError> {
        self.code_search(request, context, Lookup::Store).await
    }

    /// The code search for a request [`Self::cached_code_search`] just
    /// missed: the store is read again only when an identical search in
    /// flight stored its page meanwhile.
    pub async fn search_code_uncached(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
    ) -> Result<CodeSearchPage, ProviderError> {
        self.code_search(request, context, Lookup::Missed).await
    }

    async fn code_search(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
        lookup: Lookup,
    ) -> Result<CodeSearchPage, ProviderError> {
        let (mut page, withheld) = self
            .cached_search(
                "github-code-search",
                self.code_search_spec(request)?,
                context,
                lookup,
                "invalid GitHub code search response",
                |page: &CodeSearchPage| !page.incomplete_results,
            )
            .await?;
        page.sso_partial_organizations = withheld;
        Ok(page)
    }

    /// The stored page a code search would return for `request`, without
    /// sending: a caller planning several searches counts window slots only
    /// for the ones the cache cannot answer.
    pub async fn cached_code_search(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
    ) -> Result<Option<CodeSearchPage>, ProviderError> {
        self.cached_page(
            "github-code-search",
            &self.code_search_spec(request)?,
            context,
        )
        .await
    }

    fn code_search_spec(&self, request: &CodeSearchRequest) -> Result<RequestSpec, ProviderError> {
        let mut url = self.endpoint().rest(&["search", "code"])?;
        url.query_pairs_mut()
            .append_pair("q", &request.query)
            .append_pair("page", &request.page.to_string())
            .append_pair("per_page", &request.per_page.to_string());
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static(if request.include_fragments {
                "application/vnd.github.v3.text-match+json"
            } else {
                "application/vnd.github+json"
            }),
        );
        Ok(spec)
    }

    pub async fn search_repositories(
        &self,
        request: &RepositorySearchRequest,
        context: &RequestContext,
    ) -> Result<RepositorySearchPage, ProviderError> {
        let mut url = self.endpoint().rest(&["search", "repositories"])?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs
                .append_pair("q", &request.query)
                .append_pair("page", &request.page.to_string())
                .append_pair("per_page", &request.per_page.to_string());
            // The wire `sort` is `field:direction`; the API takes two params.
            if let Some((field, order)) = request.sort.as_deref().and_then(|s| s.split_once(':')) {
                pairs.append_pair("sort", field).append_pair("order", order);
            }
        }
        let mut spec = RequestSpec::get(url);
        spec.headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"),
        );
        self.cached_search(
            "github-repo-search",
            spec,
            context,
            Lookup::Store,
            "invalid GitHub repository search response",
            |page: &RepositorySearchPage| !page.incomplete_results,
        )
        .await
        .map(|(page, _)| page)
    }
}
