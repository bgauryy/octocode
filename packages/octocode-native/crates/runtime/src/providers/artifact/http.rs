use super::{ArtifactError, ArtifactSearchQueryType};
use crate::cache::{BoundedCache, CacheConfig, CacheKey, CacheLookup, CachePartition};
use crate::providers::RequestBudget;
use bytes::BytesMut;
use futures_util::StreamExt;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue, USER_AGENT};
use secrecy::{ExposeSecret, SecretString};
use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use url::Url;

fn artifact_cache() -> &'static Mutex<BoundedCache<Vec<u8>>> {
    static CACHE: OnceLock<Mutex<BoundedCache<Vec<u8>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(BoundedCache::new(CacheConfig::default())))
}

fn cache_key(url: &Url) -> CacheKey {
    CacheKey {
        namespace: "artifact".into(),
        resource: url.as_str().to_owned(),
        partition: CachePartition {
            endpoint: url.host_str().unwrap_or("registry").to_owned(),
            credential_fingerprint: "anonymous".into(),
        },
    }
}

pub type ArtifactHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ArtifactHttpResponse, ArtifactError>> + Send + 'a>>;

#[derive(Clone)]
pub struct ArtifactHttpRequest {
    pub url: Url,
    pub accept: &'static str,
    pub authorization: Option<SecretString>,
    pub(crate) dns_pin: Option<DnsPin>,
}

impl fmt::Debug for ArtifactHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ArtifactHttpRequest")
            .field("url", &self.url)
            .field("accept", &self.accept)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// A custom registry DNS result that was checked by the registry policy and
/// must be reused for the actual connection. Reusing these exact addresses
/// closes the validation-to-connect DNS rebinding window.
#[derive(Clone, Debug)]
pub(crate) struct DnsPin {
    pub host: String,
    pub addresses: Vec<SocketAddr>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactHttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait ArtifactHttp: Send + Sync {
    fn get<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a>;
}

#[derive(Clone)]
pub struct SystemArtifactHttp {
    client: reqwest::Client,
}

impl SystemArtifactHttp {
    pub fn new() -> Result<Self, ArtifactError> {
        let client = build_client(None)?;
        Ok(Self { client })
    }
}

fn build_client(dns_pin: Option<&DnsPin>) -> Result<reqwest::Client, ArtifactError> {
    let mut builder = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none());
    if let Some(pin) = dns_pin {
        // A proxy would resolve the target independently and defeat the pin.
        // Custom registries admitted by this path therefore connect directly
        // to the exact public addresses validated by npm policy.
        builder = builder
            .no_proxy()
            .resolve_to_addrs(&pin.host, &pin.addresses);
    }
    builder.build().map_err(|_| {
        ArtifactError::new(
            "provider_error",
            "Failed to initialize artifact registry HTTP client.",
        )
    })
}

impl ArtifactHttp for SystemArtifactHttp {
    fn get<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        Box::pin(async move {
            let client = match request.dns_pin.as_ref() {
                Some(pin) => build_client(Some(pin))?,
                None => self.client.clone(),
            };
            for attempt in 0..2 {
                check_budget(budget)?;
                let mut builder = client
                    .get(request.url.clone())
                    .header(USER_AGENT, "octocode-rust/1")
                    .header(ACCEPT, request.accept);
                if let Some(authorization) = request.authorization.as_ref() {
                    let header =
                        HeaderValue::from_str(authorization.expose_secret()).map_err(|_| {
                            ArtifactError::new(
                                "authentication",
                                "Resolved registry authorization is invalid.",
                            )
                        })?;
                    builder = builder.header(AUTHORIZATION, header);
                }
                let response = wait(budget, builder.send())
                    .await?
                    .map_err(transport_error)?;
                let status = response.status();
                if (status.is_server_error() || status.as_u16() == 429) && attempt == 0 {
                    wait_delay(budget, Duration::from_millis(200)).await?;
                    continue;
                }
                if status.is_redirection() {
                    return Err(ArtifactError::new(
                        "provider_error",
                        "Artifact registry redirects are not followed.",
                    )
                    .with_status(status.as_u16()));
                }
                let mut body = BytesMut::new();
                let mut stream = response.bytes_stream();
                while let Some(chunk) = wait(budget, stream.next()).await? {
                    let chunk = chunk.map_err(transport_error)?;
                    if body.len().saturating_add(chunk.len()) > budget.max_body_bytes {
                        return Err(ArtifactError::new(
                            "provider_error",
                            "Artifact registry response exceeded the configured body limit.",
                        ));
                    }
                    body.extend_from_slice(&chunk);
                }
                return Ok(ArtifactHttpResponse {
                    status: status.as_u16(),
                    body: body.to_vec(),
                });
            }
            Err(ArtifactError::new(
                "provider_error",
                "Artifact registry request failed. Retry later.",
            ))
        })
    }
}

fn check_budget(budget: &RequestBudget) -> Result<(), ArtifactError> {
    if budget.cancellation.is_cancelled() {
        return Err(ArtifactError::new(
            "cancelled",
            "Artifact registry request was cancelled.",
        ));
    }
    if Instant::now() >= budget.deadline {
        return Err(ArtifactError::new(
            "timeout",
            "Artifact registry request exceeded its deadline.",
        ));
    }
    Ok(())
}

async fn wait<T>(
    budget: &RequestBudget,
    future: impl Future<Output = T>,
) -> Result<T, ArtifactError> {
    check_budget(budget)?;
    let remaining = budget.deadline.saturating_duration_since(Instant::now());
    tokio::select! {
        _ = budget.cancellation.cancelled() => Err(ArtifactError::new("cancelled", "Artifact registry request was cancelled.")),
        value = tokio::time::timeout(remaining, future) => value.map_err(|_| ArtifactError::new("timeout", "Artifact registry request exceeded its deadline.")),
    }
}

async fn wait_delay(budget: &RequestBudget, duration: Duration) -> Result<(), ArtifactError> {
    wait(budget, tokio::time::sleep(duration)).await
}

fn transport_error(_error: impl fmt::Display) -> ArtifactError {
    ArtifactError::new(
        "provider_error",
        "Artifact registry request failed. Retry later.",
    )
}

pub(crate) struct RegistryClient<'a> {
    pub http: &'a dyn ArtifactHttp,
    pub budget: &'a RequestBudget,
    /// Config revision used to key the in-process cache.  Changing this value
    /// (e.g. when `storage.mode` or other settings change) causes the
    /// `BoundedCache` to treat every existing entry as stale and evict it on
    /// the next access.
    pub cache_revision: u64,
    /// When `false` the in-process registry cache is bypassed for both reads
    /// and writes.  Set to `false` when `storage.mode == "memory"` so that
    /// an operator can disable all caching without restarting the process.
    pub cache_enabled: bool,
}

impl RegistryClient<'_> {
    pub async fn json(
        &self,
        artifact_type: ArtifactSearchQueryType,
        url: Url,
        not_found_is_empty: bool,
        authorization: Option<SecretString>,
    ) -> Result<Option<serde_json::Value>, ArtifactError> {
        self.json_with_dns_pin(artifact_type, url, not_found_is_empty, authorization, None)
            .await
    }

    pub(crate) async fn json_with_dns_pin(
        &self,
        artifact_type: ArtifactSearchQueryType,
        url: Url,
        not_found_is_empty: bool,
        authorization: Option<SecretString>,
        dns_pin: Option<DnsPin>,
    ) -> Result<Option<serde_json::Value>, ArtifactError> {
        let anonymous = authorization.is_none();
        if anonymous && self.cache_enabled {
            let key = cache_key(&url);
            let hit = artifact_cache()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get(&key, self.cache_revision, None, Instant::now());
            if let CacheLookup::Hit { value, .. } = hit {
                return serde_json::from_slice(value.as_ref())
                    .map(Some)
                    .map_err(|_| invalid_response(artifact_type));
            }
        }
        let response = self
            .http
            .get(
                ArtifactHttpRequest {
                    url: url.clone(),
                    accept: "application/json",
                    authorization,
                    dns_pin,
                },
                self.budget,
            )
            .await?;
        let body = self.status(artifact_type, response, not_found_is_empty)?;
        if anonymous
            && self.cache_enabled
            && let Some(bytes) = body.as_ref()
        {
            let key = cache_key(&url);
            artifact_cache()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(
                    key,
                    bytes.clone(),
                    bytes.len(),
                    self.cache_revision,
                    Instant::now(),
                );
        }
        body.map(|bytes| {
            serde_json::from_slice(&bytes).map_err(|_| invalid_response(artifact_type))
        })
        .transpose()
    }

    pub async fn text(
        &self,
        artifact_type: ArtifactSearchQueryType,
        url: Url,
        not_found_is_empty: bool,
    ) -> Result<Option<String>, ArtifactError> {
        let response = self
            .http
            .get(
                ArtifactHttpRequest {
                    url,
                    accept: "application/xml",
                    authorization: None,
                    dns_pin: None,
                },
                self.budget,
            )
            .await?;
        let body = self.status(artifact_type, response, not_found_is_empty)?;
        body.map(|bytes| String::from_utf8(bytes).map_err(|_| invalid_response(artifact_type)))
            .transpose()
    }

    fn status(
        &self,
        artifact_type: ArtifactSearchQueryType,
        response: ArtifactHttpResponse,
        not_found_is_empty: bool,
    ) -> Result<Option<Vec<u8>>, ArtifactError> {
        match response.status {
            200..=299 => Ok(Some(response.body)),
            404 if not_found_is_empty => Ok(None),
            401 | 403 => Err(ArtifactError::new(
                "authentication",
                format!("{} denied registry access.", artifact_type.as_str()),
            )
            .with_status(response.status)),
            429 => Err(ArtifactError::new(
                "rate_limit",
                format!(
                    "{} rate limit reached. Retry later.",
                    artifact_type.as_str()
                ),
            )
            .with_status(response.status)),
            // Remaining 4xx (except 408) are deterministic request errors:
            // retrying cannot help, so name the status and blame the query.
            status @ 400..=499 if status != 408 => Err(ArtifactError::new(
                "invalid_query",
                format!(
                    "{} registry rejected the request (HTTP {status}). Check the package name or query.",
                    artifact_type.as_str()
                ),
            )
            .with_status(status)),
            status => Err(ArtifactError::new(
                "provider_error",
                format!(
                    "{} registry request failed (HTTP {status}). Retry later.",
                    artifact_type.as_str()
                ),
            )
            .with_status(status)),
        }
    }
}

pub(crate) fn invalid_response(artifact_type: ArtifactSearchQueryType) -> ArtifactError {
    ArtifactError::new(
        "provider_error",
        format!(
            "{} returned an invalid registry response.",
            artifact_type.as_str()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NoHttp;

    impl ArtifactHttp for NoHttp {
        fn get<'a>(
            &'a self,
            _request: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            Box::pin(async { panic!("status mapping tests never issue requests") })
        }
    }

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// An `ArtifactHttp` impl that records every call and returns a fixed body.
    struct CountingHttp {
        calls: Arc<AtomicUsize>,
        body: Vec<u8>,
    }

    impl ArtifactHttp for CountingHttp {
        fn get<'a>(
            &'a self,
            _request: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let body = self.body.clone();
            Box::pin(async move { Ok(ArtifactHttpResponse { status: 200, body }) })
        }
    }

    fn classify(status: u16) -> Result<Option<Vec<u8>>, ArtifactError> {
        let budget = RequestBudget::with_timeout(Duration::from_secs(10), 10_000_000);
        let client = RegistryClient {
            http: &NoHttp,
            budget: &budget,
            cache_revision: 0,
            cache_enabled: false,
        };
        client.status(
            ArtifactSearchQueryType::Npm,
            ArtifactHttpResponse {
                status,
                body: vec![],
            },
            true,
        )
    }

    #[test]
    fn deterministic_4xx_is_invalid_query_with_status() {
        // npm answers exact lookups for malformed names (e.g. non-ASCII
        // coordinates) with 405, not 404; it must not read as retryable.
        for status in [400u16, 405, 422] {
            let error = classify(status).expect_err("4xx is an error");
            assert_eq!(error.code, "invalid_query");
            assert_eq!(error.status, Some(status));
            assert!(
                error.message.contains(&format!("HTTP {status}")),
                "{}",
                error.message
            );
        }
    }

    #[test]
    fn server_errors_and_408_stay_retryable_provider_error() {
        for status in [408u16, 500, 502, 503] {
            let error = classify(status).expect_err("5xx is an error");
            assert_eq!(error.code, "provider_error");
            assert_eq!(error.status, Some(status));
            assert!(
                error.message.contains(&format!("HTTP {status}"))
                    && error.message.contains("Retry later"),
                "{}",
                error.message
            );
        }
    }

    #[test]
    fn auth_rate_limit_and_not_found_keep_dedicated_mappings() {
        assert_eq!(classify(404).expect("404 maps to empty"), None);
        assert_eq!(classify(401).expect_err("401").code, "authentication");
        assert_eq!(classify(403).expect_err("403").code, "authentication");
        let limited = classify(429).expect_err("429");
        assert_eq!(limited.code, "rate_limit");
        assert_eq!(limited.status, Some(429));
    }

    /// `cache_enabled: false` must bypass the in-process cache on both reads
    /// and writes: consecutive anonymous calls for the same URL always reach
    /// the HTTP layer.
    #[tokio::test]
    async fn cache_disabled_skips_cache_for_reads_and_writes() {
        let calls = Arc::new(AtomicUsize::new(0));
        let http = CountingHttp {
            calls: Arc::clone(&calls),
            body: b"{\"name\":\"test\"}".to_vec(),
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(10), 10_000_000);
        let client = RegistryClient {
            http: &http,
            budget: &budget,
            cache_revision: 42,
            cache_enabled: false, // <-- cache must be skipped
        };
        let url = Url::parse("https://cache-disabled-test.invalid/pkg").expect("test URL");
        // First call — must hit HTTP.
        let _ = client
            .json(ArtifactSearchQueryType::Npm, url.clone(), false, None)
            .await;
        assert_eq!(calls.load(Ordering::Relaxed), 1, "first call must hit HTTP");
        // Second call with the same URL — must hit HTTP again (nothing written to cache).
        let _ = client.json(ArtifactSearchQueryType::Npm, url, false, None).await;
        assert_eq!(
            calls.load(Ordering::Relaxed),
            2,
            "second call must hit HTTP when cache_enabled=false"
        );
    }

    /// When `cache_revision` advances, entries written under the previous
    /// revision must not be served — the `BoundedCache` revision check treats
    /// them as stale and evicts them on the next access.
    #[tokio::test]
    async fn cache_revision_change_invalidates_stale_entries() {
        let calls = Arc::new(AtomicUsize::new(0));
        let http = CountingHttp {
            calls: Arc::clone(&calls),
            body: b"{\"name\":\"serde\"}".to_vec(),
        };
        let budget = RequestBudget::with_timeout(Duration::from_secs(10), 10_000_000);
        // Use a URL unlikely to collide with other parallel tests.
        let url = Url::parse(&format!(
            "https://cache-revision-test.invalid/pkg-{}",
            std::process::id()
        ))
        .expect("test URL");

        // Populate the cache at revision 1.
        let client_rev1 = RegistryClient {
            http: &http,
            budget: &budget,
            cache_revision: 1,
            cache_enabled: true,
        };
        let _ = client_rev1
            .json(ArtifactSearchQueryType::Npm, url.clone(), false, None)
            .await;
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "revision-1 write must hit HTTP"
        );

        // Read at the same revision — must be a cache hit (HTTP not called again).
        let _ = client_rev1
            .json(ArtifactSearchQueryType::Npm, url.clone(), false, None)
            .await;
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "same-revision read must be a cache hit"
        );

        // Read at a newer revision — the stale entry must be evicted and HTTP called.
        let client_rev2 = RegistryClient {
            http: &http,
            budget: &budget,
            cache_revision: 2,
            cache_enabled: true,
        };
        let _ = client_rev2.json(ArtifactSearchQueryType::Npm, url, false, None).await;
        assert_eq!(
            calls.load(Ordering::Relaxed),
            2,
            "incremented revision must invalidate the cached entry"
        );
    }

    #[tokio::test]
    async fn dns_pin_connects_to_the_validated_address() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/package"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;
        let port = server.address().port();
        let request = ArtifactHttpRequest {
            url: Url::parse(&format!("http://registry.invalid:{port}/package")).expect("test URL"),
            accept: "application/json",
            authorization: None,
            dns_pin: Some(DnsPin {
                host: "registry.invalid".to_owned(),
                addresses: vec![*server.address()],
            }),
        };
        // The full native suite runs several network/process fixtures in
        // parallel. This test proves address pinning, not latency, so keep its
        // fixture deadline wide enough to avoid scheduler-starvation flakes.
        let budget = RequestBudget::with_timeout(Duration::from_secs(60), 1024);
        let response = SystemArtifactHttp::new()
            .expect("HTTP client")
            .get(request, &budget)
            .await
            .expect("pinned request reaches the validated address");
        assert_eq!(response.status, 200);
    }
}
