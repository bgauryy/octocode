use super::{GitHubTransport, ProviderError, ProviderErrorKind, RequestContext, RequestSpec};
use serde_json::Value;
#[derive(Clone, Debug)]
pub struct HistoryItemResponse {
    pub value: Value,
    pub has_more: bool,
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
        let (value, has_more) = self
            .read_json(RequestSpec::get(url), context, "history item")
            .await?;
        Ok(HistoryItemResponse { value, has_more })
    }

    /// GET decoded as JSON, retaining whether the response links a next page.
    pub(crate) async fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        what: &str,
    ) -> Result<(T, bool), ProviderError> {
        let response = self.execute(spec, context).await?;
        let value = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                format!("invalid GitHub {what} response"),
            )
        })?;
        Ok((value, response.next.is_some()))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        CredentialSource, GitHubEndpoint, GitHubTransport, RequestContext, ResolvedCredential,
        RetryPolicy,
    };
    use octocode_io::budget::RequestBudget;
    use std::time::Duration;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    /// Rows of one batch each read the current pull request.
    #[tokio::test]
    async fn concurrent_reads_of_one_item_send_independent_requests() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/pulls/9"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"v1\"")
                    .set_delay(Duration::from_millis(150))
                    .set_body_json(serde_json::json!({"number": 9})),
            )
            .expect(3)
            .mount(&server)
            .await;
        let endpoint =
            GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).unwrap())
                .unwrap();
        let transport = GitHubTransport::new(
            endpoint,
            RetryPolicy {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                max_retry_after: Duration::from_secs(1),
            },
            crate::GitHubBudget::relaxed(),
        )
        .unwrap();
        let context = || {
            RequestContext::new(
                RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
                Some(ResolvedCredential::new(
                    "flight",
                    CredentialSource::Environment,
                )),
            )
        };
        let read = || async {
            transport
                .history_item(&["repos", "o", "r", "pulls", "9"], &[], &context())
                .await
                .expect("pull request")
        };
        let (first, second, third) = tokio::join!(read(), read(), read());
        for item in [first, second, third] {
            assert_eq!(item.value["number"], 9);
        }
    }
}
