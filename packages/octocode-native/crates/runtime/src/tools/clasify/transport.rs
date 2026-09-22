//! Classification provider HTTP transport, deadline, cancellation, and error
//! handling. Vendor-agnostic: the caller supplies the endpoint URL, key, and
//! JSON body; this module only handles the HTTP lifecycle.
use crate::providers::RequestBudget;
use bytes::BytesMut;
use futures_util::StreamExt;
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use std::cell::RefCell;
use std::future::Future;
use std::time::{Duration, Instant};
use url::Url;

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Per-thread client so classification pages within one runtime reuse the
/// connection pool (HTTP keep-alive) without binding the pool to a single
/// global Tokio runtime. Each OS thread — including each test thread — gets
/// its own client, which avoids cross-runtime connection pool corruption when
/// parallel `#[tokio::test]`s each run with their own short-lived runtime.
/// `reqwest::Client` is an `Arc` internally, so the clone in `shared_client`
/// is pointer-width and shares the pool within the same thread.
fn shared_client() -> Result<reqwest::Client, ClassificationError> {
    thread_local! {
        static CLIENT: RefCell<Option<reqwest::Client>> = const { RefCell::new(None) };
    }
    CLIENT.with(|cell| {
        let mut guard = cell.borrow_mut();
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| transport_error())?;
        *guard = Some(client.clone());
        Ok(client)
    })
}

/// Error returned by the classification transport layer. `code` is a stable
/// machine-readable identifier; `message` is human-readable prose; `hints`
/// are actionable suggestions shown to the caller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassificationError {
    pub code: String,
    pub message: String,
    pub hints: Vec<String>,
}

impl ClassificationError {
    pub(crate) fn new(code: &str, message: impl Into<String>, hint: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            hints: vec![hint.to_owned()],
        }
    }
}

pub(crate) fn endpoint(base_url: &str, path: &str) -> Result<Url, ClassificationError> {
    let base = Url::parse(base_url).map_err(|_| {
        ClassificationError::new(
            "invalidClassificationConfiguration",
            "OCTOCODE_CLASSIFICATION_API_HOST is not a valid URL.",
            "Use an HTTPS API root without a path, query, fragment, or credentials.",
        )
    })?;
    let host = base.host_str().unwrap_or_default();
    let loopback = matches!(host, "localhost" | "127.0.0.1" | "::1");
    let valid_scheme = base.scheme() == "https" || (base.scheme() == "http" && loopback);
    if !valid_scheme
        || host.is_empty()
        || !base.username().is_empty()
        || base.password().is_some()
        || !matches!(base.path(), "" | "/")
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(ClassificationError::new(
            "invalidClassificationConfiguration",
            "OCTOCODE_CLASSIFICATION_API_HOST must be an HTTPS API root; HTTP is allowed only on loopback.",
            "Remove paths, credentials, query strings, and fragments from the configured API root.",
        ));
    }
    base.join(path).map_err(|_| {
        ClassificationError::new(
            "invalidClassificationConfiguration",
            "Cannot construct the classification provider endpoint.",
            "Check OCTOCODE_CLASSIFICATION_API_HOST.",
        )
    })
}

pub(super) fn check_budget(budget: &RequestBudget) -> Result<(), ClassificationError> {
    if budget.cancellation.is_cancelled() {
        return Err(ClassificationError::new(
            "cancelled",
            "Classification request was cancelled.",
            "Retry only if this evaluation is still needed.",
        ));
    }
    if Instant::now() >= budget.deadline {
        return Err(ClassificationError::new(
            "timeout",
            "Classification request exceeded its total deadline.",
            "Retry later or increase the shared request timeout.",
        ));
    }
    Ok(())
}

async fn wait<T>(
    budget: &RequestBudget,
    future: impl Future<Output = T>,
) -> Result<T, ClassificationError> {
    check_budget(budget)?;
    let remaining = budget.deadline.saturating_duration_since(Instant::now());
    tokio::select! {
        _ = budget.cancellation.cancelled() => Err(ClassificationError::new(
            "cancelled",
            "Classification request was cancelled.",
            "Retry only if this evaluation is still needed.",
        )),
        value = tokio::time::timeout(remaining, future) => value.map_err(|_| ClassificationError::new(
            "timeout",
            "Classification request exceeded its total deadline.",
            "Retry later or increase the shared request timeout.",
        )),
    }
}

fn retry_delay(headers: &reqwest::header::HeaderMap, attempt: u32) -> Duration {
    if let Some(milliseconds) = headers
        .get("retry-after-ms")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_millis(milliseconds);
    }
    if let Some(seconds) = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_secs(seconds);
    }
    Duration::from_millis(500_u64.saturating_mul(1_u64 << attempt.min(10)))
}

fn transport_error() -> ClassificationError {
    ClassificationError::new(
        "classificationProviderError",
        "Classification provider HTTPS request failed or timed out.",
        "Check connectivity, certificates, proxy settings, and the configured API root.",
    )
}

pub(crate) async fn post(
    request: &Value,
    key: &SecretString,
    endpoint: Url,
    budget: &RequestBudget,
    retries: u32,
) -> Result<Value, ClassificationError> {
    let client = shared_client()?;
    for attempt in 0..=retries {
        check_budget(budget)?;
        let response = wait(
            budget,
            client
                .post(endpoint.clone())
                .bearer_auth(key.expose_secret())
                .header(reqwest::header::ACCEPT, "application/json")
                .json(request)
                .send(),
        )
        .await?
        .map_err(|_| transport_error())?;
        let status = response.status();
        if matches!(status.as_u16(), 429 | 503 | 529) && attempt < retries {
            let delay = retry_delay(response.headers(), attempt);
            if delay >= budget.deadline.saturating_duration_since(Instant::now()) {
                return Err(ClassificationError::new(
                    "timeout",
                    format!(
                        "HTTP {status}: retry delay exceeds the remaining classification deadline."
                    ),
                    "Retry later.",
                ));
            }
            drop(response);
            wait(budget, tokio::time::sleep(delay)).await?;
            continue;
        }
        if !status.is_success() {
            let hint = match status.as_u16() {
                401 | 403 => "Check OCTOCODE_CLASSIFICATION_API and account access.",
                400 | 422 => {
                    "Check the supplied state, questions, configured model, and request size."
                }
                429 | 503 | 529 => "Retry later or reduce request volume.",
                300..=399 => "Redirects are disabled; check OCTOCODE_CLASSIFICATION_API_HOST.",
                _ => "Check provider availability and OCTOCODE_CLASSIFICATION_API_HOST.",
            };
            return Err(ClassificationError::new(
                "classificationProviderError",
                format!("Classification provider returned HTTP {status}; response body omitted."),
                hint,
            ));
        }
        let mut body = BytesMut::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = wait(budget, stream.next()).await? {
            let chunk = chunk.map_err(|_| transport_error())?;
            if body.len().saturating_add(chunk.len()) > budget.max_body_bytes {
                return Err(ClassificationError::new(
                    "invalidClassificationResponse",
                    "Classification provider response exceeded the 4 MiB limit.",
                    "Reduce the supplied state or question count.",
                ));
            }
            body.extend_from_slice(&chunk);
        }
        return serde_json::from_slice(&body).map_err(|_| {
            ClassificationError::new(
                "invalidClassificationResponse",
                "Classification provider returned invalid JSON.",
                "Inspect provider compatibility before using the response.",
            )
        });
    }
    Err(transport_error())
}

pub fn budget(
    deadline: Instant,
    cancellation: tokio_util::sync::CancellationToken,
) -> RequestBudget {
    RequestBudget {
        deadline,
        cancellation,
        max_body_bytes: MAX_BODY_BYTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use serde_json::json;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    #[tokio::test]
    async fn transport_rejects_redirects_and_oversized_responses() {
        for (response, expected) in [
            (
                ResponseTemplate::new(302).insert_header("location", "https://example.com"),
                "classificationProviderError",
            ),
            (
                ResponseTemplate::new(200).set_body_string("x".repeat(65)),
                "invalidClassificationResponse",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(response)
                .expect(1)
                .mount(&server)
                .await;
            let mut budget = budget(
                Instant::now() + Duration::from_secs(30),
                tokio_util::sync::CancellationToken::new(),
            );
            budget.max_body_bytes = 64;
            let error = post(
                &json!({}),
                &SecretString::from("test-key".to_owned()),
                endpoint(&server.uri(), "v1/systemone").unwrap(),
                &budget,
                0,
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, expected);
        }
    }

    #[tokio::test]
    async fn expired_deadline_prevents_transport() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let error = post(
            &json!({}),
            &SecretString::from("test-key".to_owned()),
            endpoint(&server.uri(), "v1/systemone").unwrap(),
            &budget(Instant::now(), tokio_util::sync::CancellationToken::new()),
            0,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "timeout");
    }

    #[test]
    fn endpoint_accepts_an_https_root_and_appends_the_vendor_path() {
        let url = endpoint("https://api.typesafe.ai", "v1/systemone").expect("valid root");
        assert_eq!(url.as_str(), "https://api.typesafe.ai/v1/systemone");
        // A trailing slash is also a bare root.
        assert!(endpoint("https://api.typesafe.ai/", "v1/systemone").is_ok());
        // HTTP is allowed only on loopback.
        assert!(endpoint("http://127.0.0.1", "v1/systemone").is_ok());
    }

    #[test]
    fn endpoint_rejects_non_root_or_insecure_bases() {
        for bad in [
            "http://api.typesafe.ai",            // http, non-loopback
            "https://api.typesafe.ai/v1",        // has a path
            "https://api.typesafe.ai/?x=1",      // has a query
            "https://api.typesafe.ai/#frag",     // has a fragment
            "https://user:pass@api.typesafe.ai", // has credentials
            "not a url",
        ] {
            assert!(
                endpoint(bad, "v1/systemone").is_err(),
                "should reject {bad}"
            );
        }
    }

    #[test]
    fn retry_delay_honours_headers_then_falls_back_to_backoff() {
        let mut ms = HeaderMap::new();
        ms.insert("retry-after-ms", HeaderValue::from_static("1500"));
        assert_eq!(retry_delay(&ms, 3), Duration::from_millis(1500));

        let mut secs = HeaderMap::new();
        secs.insert(RETRY_AFTER, HeaderValue::from_static("2"));
        assert_eq!(retry_delay(&secs, 3), Duration::from_secs(2));

        // No headers: 500ms * 2^attempt.
        let empty = HeaderMap::new();
        assert_eq!(retry_delay(&empty, 0), Duration::from_millis(500));
        assert_eq!(retry_delay(&empty, 2), Duration::from_millis(2000));
    }
}
