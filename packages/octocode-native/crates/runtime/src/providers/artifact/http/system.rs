//! The system registry HTTP client: pooled and DNS-pinned clients, retries and budget-bounded reads.
use super::super::ArtifactError;
use super::{
    ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse, BodySink, DnsPin,
    FRESHNESS_HEADERS, StatusFuture,
};
use crate::providers::RuntimeClients;
use octocode_contract_types::ErrorCode;
use octocode_io::budget::{BudgetStop, RequestBudget};
use octocode_io::http::Proxy;
use octocode_io::retry::{Backoff, BodyError, StatusClass};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue, USER_AGENT};
use secrecy::ExposeSecret;
use std::fmt;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct SystemArtifactHttp {
    /// Retries after a 5xx or 429 (the runtime's `max_retries`).
    retries: u8,
}

impl SystemArtifactHttp {
    /// The registry client for the current Tokio runtime: one connection pool
    /// and TLS configuration per proxy setting for every call on it, built on
    /// a request's first need ([`SystemArtifactHttp::client_for`]).
    #[must_use]
    pub fn shared() -> Self {
        Self { retries: 1 }
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

/// The pooled client honoring `proxy` on the current runtime.
fn pooled_client(proxy: Proxy) -> Result<reqwest::Client, ArtifactError> {
    static CLIENTS: RuntimeClients<Proxy> = RuntimeClients::new(MAX_RUNTIME_CLIENTS);
    CLIENTS.get(proxy, || build_client(proxy, None))
}

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
        build_client(Proxy::Direct, Some(pin))
    })
}

/// A client honoring `proxy`. A pinned name connects directly to the exact
/// public addresses validated by npm policy whatever `proxy` says: a proxy
/// would resolve the target independently.
fn build_client(proxy: Proxy, dns_pin: Option<&DnsPin>) -> Result<reqwest::Client, ArtifactError> {
    let policy = octocode_io::http::ClientPolicy {
        proxy,
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
    /// The client a request connects through: the one pinned to its
    /// validated addresses, else the pooled one for its host: a loopback
    /// registry is reached directly, any other through the user's system
    /// proxy ([`octocode_github::proxy_for`]).
    fn client_for(request: &ArtifactHttpRequest) -> Result<reqwest::Client, ArtifactError> {
        match request.dns_pin.as_ref() {
            Some(pin) => pinned_client(pin),
            None => pooled_client(octocode_github::proxy_for(&request.url)),
        }
    }

    /// The answer to `request` once its headers arrive, and the attempt
    /// (from `first`) that got it: a failed connection, a 5xx or a 429 is
    /// retried within the retries and the request's time; a redirect is
    /// refused.
    async fn send(
        &self,
        request: &ArtifactHttpRequest,
        post: Option<&[u8]>,
        budget: &RequestBudget,
        first: u8,
    ) -> Result<(reqwest::Response, u8, Instant), ArtifactError> {
        for attempt in first..=self.retries {
            budget.check().map_err(budget_error)?;
            // After the budget check: a request with no time left builds no
            // client.
            let client = Self::client_for(request)?;
            let builder = match post {
                Some(body) => client
                    .post(request.url.clone())
                    .header(CONTENT_TYPE, super::JSON)
                    .body(body.to_vec()),
                None => client.get(request.url.clone()),
            };
            let mut builder = builder
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
            if let Some((name, value)) = request.validator.as_ref() {
                builder = builder.header(name, value);
            }
            let sent = Instant::now();
            let response = match budget
                .wait_stalled(builder.send())
                .await
                .map_err(budget_error)?
            {
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
            if class.is_retryable()
                && attempt < self.retries
                && let Some(delay) = retry_delay(Some(response.headers()), attempt, budget)
            {
                drain(response, budget).await?;
                pause(delay, budget).await?;
                continue;
            }
            // A 304 answers the validator this request asked with.
            let confirmed =
                status == reqwest::StatusCode::NOT_MODIFIED && request.validator.is_some();
            if class == StatusClass::Redirect && !confirmed {
                return Err(ArtifactError::new(
                    ErrorCode::ProviderError,
                    "Artifact registry redirects are not followed.",
                )
                .with_status(status.as_u16()));
            }
            return Ok((response, attempt, sent));
        }
        Err(ArtifactError::new(
            ErrorCode::ProviderError,
            "Artifact registry request failed. Retry later.",
        ))
    }
}

impl SystemArtifactHttp {
    /// The whole answer to one read, GET or read-only POST.
    async fn fetch(
        &self,
        request: ArtifactHttpRequest,
        post: Option<Vec<u8>>,
        budget: &RequestBudget,
    ) -> Result<ArtifactHttpResponse, ArtifactError> {
        let mut first = 0;
        loop {
            let (response, attempt, sent) =
                self.send(&request, post.as_deref(), budget, first).await?;
            let status = response.status().as_u16();
            let mut received_headers = response.headers().clone();
            match octocode_io::retry::read_bounded(response, budget, budget.max_body_bytes).await {
                Ok(body) => {
                    octocode_io::http::account_response_delay(
                        &mut received_headers,
                        sent.elapsed(),
                    );
                    let headers = FRESHNESS_HEADERS
                        .into_iter()
                        .flat_map(|name| {
                            received_headers
                                .get_all(name)
                                .iter()
                                .filter_map(move |value| {
                                    Some((name.to_owned(), value.to_str().ok()?.to_owned()))
                                })
                        })
                        .collect();
                    return Ok(ArtifactHttpResponse {
                        status,
                        body: body.into(),
                        headers,
                    });
                }
                Err(BodyError::Stopped(stop)) => return Err(budget_error(stop)),
                Err(BodyError::TooLarge) => return Err(body_too_large()),
                Err(BodyError::Decode) => return Err(body_undecodable()),
                // Every read here is idempotent: a body that breaks off is
                // retried like a failed connection.
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
    }
}

impl ArtifactHttp for SystemArtifactHttp {
    fn get<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        Box::pin(self.fetch(request, None, budget))
    }

    fn post<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        body: Vec<u8>,
        budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        Box::pin(self.fetch(request, Some(body), budget))
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
            let (mut response, _, _) = self.send(&request, None, budget, 0).await?;
            let status = response.status().as_u16();
            if !(200..300).contains(&status) {
                drain(response, budget).await?;
                return Ok(status);
            }
            let mut read = 0usize;
            while let Some(chunk) = budget
                .wait_stalled(response.chunk())
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

fn body_undecodable() -> ArtifactError {
    ArtifactError::new(
        ErrorCode::ProviderError,
        "Artifact registry response body does not decode as its Content-Encoding says.",
    )
}

/// The most of an error body read before a retry. Reading a short body to
/// its end hands the connection back to the pool; a longer one is dropped
/// with its connection.
pub(super) const MAX_DRAIN_BYTES: usize = 64 * 1024;

/// Read and discard a retried response's body, up to [`MAX_DRAIN_BYTES`].
async fn drain(response: reqwest::Response, budget: &RequestBudget) -> Result<(), ArtifactError> {
    match octocode_io::retry::read_bounded(response, budget, MAX_DRAIN_BYTES).await {
        Err(BodyError::Stopped(stop)) => Err(budget_error(stop)),
        Ok(_) | Err(BodyError::TooLarge | BodyError::Transport | BodyError::Decode) => Ok(()),
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

#[cfg(test)]
mod tests;
