use super::{GitHubTransport, ProviderError, ProviderErrorKind, RequestContext, RequestSpec};
use serde_json::Value;
#[derive(Clone, Debug)]
pub struct HistoryItemResponse {
    pub value: Value,
    pub has_more: bool,
    /// The last page number GitHub's `Link` header names (`rel="last"`), when
    /// it names one: how many pages a list holds at this `per_page`.
    pub last_page: Option<usize>,
}

/// What a response's `Link` header says of its list.
struct PageLinks {
    has_more: bool,
    last_page: Option<usize>,
}

impl GitHubTransport {
    /// One history read (issue, pull request, commit, comparison or their pages).
    pub async fn history_item(
        &self,
        segments: &[&str],
        query: &[(&str, String)],
        context: &RequestContext,
    ) -> Result<HistoryItemResponse, ProviderError> {
        let mut url = self.endpoint().rest(segments)?;
        for (key, value) in query {
            url.query_pairs_mut().append_pair(key, value);
        }
        let (value, links) = self
            .read_json_linked(RequestSpec::get(url), context, "history item")
            .await?;
        Ok(HistoryItemResponse {
            value,
            has_more: links.has_more,
            last_page: links.last_page,
        })
    }

    /// GET decoded as JSON, retaining whether the response links a next page.
    pub(crate) async fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        what: &str,
    ) -> Result<(T, bool), ProviderError> {
        let (value, links) = self.read_json_linked(spec, context, what).await?;
        Ok((value, links.has_more))
    }

    /// GET decoded as JSON with the pagination facts of its `Link` header.
    async fn read_json_linked<T: serde::de::DeserializeOwned>(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        what: &str,
    ) -> Result<(T, PageLinks), ProviderError> {
        let response = self.execute(spec, context).await?;
        let value = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                format!("invalid GitHub {what} response"),
            )
        })?;
        let links = PageLinks {
            has_more: response.next.is_some(),
            last_page: super::transport::last_page(&response.headers),
        };
        Ok((value, links))
    }
}

#[cfg(test)]
mod tests;
