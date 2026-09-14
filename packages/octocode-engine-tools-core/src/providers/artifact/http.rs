use super::{ArtifactError, ArtifactType};
use crate::providers::RequestBudget;
use bytes::BytesMut;
use futures_util::StreamExt;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue, USER_AGENT};
use secrecy::{ExposeSecret, SecretString};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};
use url::Url;

pub type ArtifactHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ArtifactHttpResponse, ArtifactError>> + Send + 'a>>;

#[derive(Clone)]
pub struct ArtifactHttpRequest {
    pub url: Url,
    pub accept: &'static str,
    pub authorization: Option<SecretString>,
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
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| {
                ArtifactError::new(
                    "provider_error",
                    "Failed to initialize artifact registry HTTP client.",
                )
            })?;
        Ok(Self { client })
    }
}

impl ArtifactHttp for SystemArtifactHttp {
    fn get<'a>(
        &'a self,
        request: ArtifactHttpRequest,
        budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        Box::pin(async move {
            for attempt in 0..2 {
                check_budget(budget)?;
                let mut builder = self
                    .client
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
}

impl RegistryClient<'_> {
    pub async fn json(
        &self,
        artifact_type: ArtifactType,
        url: Url,
        not_found_is_empty: bool,
        authorization: Option<SecretString>,
    ) -> Result<Option<serde_json::Value>, ArtifactError> {
        let response = self
            .http
            .get(
                ArtifactHttpRequest {
                    url,
                    accept: "application/json",
                    authorization,
                },
                self.budget,
            )
            .await?;
        let body = self.status(artifact_type, response, not_found_is_empty)?;
        body.map(|bytes| {
            serde_json::from_slice(&bytes).map_err(|_| invalid_response(artifact_type))
        })
        .transpose()
    }

    pub async fn text(
        &self,
        artifact_type: ArtifactType,
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
        artifact_type: ArtifactType,
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
            status => Err(ArtifactError::new(
                "provider_error",
                format!(
                    "{} registry request failed. Retry later.",
                    artifact_type.as_str()
                ),
            )
            .with_status(status)),
        }
    }
}

pub(crate) fn invalid_response(artifact_type: ArtifactType) -> ArtifactError {
    ArtifactError::new(
        "provider_error",
        format!(
            "{} returned an invalid registry response.",
            artifact_type.as_str()
        ),
    )
}
