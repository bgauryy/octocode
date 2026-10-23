//! The system registry HTTP client: pooled and DNS-pinned clients, retries and budget-bounded reads.
use super::super::ArtifactError;
use super::{
    ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse, BodySink, DnsPin,
    StatusFuture,
};
use crate::providers::RuntimeClients;
use octocode_contract_types::ErrorCode;
use octocode_io::budget::{BudgetStop, RequestBudget};
use octocode_io::retry::{Backoff, BodyError, StatusClass};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue, USER_AGENT};
use secrecy::ExposeSecret;
use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

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
            if class.is_retryable()
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
pub(super) const MAX_DRAIN_BYTES: usize = 64 * 1024;

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
