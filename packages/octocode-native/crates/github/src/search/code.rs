//! Code and repository searches.

use super::{CodeSearchPage, CodeSearchRequest, RepositorySearchPage, RepositorySearchRequest};
use crate::{GitHubTransport, ProviderError, RequestContext, RequestSpec};

impl GitHubTransport {
    /// A code search with the response's partial-organization evidence.
    pub async fn search_code(
        &self,
        request: &CodeSearchRequest,
        context: &RequestContext,
    ) -> Result<CodeSearchPage, ProviderError> {
        let (mut page, withheld): (CodeSearchPage, _) = self
            .read_search_page(
                |page, per_page| self.code_search_spec(request, page, per_page),
                request.page,
                request.per_page,
                context,
                "invalid GitHub code search response",
            )
            .await?;
        page.sso_partial_organizations = withheld;
        Ok(page)
    }

    /// How many of `requests` the code-search quota must cover: those a
    /// resumed continuation's journal or a saved answer cannot answer with
    /// every provider page they read (neither costs a provider call).
    pub fn code_search_unanswered(
        &self,
        requests: &[CodeSearchRequest],
        context: &RequestContext,
    ) -> Result<usize, ProviderError> {
        let mut unanswered = 0;
        for request in requests {
            let specs = super::read::search_requests(
                |page, per_page| self.code_search_spec(request, page, per_page),
                request.page,
                request.per_page,
            )?;
            if !specs.iter().all(|spec| self.answers_unasked(spec, context)) {
                unanswered += 1;
            }
        }
        Ok(unanswered)
    }

    fn code_search_spec(
        &self,
        request: &CodeSearchRequest,
        page: usize,
        per_page: usize,
    ) -> Result<RequestSpec, ProviderError> {
        let mut url = self.endpoint().rest(&["search", "code"])?;
        url.query_pairs_mut()
            .append_pair("q", &request.query)
            .append_pair("page", &page.to_string())
            .append_pair("per_page", &per_page.to_string());
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
        let spec = |page: usize, per_page: usize| -> Result<RequestSpec, ProviderError> {
            let mut url = self.endpoint().rest(&["search", "repositories"])?;
            {
                let mut pairs = url.query_pairs_mut();
                pairs
                    .append_pair("q", &request.query)
                    .append_pair("page", &page.to_string())
                    .append_pair("per_page", &per_page.to_string());
                // The wire `sort` is `field:direction`; the API takes two params.
                if let Some((field, order)) =
                    request.sort.as_deref().and_then(|s| s.split_once(':'))
                {
                    pairs.append_pair("sort", field).append_pair("order", order);
                }
            }
            let mut spec = RequestSpec::get(url);
            spec.headers.insert(
                reqwest::header::ACCEPT,
                reqwest::header::HeaderValue::from_static("application/vnd.github.v3+json"),
            );
            Ok(spec)
        };
        self.read_search_page(
            spec,
            request.page,
            request.per_page,
            context,
            "invalid GitHub repository search response",
        )
        .await
        .map(|(page, _)| page)
    }
}
