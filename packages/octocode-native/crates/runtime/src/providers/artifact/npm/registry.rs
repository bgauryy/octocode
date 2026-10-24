//! The npm registry a lookup may reach: a plain HTTP(S) base matching the
//! query, and (unless private registries are allowed) a public host whose
//! DNS answers are pinned through connect.
use crate::policy::network::is_public;
use crate::providers::artifact::http::DnsPin;
use crate::providers::artifact::query::artifact_search_query;
use crate::providers::artifact::{ArtifactError, ResolvedNpmRegistry};
use octocode_contract_types::{ArtifactSearchQuery, ErrorCode};
use std::net::{SocketAddr, ToSocketAddrs};
use url::{Host, Url};

pub(super) fn validate_registry(
    query: &ArtifactSearchQuery,
    registry: &ResolvedNpmRegistry,
    allow_private_registry: bool,
) -> Result<Option<DnsPin>, ArtifactError> {
    let base = &registry.base;
    if !matches!(base.scheme(), "http" | "https")
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(invalid_registry_url());
    }
    if let Some(requested) = artifact_search_query::registry_url(query) {
        let requested = Url::parse(requested).map_err(|_| invalid_registry_url())?;
        if trim_registry(&requested) != trim_registry(base) {
            return Err(ArtifactError::new(
                ErrorCode::InvalidInput,
                "Resolved npm registry does not match the query registry.",
            ));
        }
    }
    if allow_private_registry {
        return Ok(None);
    }
    validate_registry_target(base)
}

pub(super) fn trim_registry(url: &Url) -> String {
    url.as_str().trim_end_matches('/').to_owned()
}

/// A `registryUrl` that is not a plain HTTP(S) registry base.
pub(crate) fn invalid_registry_url() -> ArtifactError {
    ArtifactError::new(
        ErrorCode::InvalidInput,
        "registryUrl must be an HTTP(S) URL without credentials, query or fragment.",
    )
}

fn registry_target_error() -> ArtifactError {
    ArtifactError::new(
        ErrorCode::InvalidInput,
        "registryUrl names a loopback, link-local or private host; network.allowPrivateRegistry \
         (OCTOCODE_ALLOW_PRIVATE_REGISTRY) permits it.",
    )
}

/// Validate a custom registry target and return the exact public DNS answers
/// that the HTTP transport must use. The official npm registry is the sole
/// hostname allowlist entry; every other domain is resolved once, rejects a
/// mixed public/private answer, and is pinned through connect.
fn validate_registry_target(base: &Url) -> Result<Option<DnsPin>, ArtifactError> {
    match base.host() {
        Some(Host::Ipv4(ip)) if !is_public(ip.into()) => Err(registry_target_error()),
        Some(Host::Ipv6(ip)) if !is_public(ip.into()) => Err(registry_target_error()),
        Some(Host::Ipv4(_) | Host::Ipv6(_)) => Ok(None),
        Some(Host::Domain(name)) => {
            let dns_host = name.to_ascii_lowercase();
            let host = dns_host.trim_end_matches('.');
            if host.is_empty() || octocode_github::names_localhost(host) {
                return Err(registry_target_error());
            }
            if base.scheme() == "https"
                && host == "registry.npmjs.org"
                && base.port_or_known_default() == Some(443)
            {
                return Ok(None);
            }
            let port = base
                .port_or_known_default()
                .ok_or_else(registry_target_error)?;
            let addresses: Vec<SocketAddr> = (dns_host.as_str(), port)
                .to_socket_addrs()
                .map_err(|_| {
                    ArtifactError::new(
                        ErrorCode::ProviderError,
                        "Custom npm registry hostname could not be resolved safely.",
                    )
                })?
                .collect();
            validate_resolved_addresses(&addresses)?;
            Ok(Some(DnsPin {
                host: dns_host,
                addresses,
            }))
        }
        None => Err(registry_target_error()),
    }
}

pub(super) fn validate_resolved_addresses(addresses: &[SocketAddr]) -> Result<(), ArtifactError> {
    if addresses.is_empty() {
        return Err(ArtifactError::new(
            ErrorCode::ProviderError,
            "Custom npm registry hostname resolved without any usable address.",
        ));
    }
    if addresses.iter().any(|address| !is_public(address.ip())) {
        return Err(registry_target_error());
    }
    Ok(())
}

pub(super) fn registry_url(base: &Url, path: &str) -> Result<Url, ArtifactError> {
    Url::parse(&format!("{}/{}", trim_registry(base), path)).map_err(|_| invalid_registry_url())
}

/// A registry at `url` without credentials.
#[cfg(test)]
pub(super) fn test_registry(url: &str) -> ResolvedNpmRegistry {
    ResolvedNpmRegistry {
        base: Url::parse(url).expect("valid url"),
        authorization: None,
        cache_identity: "test".into(),
    }
}

#[cfg(test)]
mod tests;
