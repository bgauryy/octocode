use super::http::{DnsPin, RegistryClient};
use super::util::{encode_component, endpoint, object_for, required, safe_url, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType, ResolvedNpmRegistry,
};
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use url::{Host, Url};

pub(crate) async fn npm(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    allow_private_registry: bool,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let dns_pin = validate_registry(query, registry, allow_private_registry)?;
    if let Some(name) = query.package_name() {
        exact(name, registry, client, dns_pin).await
    } else {
        search(query, state, registry, client, dns_pin).await
    }
}

fn validate_registry(
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
        return Err(ArtifactError::new(
            "invalid_query",
            "Invalid npm registry URL: use HTTP(S) without credentials, query or fragment.",
        ));
    }
    if let Some(requested) = query.registry() {
        let requested = Url::parse(requested).map_err(|_| {
            ArtifactError::new(
                "invalid_query",
                "Invalid npm registry URL: use HTTP(S) without credentials, query or fragment.",
            )
        })?;
        if trim_registry(&requested) != trim_registry(base) {
            return Err(ArtifactError::new(
                "invalid_query",
                "Resolved npm registry does not match the query registry.",
            ));
        }
    }
    if allow_private_registry {
        return Ok(None);
    }
    validate_registry_target(base)
}

fn trim_registry(url: &Url) -> String {
    url.as_str().trim_end_matches('/').to_owned()
}

fn registry_target_error() -> ArtifactError {
    ArtifactError::new(
        "invalid_query",
        "Invalid npm registry URL: loopback, link-local, and private hosts are not allowed \
         (set network.allowPrivateRegistry / OCTOCODE_ALLOW_PRIVATE_REGISTRY to permit).",
    )
}

/// Validate a custom registry target and return the exact public DNS answers
/// that the HTTP transport must use. The official npm registry is the sole
/// hostname allowlist entry; every other domain is resolved once, rejects a
/// mixed public/private answer, and is pinned through connect.
fn validate_registry_target(base: &Url) -> Result<Option<DnsPin>, ArtifactError> {
    match base.host() {
        Some(Host::Ipv4(ip)) if is_blocked_v4(&ip) => Err(registry_target_error()),
        Some(Host::Ipv6(ip)) if is_blocked_v6(&ip) => Err(registry_target_error()),
        Some(Host::Ipv4(_) | Host::Ipv6(_)) => Ok(None),
        Some(Host::Domain(name)) => {
            let dns_host = name.to_ascii_lowercase();
            let host = dns_host.trim_end_matches('.');
            if host.is_empty() || host == "localhost" || host.ends_with(".localhost") {
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
                        "provider_error",
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

fn validate_resolved_addresses(addresses: &[SocketAddr]) -> Result<(), ArtifactError> {
    if addresses.is_empty() {
        return Err(ArtifactError::new(
            "provider_error",
            "Custom npm registry hostname resolved without any usable address.",
        ));
    }
    if addresses.iter().any(|address| is_blocked_ip(&address.ip())) {
        return Err(registry_target_error());
    }
    Ok(())
}

fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => is_blocked_v6(v6),
    }
}

fn is_blocked_v4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        // CGNAT / shared address space 100.64.0.0/10
        || (octets[0] == 100 && (octets[1] & 0xc0) == 64)
}

fn is_blocked_v6(ip: &Ipv6Addr) -> bool {
    let segments = ip.segments();
    ip.is_loopback()
        || ip.is_unspecified()
        // link-local fe80::/10
        || (segments[0] & 0xffc0) == 0xfe80
        // unique local (ULA) fc00::/7
        || (segments[0] & 0xfe00) == 0xfc00
        // IPv4-mapped/compatible addresses embedding a blocked v4
        || matches!(ip.to_ipv4_mapped(), Some(v4) if is_blocked_v4(&v4))
        || matches!(ip.to_ipv4(), Some(v4) if is_blocked_v4(&v4))
}

pub(crate) fn split_npm_coordinate(package_name: &str) -> (&str, Option<&str>) {
    if let Some(rest) = package_name.strip_prefix('@') {
        if let Some((name, version)) = rest.rsplit_once('@')
            && !version.is_empty()
            && name.contains('/')
        {
            let name_len = package_name.len() - version.len() - 1;
            return (&package_name[..name_len], Some(version));
        }
        return (package_name, None);
    }
    if let Some((name, version)) = package_name.rsplit_once('@')
        && !name.is_empty()
        && !version.is_empty()
        && !name.contains('/')
    {
        return (name, Some(version));
    }
    (package_name, None)
}

async fn exact(
    package_name: &str,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let (name, version) = split_npm_coordinate(package_name);
    let encoded = if let Some(scoped) = name.strip_prefix('@') {
        format!("@{}", encode_component(scoped))
    } else {
        encode_component(name)
    };
    let spec = version.unwrap_or("latest");
    // Percent-encode the version/spec as its own path segment so ranges or
    // tags (e.g. "^1.0.0") cannot alter the request path.
    let request_url = registry_url(
        &registry.base,
        &format!("{encoded}/{}", encode_component(spec)),
    )?;
    let response = client
        .json_with_dns_pin(
            ArtifactType::Npm,
            request_url,
            true,
            registry.authorization.clone(),
            dns_pin,
        )
        .await?;
    let Some(response) = response else {
        let mut page = ArtifactProviderPage::empty(Some(0));
        page.registry = Some(trim_registry(&registry.base));
        return Ok(page);
    };
    let row = object_for(&response, ArtifactType::Npm)?;
    let name = required(row.get("name"), ArtifactType::Npm)?;
    if name != package_name && name != split_npm_coordinate(package_name).0 {
        return Err(ArtifactError::new(
            "provider_error",
            "npm registry returned a different package name.",
        ));
    }
    let version = required(row.get("version"), ArtifactType::Npm)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Npm,
        name.clone(),
        format!(
            "{}/{}",
            trim_registry(&registry.base),
            encode_component(&name)
        ),
    );
    artifact.version = Some(version);
    artifact.description = string(row.get("description"));
    artifact.license = match row.get("license") {
        Some(Value::Object(value)) => string(value.get("type")),
        value => string(value),
    };
    artifact.homepage = string(row.get("homepage"));
    match row.get("repository") {
        Some(Value::String(value)) => artifact.repository = normalize_repository(value),
        Some(Value::Object(value)) => {
            artifact.repository = value
                .get("url")
                .and_then(Value::as_str)
                .and_then(normalize_repository);
            artifact.repository_directory = string(value.get("directory")).map(|value| {
                value
                    .trim_start_matches("./")
                    .trim_start_matches('/')
                    .to_owned()
            });
        }
        _ => {}
    }
    Ok(ArtifactProviderPage {
        artifacts: vec![artifact],
        next_state: None,
        total: Some(1),
        terminal_limit: None,
        registry: Some(trim_registry(&registry.base)),
    })
}

async fn search(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let offset = state.offset.unwrap_or(0);
    let size = query.page_size().unwrap_or(10);
    let terms = query.terms();
    let url = endpoint(
        &format!("{}/-/v1/search", trim_registry(&registry.base)),
        &[
            ("text", Some(terms)),
            ("size", Some(size.to_string())),
            ("from", (offset > 0).then(|| offset.to_string())),
        ],
    )?;
    let response = client
        .json_with_dns_pin(
            ArtifactType::Npm,
            url,
            false,
            registry.authorization.clone(),
            dns_pin,
        )
        .await?
        .ok_or_else(|| ArtifactError::new("provider_error", "npm registry search failed."))?;
    let data = object_for(&response, ArtifactType::Npm)?;
    let total_found = total(data.get("total")).ok_or_else(|| {
        ArtifactError::new(
            "provider_error",
            "npm registry search omitted a valid total; pagination cannot be determined.",
        )
    })?;
    let objects = data
        .get("objects")
        .and_then(Value::as_array)
        .ok_or_else(|| ArtifactError::new("provider_error", "Invalid npm registry search response; expected an object with results and a total."))?;
    let mut artifacts = Vec::with_capacity(objects.len().min(size));
    for entry in objects.iter().take(size) {
        let package = object_for(
            object_for(entry, ArtifactType::Npm)?
                .get("package")
                .ok_or_else(|| super::util::invalid(ArtifactType::Npm))?,
            ArtifactType::Npm,
        )?;
        let name = required(package.get("name"), ArtifactType::Npm).map_err(|_| {
            ArtifactError::new(
                "provider_error",
                "npm registry search returned an unnamed package; refusing to skip a result.",
            )
        })?;
        let mut artifact = ArtifactItem::new(
            ArtifactType::Npm,
            name.clone(),
            format!(
                "{}/{}",
                trim_registry(&registry.base),
                encode_component(&name)
            ),
        );
        artifact.version = string(package.get("version")).filter(|value| value != "unknown");
        artifact.description = string(package.get("description"));
        artifact.license = string(package.get("license"));
        if let Some(Value::Object(links)) = package.get("links") {
            artifact.homepage = safe_url(links.get("homepage"));
            artifact.repository = links
                .get("repository")
                .and_then(Value::as_str)
                .and_then(normalize_repository);
        }
        artifacts.push(artifact);
    }
    let has_more = offset.saturating_add(artifacts.len() as u64) < total_found;
    let terminal_limit = (has_more && artifacts.is_empty())
        .then(|| "npm returned an empty page before its reported total.".to_owned());
    Ok(ArtifactProviderPage {
        next_state: (has_more && !artifacts.is_empty()).then(|| ArtifactProviderState {
            offset: Some(offset + artifacts.len() as u64),
            ..Default::default()
        }),
        artifacts,
        total: Some(total_found),
        terminal_limit,
        registry: Some(trim_registry(&registry.base)),
    })
}

fn registry_url(base: &Url, path: &str) -> Result<Url, ArtifactError> {
    Url::parse(&format!("{}/{}", trim_registry(base), path))
        .map_err(|_| ArtifactError::new("invalid_query", "Invalid npm registry URL."))
}

pub(crate) fn normalize_repository(value: &str) -> Option<String> {
    let mut value = value
        .trim()
        .strip_prefix("git+")
        .unwrap_or(value.trim())
        .to_owned();
    if value.is_empty() {
        return None;
    }
    for (prefix, host) in [
        ("github:", "github.com"),
        ("gitlab:", "gitlab.com"),
        ("bitbucket:", "bitbucket.org"),
    ] {
        if value.to_ascii_lowercase().starts_with(prefix) {
            value = format!("https://{host}/{}", &value[prefix.len()..]);
            return Some(value.trim_end_matches(".git").to_owned());
        }
    }
    if let Some((_, rest)) = value.split_once('@')
        && !value.contains("://")
        && let Some((host, path)) = rest.split_once(':')
    {
        return Some(format!("https://{host}/{}", path.trim_end_matches(".git")));
    }
    if let Some(rest) = value
        .strip_prefix("ssh://")
        .or_else(|| value.strip_prefix("git://"))
        .or_else(|| value.strip_prefix("http://"))
        .or_else(|| value.strip_prefix("https://"))
    {
        let rest = rest.split_once('@').map(|(_, tail)| tail).unwrap_or(rest);
        return Some(format!("https://{}", rest.trim_end_matches(".git")));
    }
    Some(value.trim_end_matches(".git").to_owned())
}

#[cfg(test)]
mod tests {
    use super::super::types::artifact_query;
    use super::normalize_repository;
    use super::{
        ArtifactSearchQuery, ArtifactType, ResolvedNpmRegistry, is_blocked_v4, is_blocked_v6,
        validate_registry, validate_resolved_addresses,
    };
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
    use url::Url;

    fn npm_registry(url: &str) -> ResolvedNpmRegistry {
        ResolvedNpmRegistry {
            base: Url::parse(url).expect("valid url"),
            authorization: None,
            cache_identity: "test".into(),
        }
    }

    fn npm_query(registry: Option<&str>) -> ArtifactSearchQuery {
        artifact_query(
            serde_json::json!({"type": ArtifactType::Npm, "packageName": "left-pad".to_string(), "registry": registry.map(str::to_owned)}),
            None,
        )
    }

    #[test]
    fn rejects_link_local_metadata_registry() {
        let registry = npm_registry("http://169.254.169.254");
        let query = npm_query(Some("http://169.254.169.254"));
        let err =
            validate_registry(&query, &registry, false).expect_err("must reject link-local host");
        // The opt-in escape hatch permits the same host.
        assert!(validate_registry(&query, &registry, true).is_ok());
        assert_eq!(err.code, "invalid_query");
    }

    #[test]
    fn blocks_private_and_loopback_addresses() {
        assert!(is_blocked_v4(&Ipv4Addr::new(127, 0, 0, 1)));
        assert!(is_blocked_v4(&Ipv4Addr::new(10, 0, 0, 5)));
        assert!(is_blocked_v4(&Ipv4Addr::new(192, 168, 1, 1)));
        assert!(is_blocked_v4(&Ipv4Addr::new(169, 254, 169, 254)));
        assert!(is_blocked_v4(&Ipv4Addr::new(100, 64, 0, 1)));
        assert!(!is_blocked_v4(&Ipv4Addr::new(104, 16, 0, 1)));
        assert!(is_blocked_v6(&Ipv6Addr::LOCALHOST));
        assert!(is_blocked_v6(&Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)));
        assert!(is_blocked_v6(&Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 1)));
    }

    #[test]
    fn rejects_mixed_public_private_dns_answers() {
        let addresses = [
            SocketAddr::from(([104, 16, 0, 1], 443)),
            SocketAddr::from(([127, 0, 0, 1], 443)),
        ];
        let error = validate_resolved_addresses(&addresses)
            .expect_err("one blocked answer must reject the complete DNS result");
        assert_eq!(error.code, "invalid_query");
        assert!(validate_resolved_addresses(&[addresses[0]]).is_ok());
    }

    #[test]
    fn repository_shapes_are_canonical() {
        for (input, expected) in [
            (
                "github:octokit/rest.js",
                "https://github.com/octokit/rest.js",
            ),
            ("git+https://github.com/a/b.git", "https://github.com/a/b"),
            ("git@github.com:a/b.git", "https://github.com/a/b"),
        ] {
            assert_eq!(normalize_repository(input).as_deref(), Some(expected));
        }
    }

    #[test]
    fn splits_versioned_and_scoped_coordinates() {
        assert_eq!(
            super::split_npm_coordinate("leftpad@1.2.3"),
            ("leftpad", Some("1.2.3"))
        );
        assert_eq!(
            super::split_npm_coordinate("@scope/pkg@9.0.0"),
            ("@scope/pkg", Some("9.0.0"))
        );
        assert_eq!(
            super::split_npm_coordinate("@scope/pkg"),
            ("@scope/pkg", None)
        );
    }
}
