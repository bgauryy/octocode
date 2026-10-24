//! The registry client every provider reads through: cached JSON and text answers, remembered facts and cursors, registry errors.
use super::super::util::invalid;
use super::super::{ArtifactError, ReleaseTags};
use super::{
    ArtifactCache, ArtifactHttp, ArtifactHttpRequest, ArtifactHttpResponse, BodySink, DnsPin,
    REGISTRY_ANSWER, cache_key, cursor_key, fact_key,
};
use crate::cache::{CacheClass, CacheKey};
use crate::providers::artifact::query::artifact_type;
use octocode_contract_types::ArtifactType;
use octocode_github::ProviderErrorKind;
use octocode_io::budget::RequestBudget;
use secrecy::SecretString;
use url::Url;

pub(crate) struct RegistryClient<'a> {
    pub http: &'a dyn ArtifactHttp,
    pub budget: &'a RequestBudget,
    /// The runtime's registry cache; `None` (`storage.mode == "memory"`)
    /// bypasses it for both reads and writes.
    pub cache: Option<&'a ArtifactCache>,
    /// Upstream release-tag checks; `None` reads no tags.
    pub tags: Option<&'a dyn ReleaseTags>,
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
                _ => Err(invalid(artifact_type)),
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
        let Some(bytes) = response_body(artifact_type, response, not_found_is_empty)? else {
            return Ok(None);
        };
        let (value, weight) = decode(&bytes).ok_or_else(|| invalid(artifact_type))?;
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
