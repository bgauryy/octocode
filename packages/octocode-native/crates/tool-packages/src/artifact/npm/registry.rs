//! The npm registry a lookup may reach: a plain HTTP(S) base matching the
//! query, and (unless private registries are allowed) a public host whose
//! DNS answers are pinned through connect. A target that does not resolve
//! gets the same refusal as a private one, naming only the host as given.
use crate::artifact::http::DnsPin;
use crate::artifact::query::artifact_search_query;
use crate::artifact::util::encode_component;
use crate::artifact::{ArtifactError, ResolvedNpmRegistry};
use octocode_contract_types::{ArtifactSearchQuery, ErrorCode};
use octocode_runtime_base::policy::network::{is_public, not_public_refusal};
use std::net::{SocketAddr, ToSocketAddrs};
use url::{Host, Url};

pub(super) fn validate_registry(
    query: &ArtifactSearchQuery,
    registry: &ResolvedNpmRegistry,
    allow_private_registry: bool,
) -> Result<Option<DnsPin>, ArtifactError> {
    validate_base(query, registry)?;
    if allow_private_registry {
        return Ok(None);
    }
    validate_registry_target(&registry.base)
}

/// Validate the configured base locally, before any DNS or HTTP lookup.
pub(crate) fn validate_base(
    query: &ArtifactSearchQuery,
    registry: &ResolvedNpmRegistry,
) -> Result<(), ArtifactError> {
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
    Ok(())
}

pub(super) fn trim_registry(url: &Url) -> String {
    url.as_str().trim_end_matches('/').to_owned()
}

/// The page of package `name` for people to open: npmjs.com's page for the
/// public registry, else the package's address on the configured registry,
/// which has no public page.
pub(super) fn package_page(registry: &ResolvedNpmRegistry, name: &str) -> String {
    if registry.base.origin() == ResolvedNpmRegistry::npmjs().base.origin() {
        format!("https://www.npmjs.com/package/{name}")
    } else {
        format!(
            "{}/{}",
            trim_registry(&registry.base),
            encode_component(name)
        )
    }
}

/// A `registryUrl` that is not a plain HTTP(S) registry base.
pub fn invalid_registry_url() -> ArtifactError {
    ArtifactError::new(
        ErrorCode::InvalidInput,
        "registryUrl must be an HTTP(S) URL without credentials, query or fragment.",
    )
}

/// The one refusal for a registry host that is internal, does not resolve,
/// or resolves to any address that is not public: a security refusal, as
/// the same policy's refusal of a page URL is, which no other input fixes;
/// only the user's setting the message names permits the host.
fn registry_target_error(host: &str) -> ArtifactError {
    ArtifactError::new(
        ErrorCode::SecurityValidationFailed,
        format!(
            "registryUrl host {} network.allowPrivateRegistry \
             (OCTOCODE_ALLOW_PRIVATE_REGISTRY) permits a private registry.",
            not_public_refusal(host)
        ),
    )
}

/// Validate a custom registry target and return the exact public DNS answers
/// that the HTTP transport must use. The official npm registry is the sole
/// hostname allowlist entry; every other domain is resolved once, rejects a
/// mixed public/private answer, and is pinned through connect.
fn validate_registry_target(base: &Url) -> Result<Option<DnsPin>, ArtifactError> {
    let refused = || registry_target_error(base.host_str().unwrap_or_default());
    match base.host() {
        Some(Host::Ipv4(ip)) if !is_public(ip.into()) => Err(refused()),
        Some(Host::Ipv6(ip)) if !is_public(ip.into()) => Err(refused()),
        Some(Host::Ipv4(_) | Host::Ipv6(_)) => Ok(None),
        Some(Host::Domain(name)) => {
            let dns_host = name.to_ascii_lowercase();
            let host = dns_host.trim_end_matches('.');
            if host.is_empty() || octocode_github::names_localhost(host) {
                return Err(refused());
            }
            if base.scheme() == "https"
                && host == "registry.npmjs.org"
                && base.port_or_known_default() == Some(443)
            {
                return Ok(None);
            }
            let port = base.port_or_known_default().ok_or_else(refused)?;
            let addresses: Vec<SocketAddr> = (dns_host.as_str(), port)
                .to_socket_addrs()
                .map_err(|_| refused())?
                .collect();
            validate_resolved_addresses(name, &addresses)?;
            Ok(Some(DnsPin {
                host: dns_host,
                addresses,
            }))
        }
        None => Err(refused()),
    }
}

/// `host`'s DNS answer is usable when it has an address and every one is
/// public.
pub(super) fn validate_resolved_addresses(
    host: &str,
    addresses: &[SocketAddr],
) -> Result<(), ArtifactError> {
    if addresses.is_empty() || addresses.iter().any(|address| !is_public(address.ip())) {
        return Err(registry_target_error(host));
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
    }
}

#[cfg(test)]
mod tests;
