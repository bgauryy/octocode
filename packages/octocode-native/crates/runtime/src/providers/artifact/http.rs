use super::{ArtifactError, ArtifactType};
use crate::cache::{CacheClass, CacheConfig, CacheKey, Store};
use crate::providers::RuntimeClients;
use crate::providers::artifact::types::artifact_type;
use octocode_contract_types::ErrorCode;
use octocode_io::budget::{BudgetStop, RequestBudget};
use octocode_io::retry::{Backoff, BodyError, StatusClass};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue, USER_AGENT};
use secrecy::{ExposeSecret, SecretString};
use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::ops::ControlFlow;
use std::pin::Pin;
use std::time::Duration;
use url::Url;

/// One runtime's registry answers and release facts. A disk directory
/// keeps them across CLI processes; without one they stay in memory.
pub struct ArtifactCache(Store<serde_json::Value>);

impl ArtifactCache {
    pub fn new(dir: Option<std::path::PathBuf>) -> Self {
        Self(Store::new(CacheConfig::default(), dir))
    }
}

/// A registry answer lives five minutes: a new release shows up soon.
const REGISTRY_ANSWER: CacheClass = CacheClass::Volatile {
    ttl: Duration::from_secs(300),
};

/// A read's key: the registry host scoped by the exact credential
/// ([`octocode_github::Partition`]), so one token's answers never serve
/// another caller.
fn cache_key(url: &Url, accept: &str, authorization: Option<&SecretString>) -> CacheKey {
    // One URL can answer in several representations (npm's abbreviated
    // packument); the default JSON keeps its historical key.
    let resource = if accept == JSON {
        url.as_str().to_owned()
    } else {
        format!("{accept} {url}")
    };
    CacheKey {
        namespace: "artifact".into(),
        scope: octocode_github::Partition::new(url.host_str().unwrap_or("registry"), authorization)
            .identity()
            .to_owned(),
        resource,
    }
}

/// Release facts are public: one unscoped namespace.
fn fact_key(resource: &str) -> CacheKey {
    CacheKey {
        namespace: "artifact-fact".into(),
        scope: String::new(),
        resource: resource.to_owned(),
    }
}

/// Page cursors of public registry searches: one unscoped namespace.
fn cursor_key(resource: &str) -> CacheKey {
    CacheKey {
        namespace: "artifact-cursor".into(),
        scope: String::new(),
        resource: resource.to_owned(),
    }
}

pub(crate) const JSON: &str = "application/json";
/// npm's abbreviated packument: versions and dist-tags without readmes.
pub(crate) const NPM_INSTALL_JSON: &str = "application/vnd.npm.install-v1+json";

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

/// Takes one body chunk; `Break` ends the read before the body does.
pub type BodySink<'a> = &'a mut (dyn FnMut(&[u8]) -> ControlFlow<()> + Send);
pub type StatusFuture<'a> = Pin<Box<dyn Future<Output = Result<u16, ArtifactError>> + Send + 'a>>;

pub trait ArtifactHttp: Send + Sync {
    fn get<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a>;

    /// Feeds a 2xx body to `sink` as it arrives, until the sink breaks or
    /// the body ends, and returns the answer's status (another status feeds
    /// nothing). By default the whole body is read with [`Self::get`] and
    /// fed once.
    fn stream<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
        sink: BodySink<'a>,
    ) -> StatusFuture<'a> {
        Box::pin(async move {
            let response = self.get(request, budget).await?;
            if (200..300).contains(&response.status) {
                let _ = sink(&response.body);
            }
            Ok(response.status)
        })
    }
}

#[derive(Clone)]
pub struct SystemArtifactHttp {
    client: reqwest::Client,
    /// Retries after a 5xx or 429 (the runtime's `max_retries`).
    retries: u8,
}

impl SystemArtifactHttp {
    /// The registry client for the current Tokio runtime: one connection pool
    /// and TLS configuration for every call on it. A DNS-pinned registry
    /// request uses a client pinned to its validated addresses (see
    /// [`pinned_client`]).
    pub fn shared() -> Result<Self, ArtifactError> {
        static CLIENTS: RuntimeClients<()> = RuntimeClients::new(MAX_RUNTIME_CLIENTS);
        CLIENTS
            .get((), || build_client(None))
            .map(|client| Self { client, retries: 1 })
    }

    /// Retry a 5xx or 429 answer up to `retries` times.
    #[must_use]
    pub fn with_retries(mut self, retries: u8) -> Self {
        self.retries = retries;
        self
    }
}

/// Clients kept for reuse per runtime; the oldest is dropped past this count.
const MAX_RUNTIME_CLIENTS: usize = 8;

/// The client pinned to `pin`'s exact host and validated addresses, reused
/// across calls on one runtime so a later GET to the same registry keeps its
/// pooled connection and TLS session. A different address set (a new DNS
/// answer) is a different key, so a reused client never connects anywhere the
/// registry policy did not validate for this call.
fn pinned_client(pin: &DnsPin) -> Result<reqwest::Client, ArtifactError> {
    static CLIENTS: RuntimeClients<(String, Vec<SocketAddr>)> =
        RuntimeClients::new(MAX_RUNTIME_CLIENTS);
    let mut addresses = pin.addresses.clone();
    addresses.sort_unstable();
    addresses.dedup();
    CLIENTS.get((pin.host.to_ascii_lowercase(), addresses), || {
        build_client(Some(pin))
    })
}

fn build_client(dns_pin: Option<&DnsPin>) -> Result<reqwest::Client, ArtifactError> {
    // A pinned name connects directly to the exact public addresses validated
    // by npm policy: a proxy would resolve the target independently.
    let policy = octocode_io::http::ClientPolicy {
        proxy: if dns_pin.is_some() {
            octocode_io::http::Proxy::Direct
        } else {
            octocode_io::http::Proxy::System
        },
        resolve: dns_pin.map(|pin| (pin.host.clone(), pin.addresses.clone())),
        ..octocode_io::http::ClientPolicy::default()
    };
    octocode_io::http::client(&policy).map_err(|_| {
        ArtifactError::new(
            ErrorCode::ProviderError,
            "Failed to initialize artifact registry HTTP client.",
        )
    })
}

impl SystemArtifactHttp {
    /// The client a request connects through: the shared one, or the one
    /// pinned to its validated addresses.
    fn client_for(&self, request: &ArtifactHttpRequest) -> Result<reqwest::Client, ArtifactError> {
        match request.dns_pin.as_ref() {
            Some(pin) => pinned_client(pin),
            None => Ok(self.client.clone()),
        }
    }

    /// The answer to `request` once its headers arrive, and the attempt
    /// (from `first`) that got it: a failed connection, a 5xx or a 429 is
    /// retried within the retries and the request's time; a redirect is
    /// refused.
    async fn send(
        &self,
        client: &reqwest::Client,
        request: &ArtifactHttpRequest,
        budget: &RequestBudget,
        first: u8,
    ) -> Result<(reqwest::Response, u8), ArtifactError> {
        for attempt in first..=self.retries {
            budget.check().map_err(budget_error)?;
            let mut builder = client
                .get(request.url.clone())
                .header(USER_AGENT, "octocode-rust/1")
                .header(ACCEPT, request.accept);
            if let Some(authorization) = request.authorization.as_ref() {
                let header =
                    HeaderValue::from_str(authorization.expose_secret()).map_err(|_| {
                        ArtifactError::new(
                            ErrorCode::Authentication,
                            "Resolved registry authorization is invalid.",
                        )
                    })?;
                builder = builder.header(AUTHORIZATION, header);
            }
            let response = match budget.wait(builder.send()).await.map_err(budget_error)? {
                Ok(response) => response,
                Err(error) => {
                    if octocode_io::retry::is_retryable_send(&error)
                        && attempt < self.retries
                        && let Some(delay) = retry_delay(None, attempt, budget)
                    {
                        pause(delay, budget).await?;
                        continue;
                    }
                    return Err(transport_error(error));
                }
            };
            let status = response.status();
            let class = octocode_io::retry::classify_status(status.as_u16());
            if (class.is_server() || class == StatusClass::RateLimited)
                && attempt < self.retries
                && let Some(delay) = retry_delay(Some(response.headers()), attempt, budget)
            {
                drain(response, budget).await?;
                pause(delay, budget).await?;
                continue;
            }
            if class == StatusClass::Redirect {
                return Err(ArtifactError::new(
                    ErrorCode::ProviderError,
                    "Artifact registry redirects are not followed.",
                )
                .with_status(status.as_u16()));
            }
            return Ok((response, attempt));
        }
        Err(ArtifactError::new(
            ErrorCode::ProviderError,
            "Artifact registry request failed. Retry later.",
        ))
    }
}

impl ArtifactHttp for SystemArtifactHttp {
    fn get<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        Box::pin(async move {
            let client = self.client_for(&request)?;
            let mut first = 0;
            loop {
                let (response, attempt) = self.send(&client, &request, budget, first).await?;
                let status = response.status().as_u16();
                match octocode_io::retry::read_bounded(response, budget, budget.max_body_bytes)
                    .await
                {
                    Ok(body) => {
                        return Ok(ArtifactHttpResponse {
                            status,
                            body: body.into(),
                        });
                    }
                    Err(BodyError::Stopped(stop)) => return Err(budget_error(stop)),
                    Err(BodyError::TooLarge) => return Err(body_too_large()),
                    // A registry read is an idempotent GET: a body that
                    // breaks off is retried like a failed connection.
                    Err(BodyError::Transport) => {
                        if attempt < self.retries
                            && let Some(delay) = retry_delay(None, attempt, budget)
                        {
                            pause(delay, budget).await?;
                            first = attempt + 1;
                            continue;
                        }
                        return Err(transport_error("body"));
                    }
                }
            }
        })
    }

    /// Chunks as they arrive: a sink that found what it reads stops the
    /// download there. A body that breaks off is not retried (the sink
    /// already took its start).
    fn stream<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
        sink: BodySink<'a>,
    ) -> StatusFuture<'a> {
        Box::pin(async move {
            let client = self.client_for(&request)?;
            let (mut response, _) = self.send(&client, &request, budget, 0).await?;
            let status = response.status().as_u16();
            if !(200..300).contains(&status) {
                drain(response, budget).await?;
                return Ok(status);
            }
            let mut read = 0usize;
            while let Some(chunk) = budget
                .wait(response.chunk())
                .await
                .map_err(budget_error)?
                .map_err(transport_error)?
            {
                read = read.saturating_add(chunk.len());
                if read > budget.max_body_bytes {
                    return Err(body_too_large());
                }
                if sink(&chunk).is_break() {
                    break;
                }
            }
            Ok(status)
        })
    }
}

fn body_too_large() -> ArtifactError {
    ArtifactError::new(
        ErrorCode::ProviderError,
        "Artifact registry response exceeded the configured body limit.",
    )
}

/// The most of an error body read before a retry. Reading a short body to
/// its end hands the connection back to the pool; a longer one is dropped
/// with its connection.
const MAX_DRAIN_BYTES: usize = 64 * 1024;

/// Read and discard a retried response's body, up to [`MAX_DRAIN_BYTES`].
async fn drain(response: reqwest::Response, budget: &RequestBudget) -> Result<(), ArtifactError> {
    match octocode_io::retry::read_bounded(response, budget, MAX_DRAIN_BYTES).await {
        Err(BodyError::Stopped(stop)) => Err(budget_error(stop)),
        Ok(_) | Err(BodyError::TooLarge | BodyError::Transport) => Ok(()),
    }
}

async fn pause(delay: Duration, budget: &RequestBudget) -> Result<(), ArtifactError> {
    budget
        .wait(tokio::time::sleep(delay))
        .await
        .map_err(budget_error)
}

/// Longest wait before a retry; a registry that asks for more gets its
/// answer returned now.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(10);
const BACKOFF: Backoff = Backoff {
    base: Duration::from_millis(200),
    cap: MAX_RETRY_WAIT,
    floor: Duration::ZERO,
};

/// The wait before retrying: the registry's `Retry-After`, else jittered
/// backoff. `None` when that wait exceeds [`MAX_RETRY_WAIT`] or the
/// request's remaining time.
fn retry_delay(
    headers: Option<&reqwest::header::HeaderMap>,
    attempt: u8,
    budget: &RequestBudget,
) -> Option<Duration> {
    let requested = headers.and_then(|headers| {
        octocode_io::retry::retry_after_delay(
            headers,
            Duration::from_secs(86_400),
            std::time::SystemTime::now(),
        )
    });
    let delay = BACKOFF.after(requested, u32::from(attempt));
    (delay <= MAX_RETRY_WAIT && std::time::Instant::now() + delay < budget.deadline)
        .then_some(delay)
}

fn budget_error(stop: BudgetStop) -> ArtifactError {
    match stop {
        BudgetStop::Cancelled => ArtifactError::new(
            ErrorCode::Cancelled,
            "Artifact registry request was cancelled.",
        ),
        BudgetStop::Deadline => ArtifactError::new(
            ErrorCode::Timeout,
            "Artifact registry request exceeded its deadline.",
        ),
    }
}

fn transport_error(_error: impl fmt::Display) -> ArtifactError {
    ArtifactError::new(
        ErrorCode::ProviderError,
        "Artifact registry request failed. Retry later.",
    )
}

pub(crate) struct RegistryClient<'a> {
    pub http: &'a dyn ArtifactHttp,
    pub budget: &'a RequestBudget,
    /// The runtime's registry cache; `None` (`storage.mode == "memory"`)
    /// bypasses it for both reads and writes.
    pub cache: Option<&'a ArtifactCache>,
    /// Upstream release-tag checks; `None` reads no tags.
    pub tags: Option<&'a dyn super::ReleaseTags>,
}

#[cfg(test)]
impl<'a> RegistryClient<'a> {
    /// A client that skips the registry cache and reads no release tags.
    pub(crate) fn uncached(http: &'a dyn ArtifactHttp, budget: &'a RequestBudget) -> Self {
        Self {
            http,
            budget,
            cache: None,
            tags: None,
        }
    }
}

impl RegistryClient<'_> {
    /// A registry's JSON answer for `url` in the `accept` representation
    /// ([`JSON`] by default), through the runtime cache. `dns_pin` connects a
    /// custom registry only to its validated addresses; `None` when absent
    /// (404) and `not_found_is_empty`.
    pub async fn json(
        &self,
        artifact_type: ArtifactType,
        url: Url,
        not_found_is_empty: bool,
        authorization: Option<SecretString>,
        dns_pin: Option<DnsPin>,
        accept: &'static str,
    ) -> Result<Option<serde_json::Value>, ArtifactError> {
        let key = cache_key(&url, accept, authorization.as_ref());
        let request = ArtifactHttpRequest {
            url,
            accept,
            authorization,
            dns_pin,
        };
        self.cached(artifact_type, key, request, not_found_is_empty, |bytes| {
            let value = serde_json::from_slice(bytes).ok()?;
            Some((value, bytes.len()))
        })
        .await
    }

    /// [`Self::json`] reduced to the part a caller reads (`view`, decoded
    /// straight from the body) before it is cached: an 8 MB packument read
    /// for its version list keeps only that list. `name` keys the view apart
    /// from the full answer.
    pub(crate) async fn json_view(
        &self,
        artifact_type: ArtifactType,
        url: Url,
        not_found_is_empty: bool,
        authorization: Option<SecretString>,
        dns_pin: Option<DnsPin>,
        accept: &'static str,
        name: &str,
        view: fn(&[u8]) -> Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, ArtifactError> {
        let mut key = cache_key(&url, accept, authorization.as_ref());
        key.resource = format!("{} #{name}", key.resource);
        let request = ArtifactHttpRequest {
            url,
            accept,
            authorization,
            dns_pin,
        };
        self.cached(artifact_type, key, request, not_found_is_empty, |bytes| {
            let value = view(bytes)?;
            let weight = octocode_engine::json_size::bytes(&value).unwrap_or(usize::MAX);
            Some((value, weight))
        })
        .await
    }

    /// An XML answer (Maven metadata and POMs, NuGet nuspecs) through the
    /// same cache as JSON answers.
    pub async fn text(
        &self,
        artifact_type: ArtifactType,
        url: Url,
        not_found_is_empty: bool,
    ) -> Result<Option<String>, ArtifactError> {
        const XML: &str = "application/xml";
        let key = cache_key(&url, XML, None);
        let request = ArtifactHttpRequest {
            url,
            accept: XML,
            authorization: None,
            dns_pin: None,
        };
        let value = self
            .cached(artifact_type, key, request, not_found_is_empty, |bytes| {
                let text = std::str::from_utf8(bytes).ok()?;
                Some((serde_json::Value::String(text.to_owned()), bytes.len()))
            })
            .await?;
        value
            .map(|value| match value {
                serde_json::Value::String(text) => Ok(text),
                _ => Err(super::util::invalid(artifact_type)),
            })
            .transpose()
    }

    /// The answer under `key`, else the registry's answer to `request`
    /// decoded (with its cache weight) and cached. Entries are stored
    /// decoded: a hit copies the answer, never re-decodes it. Only a body
    /// that decodes is cached, so a malformed answer never outlives the call
    /// that received it.
    async fn cached(
        &self,
        artifact_type: ArtifactType,
        key: CacheKey,
        request: ArtifactHttpRequest,
        not_found_is_empty: bool,
        decode: impl FnOnce(&[u8]) -> Option<(serde_json::Value, usize)>,
    ) -> Result<Option<serde_json::Value>, ArtifactError> {
        // An entry that does not deserialize is a miss and is removed.
        if let Some(cache) = self.cache
            && let Some(hit) = cache.0.get(&key)
        {
            return Ok(Some(hit.value.as_ref().clone()));
        }
        let response = self.http.get(request, self.budget).await?;
        let Some(bytes) = self.status(artifact_type, response, not_found_is_empty)? else {
            return Ok(None);
        };
        let (value, weight) = decode(&bytes).ok_or_else(|| super::util::invalid(artifact_type))?;
        if let Some(cache) = self.cache {
            cache.0.put(key, value.clone(), weight, REGISTRY_ANSWER);
        }
        Ok(Some(value))
    }

    /// A release fact remembered under `resource` (`type:name@version:…`);
    /// published versions never change, so a fact never expires.
    pub(crate) fn fact(&self, resource: &str) -> Option<serde_json::Value> {
        let hit = self.cache?.0.get(&fact_key(resource))?;
        Some(hit.value.as_ref().clone())
    }

    pub(crate) fn remember_fact(&self, resource: &str, fact: &serde_json::Value) {
        let Some(cache) = self.cache else {
            return;
        };
        let size = octocode_engine::json_size::bytes(fact).unwrap_or(usize::MAX);
        cache.0.put(
            fact_key(resource),
            fact.clone(),
            size,
            CacheClass::Immutable,
        );
    }

    /// A release fact that may change (an absent tag, pushed later),
    /// remembered under `resource` for as long as a registry answer lives.
    pub(crate) fn remember_answer(&self, resource: &str, fact: &serde_json::Value) {
        if let Some(cache) = self.cache {
            let size = octocode_engine::json_size::bytes(fact).unwrap_or(usize::MAX);
            cache
                .0
                .put(fact_key(resource), fact.clone(), size, REGISTRY_ANSWER);
        }
    }

    /// A discovery page's start remembered under `resource` (an opaque
    /// next-page token), for as long as a registry answer lives.
    pub(crate) fn cursor(&self, resource: &str) -> Option<String> {
        let hit = self.cache?.0.get(&cursor_key(resource))?;
        hit.value.as_str().map(str::to_owned)
    }

    pub(crate) fn remember_cursor(&self, resource: &str, cursor: &str) {
        if let Some(cache) = self.cache {
            cache.0.put(
                cursor_key(resource),
                serde_json::Value::String(cursor.to_owned()),
                cursor.len(),
                REGISTRY_ANSWER,
            );
        }
    }

    /// An uncached binary download fed to `sink` as it arrives, until the
    /// sink breaks (an archive read for one small entry); `None` when it is
    /// absent (404).
    pub(crate) async fn stream(
        &self,
        artifact_type: ArtifactType,
        url: Url,
        sink: BodySink<'_>,
    ) -> Result<Option<()>, ArtifactError> {
        let request = ArtifactHttpRequest {
            url,
            accept: "application/octet-stream",
            authorization: None,
            dns_pin: None,
        };
        let status = self.http.stream(request, self.budget, sink).await?;
        let response = ArtifactHttpResponse {
            status,
            body: Vec::new(),
        };
        Ok(self.status(artifact_type, response, true)?.map(|_| ()))
    }

    fn status(
        &self,
        artifact_type: ArtifactType,
        response: ArtifactHttpResponse,
        not_found_is_empty: bool,
    ) -> Result<Option<Vec<u8>>, ArtifactError> {
        match response.status {
            200..=299 => Ok(Some(response.body)),
            404 if not_found_is_empty => Ok(None),
            401 | 403 => Err(ArtifactError::new(
                ErrorCode::Authentication,
                format!(
                    "{} denied registry access.",
                    artifact_type::as_str(artifact_type)
                ),
            )
            .with_status(response.status)),
            429 => Err(ArtifactError::new(
                ErrorCode::RateLimited,
                format!(
                    "{} rate limit reached. Retry later.",
                    artifact_type::as_str(artifact_type)
                ),
            )
            .with_status(response.status)),
            // Remaining 4xx (except 408) are deterministic request errors:
            // retrying cannot help, so name the status and the registry's
            // own reason when it gives one.
            status @ 400..=499 if status != 408 => Err(ArtifactError::new(
                ErrorCode::InvalidInput,
                match rejection_detail(&response.body) {
                    Some(detail) => format!(
                        "{} registry rejected the request (HTTP {status}): {detail}",
                        artifact_type::as_str(artifact_type)
                    ),
                    None => format!(
                        "{} registry rejected the request (HTTP {status}).",
                        artifact_type::as_str(artifact_type)
                    ),
                },
            )
            .with_status(status)),
            status => Err(ArtifactError::new(
                ErrorCode::ProviderError,
                format!(
                    "{} registry request failed (HTTP {status}). Retry later.",
                    artifact_type::as_str(artifact_type)
                ),
            )
            .with_status(status)),
        }
    }
}

/// The reason a JSON registry error body gives (`errors[0].detail`,
/// `error`, `message`), on one line and at most 200 chars; registry text is
/// data, quoted as the registry wrote it.
fn rejection_detail(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let detail = value
        .pointer("/errors/0/detail")
        .or_else(|| value.get("error"))
        .or_else(|| value.get("message"))
        .and_then(serde_json::Value::as_str)?;
    let line = detail.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped = line.chars().take(200).collect::<String>();
    (!clipped.is_empty()).then_some(clipped)
}

#[cfg(test)]
mod tests;
