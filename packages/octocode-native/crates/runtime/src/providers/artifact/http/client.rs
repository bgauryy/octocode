//! Registry reads decode directly into the representation the caller needs.
use super::super::util::invalid;
use super::super::{ArtifactError, ReleaseTags};
use super::{ArtifactHttp, ArtifactHttpRequest, ArtifactHttpResponse, BodySink, DnsPin};
use crate::providers::artifact::query::artifact_type;
use octocode_contract_types::ArtifactType;
use octocode_github::ProviderErrorKind;
use octocode_io::budget::RequestBudget;
use secrecy::SecretString;
use url::Url;

pub(crate) struct RegistryClient<'a> {
    pub http: &'a dyn ArtifactHttp,
    pub budget: &'a RequestBudget,
    pub tags: Option<&'a dyn ReleaseTags>,
}

#[cfg(test)]
impl<'a> RegistryClient<'a> {
    pub(crate) fn new(http: &'a dyn ArtifactHttp, budget: &'a RequestBudget) -> Self {
        Self {
            http,
            budget,
            tags: None,
        }
    }
}

impl RegistryClient<'_> {
    /// Decode a current JSON answer. DNS pins and credentials belong to this
    /// request, and no answer survives its execution scope.
    pub async fn json(
        &self,
        artifact_type: ArtifactType,
        url: Url,
        not_found_is_empty: bool,
        authorization: Option<SecretString>,
        dns_pin: Option<DnsPin>,
        accept: &'static str,
    ) -> Result<Option<serde_json::Value>, ArtifactError> {
        let request = ArtifactHttpRequest {
            url,
            accept,
            authorization,
            dns_pin,
        };
        self.read(artifact_type, request, not_found_is_empty, |bytes| {
            serde_json::from_slice(bytes).ok()
        })
        .await
    }

    /// Decode only the selected representation, without constructing a full
    /// packument when the caller needs its version list.
    pub(crate) async fn json_view(
        &self,
        artifact_type: ArtifactType,
        url: Url,
        not_found_is_empty: bool,
        authorization: Option<SecretString>,
        dns_pin: Option<DnsPin>,
        accept: &'static str,
        view: fn(&[u8]) -> Option<serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, ArtifactError> {
        let request = ArtifactHttpRequest {
            url,
            accept,
            authorization,
            dns_pin,
        };
        self.read(artifact_type, request, not_found_is_empty, view)
            .await
    }

    pub async fn text(
        &self,
        artifact_type: ArtifactType,
        url: Url,
        not_found_is_empty: bool,
    ) -> Result<Option<String>, ArtifactError> {
        let request = ArtifactHttpRequest {
            url,
            accept: "application/xml",
            authorization: None,
            dns_pin: None,
        };
        self.read(artifact_type, request, not_found_is_empty, |bytes| {
            std::str::from_utf8(bytes).ok().map(str::to_owned)
        })
        .await
    }

    async fn read<T>(
        &self,
        artifact_type: ArtifactType,
        request: ArtifactHttpRequest,
        not_found_is_empty: bool,
        decode: impl FnOnce(&[u8]) -> Option<T>,
    ) -> Result<Option<T>, ArtifactError> {
        let response = self.http.get(request, self.budget).await?;
        let Some(bytes) = response_body(artifact_type, response, not_found_is_empty)? else {
            return Ok(None);
        };
        decode(&bytes)
            .map(Some)
            .ok_or_else(|| invalid(artifact_type))
    }

    /// Stream an archive until the selected entry is reached or the body ends.
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
        Ok(response_body(artifact_type, response, true)?.map(|_| ()))
    }
}

/// A registry response's body: `None` for a 404 when `not_found_is_empty`,
/// an error for any other non-2xx status.
pub(super) fn response_body(
    artifact_type: ArtifactType,
    response: ArtifactHttpResponse,
    not_found_is_empty: bool,
) -> Result<Option<Vec<u8>>, ArtifactError> {
    match response.status {
        200..=299 => Ok(Some(response.body)),
        404 if not_found_is_empty => Ok(None),
        status => Err(registry_error(artifact_type, status, &response.body)),
    }
}

/// A registry's non-2xx answer under the status table GitHub answers share
/// ([`ProviderErrorKind::from_status`]): the same status reports the same
/// `errorCode` from every provider. The message names the registry, the
/// status, and the registry's own reason when it gives one.
fn registry_error(artifact_type: ArtifactType, status: u16, body: &[u8]) -> ArtifactError {
    let kind = ProviderErrorKind::from_status(status);
    let registry = artifact_type::as_str(artifact_type);
    let (summary, advice) = match kind {
        ProviderErrorKind::Authentication | ProviderErrorKind::Permission => (
            format!("{registry} denied registry access (HTTP {status})"),
            "",
        ),
        ProviderErrorKind::RateLimited => (
            format!("{registry} rate limit reached (HTTP {status})"),
            " Retry later.",
        ),
        ProviderErrorKind::NotFound => (
            format!("{registry} registry has no such resource (HTTP {status})"),
            "",
        ),
        ProviderErrorKind::Unavailable => (
            format!("{registry} registry blocked the resource for legal reasons (HTTP {status})"),
            "",
        ),
        ProviderErrorKind::Server => (
            format!("{registry} registry request failed (HTTP {status})"),
            " Retry later.",
        ),
        _ => (
            format!("{registry} registry rejected the request (HTTP {status})"),
            "",
        ),
    };
    let message = match rejection_detail(body) {
        Some(detail) => format!("{summary}: {detail}.{advice}"),
        None => format!("{summary}.{advice}"),
    };
    ArtifactError::new(kind.code(), message).with_status(status)
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
