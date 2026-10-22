use super::{
    CachedContent, GitHubTransport, Partition, ProviderError, ProviderErrorKind, RequestContext,
    RequestSpec,
};
use bytes::Bytes;
use reqwest::header::{HeaderValue, IF_NONE_MATCH};
use serde_json::Value;
#[derive(Clone, Debug)]
pub struct HistoryItemResponse {
    pub value: Value,
    pub has_more: bool,
}

/// A history read that names only immutable objects: a commit by its full
/// SHA, a comparison of two full SHAs (`base...head`), or a file's contents
/// at a full SHA (`?ref=`). Its body, and every page of it, never changes.
fn pinned_history(segments: &[&str], query: &[(&str, String)]) -> bool {
    match segments {
        ["repos", _, _, "commits", sha] => super::content::is_full_sha(sha),
        ["repos", _, _, "contents", _, ..] => query
            .iter()
            .any(|(key, value)| *key == "ref" && super::content::is_full_sha(value)),
        ["repos", _, _, "compare", refs] => refs.split_once("...").is_some_and(|(base, head)| {
            super::content::is_full_sha(base) && super::content::is_full_sha(head)
        }),
        _ => false,
    }
}

/// One history read's cache key: the URL and media type, in the pinned or
/// the revalidated namespace.
fn history_key(spec: &RequestSpec, pinned: bool) -> String {
    let namespace = if pinned {
        "github-commit"
    } else {
        "github-history"
    };
    super::cache_key::request_key(namespace, spec)
}

/// In-flight history reads: a transient slot holds the first read's answer
/// for the reads that waited on it, and is forgotten once none waits, so it
/// never outlives one concurrent wave.
type HistoryFlights = super::KeyedFlight<(Partition, String), Option<(Bytes, bool)>>;

fn history_flights() -> &'static HistoryFlights {
    static FLIGHTS: HistoryFlights = super::KeyedFlight::transient();
    &FLIGHTS
}

impl GitHubTransport {
    /// One history read (issue, pull request, commit, comparison, their
    /// pages).
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
        let (value, has_more) = self
            .revalidated_json(
                RequestSpec::get(url),
                pinned_history(segments, query),
                context,
                "history item",
            )
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
    /// it (304, no rate-limit cost). A `pinned` read (a commit, or a
    /// comparison of two commits, addressed by full SHAs) cannot change and
    /// is served from the cache directly. Concurrent reads of one URL (the
    /// rows of a batch naming one item) share a single flight: the first
    /// sends the request and the others take its answer; a failed read
    /// leaves nothing, so the next one asks again.
    /// Returns the body and whether a next page exists.
    pub(crate) async fn revalidated_get(
        &self,
        spec: RequestSpec,
        pinned: bool,
        context: &RequestContext,
    ) -> Result<(Bytes, bool), ProviderError> {
        let partition = self.cache_partition(context, None)?;
        let key = history_key(&spec, pinned);
        let ticket = history_flights().join((partition.clone(), key.clone()));
        let mut shared = context.budget.wait(ticket.turn()).await?;
        if let Some(answer) = shared.as_ref() {
            return Ok(answer.clone());
        }
        let answer = self
            .revalidated_get_once(spec, pinned, &partition, key, context)
            .await?;
        *shared = Some(answer.clone());
        Ok(answer)
    }

    /// The read behind [`Self::revalidated_get`]'s flight.
    async fn revalidated_get_once(
        &self,
        mut spec: RequestSpec,
        pinned: bool,
        partition: &Partition,
        key: String,
        context: &RequestContext,
    ) -> Result<(Bytes, bool), ProviderError> {
        let cached = self.cache.get(partition, &key).await;
        if pinned && let Some(cached) = cached.as_ref().filter(|value| value.etag.is_none()) {
            return Ok((cached.bytes.clone(), cached.has_more));
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
            return Ok((cached.bytes, cached.has_more));
        }
        let has_more = response.next.is_some();
        let body = response.body;
        let etag = response
            .headers
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        // Without an ETag a mutable read cannot be revalidated: not stored.
        if pinned || etag.is_some() {
            self.cache
                .put(
                    partition,
                    key,
                    CachedContent {
                        etag,
                        // A copy sized to the body: the response buffer may
                        // carry spare capacity the cache would not weigh.
                        bytes: Bytes::copy_from_slice(&body),
                        resolved_ref: String::new(),
                        has_more,
                    },
                    if pinned {
                        super::Retention::Pinned
                    } else {
                        super::Retention::AlwaysRevalidate
                    },
                )
                .await;
        }
        Ok((body, has_more))
    }
}

#[cfg(test)]
mod tests {
    use super::pinned_history;
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

    /// Rows of one batch that name the same pull request read it once: the
    /// first read's answer serves the reads that waited on it.
    #[tokio::test]
    async fn concurrent_reads_of_one_item_send_one_request() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/o/r/pulls/9"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"v1\"")
                    .set_delay(Duration::from_millis(150))
                    .set_body_json(serde_json::json!({"number": 9})),
            )
            .expect(1)
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
                max_rate_limit_wait: Duration::from_secs(1),
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

    /// Only reads that name immutable objects by full SHA are pinned: a
    /// commit, or a comparison of two commits. A branch, tag, short SHA or
    /// cross-repository `owner:ref` side keeps revalidating.
    #[test]
    fn commits_and_comparisons_of_full_shas_are_pinned() {
        let (a, b) = ("a".repeat(40), "b".repeat(40));
        let compare = |refs: &str| pinned_history(&["repos", "o", "r", "compare", refs], &[]);
        assert!(pinned_history(&["repos", "o", "r", "commits", &a], &[]));
        assert!(compare(&format!("{a}...{b}")));
        assert!(!compare(&format!("{a}...main")));
        assert!(!compare(&format!("v1...{b}")));
        assert!(!compare(&format!("{a}...fork:{b}")));
        assert!(!compare(&format!("{}...{b}", &a[..12])));
        assert!(!compare(&format!("{a}..{b}")));
        assert!(!pinned_history(
            &["repos", "o", "r", "commits", "main"],
            &[]
        ));
        assert!(!pinned_history(&["repos", "o", "r", "pulls", "1"], &[]));
        // A file's contents at a full SHA (a pull request's head text) is a
        // blob that never changes; at a branch it moves.
        let contents = |reference: &str| {
            pinned_history(
                &["repos", "o", "r", "contents", "src", "a.rs"],
                &[("ref", reference.to_owned())],
            )
        };
        assert!(contents(&a));
        assert!(!contents("main"));
        assert!(!contents(&a[..12]));
        assert!(!pinned_history(
            &["repos", "o", "r", "contents", "a.rs"],
            &[]
        ));
    }
}
