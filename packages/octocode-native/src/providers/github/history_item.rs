use super::{
    CredentialResolver, GitHubTransport, ProviderError, ProviderErrorKind, RequestContext,
    RequestSpec,
};
use serde_json::Value;
#[derive(Clone, Debug)]
pub struct HistoryItemResponse {
    pub value: Value,
    pub has_more: bool,
}
impl<R: CredentialResolver> GitHubTransport<R> {
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
        let response = self.execute(RequestSpec::get(url), context).await?;
        let value = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "invalid GitHub history item response",
            )
        })?;
        Ok(HistoryItemResponse {
            value,
            has_more: response.next.is_some(),
        })
    }
}
