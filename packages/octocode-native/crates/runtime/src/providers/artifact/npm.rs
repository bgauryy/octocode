use super::http::RegistryClient;
use super::util::{encode_component, endpoint, object_for, required, safe_url, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactQuery,
    ArtifactType, ResolvedNpmRegistry,
};
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use url::{Host, Url};

pub(crate) async fn npm(
    query: &ArtifactQuery,
    state: &ArtifactProviderState,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    validate_registry(query, registry)?;
    if let Some(name) = query.package_name.as_deref() {
        exact(name, registry, client).await
    } else {
        search(query, state, registry, client).await
    }
}

fn validate_registry(
    query: &ArtifactQuery,
    registry: &ResolvedNpmRegistry,
) -> Result<(), ArtifactError> {
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
    // SSRF guard: refuse to fetch from loopback, link-local, or private
    // (RFC-1918/ULA/CGNAT) hosts. The `base` is what actually gets fetched and
    // reflects the caller-supplied `registry` (a mismatch is rejected below),
    // so blocking it here covers the request target. Public registries such as
    // registry.npmjs.org resolve to public addresses and are unaffected.
    if is_blocked_host(base.host()) {
        return Err(ArtifactError::new(
            "invalid_query",
            "Invalid npm registry URL: loopback, link-local, and private hosts are not allowed.",
        ));
    }
    if let Some(requested) = query.registry.as_deref() {
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
    Ok(())
}

fn trim_registry(url: &Url) -> String {
    url.as_str().trim_end_matches('/').to_owned()
}

/// True when the registry host targets a private/loopback/link-local address
/// and must not be fetched (SSRF protection). IP literals are checked directly;
/// domains are checked for localhost and, best-effort, resolved so a name that
/// points at a private address is also blocked. Unresolvable names are allowed
/// (the actual request will fail on its own).
fn is_blocked_host(host: Option<Host<&str>>) -> bool {
    match host {
        Some(Host::Ipv4(ip)) => is_blocked_v4(&ip),
        Some(Host::Ipv6(ip)) => is_blocked_v6(&ip),
        Some(Host::Domain(name)) => is_blocked_domain(name),
        None => true,
    }
}

fn is_blocked_domain(name: &str) -> bool {
    let host = name.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    match (host.as_str(), 443u16).to_socket_addrs() {
        Ok(addrs) => {
            let mut resolved = addrs.peekable();
            // Only block on a positive resolution to a private address; an
            // empty or failed lookup falls through to the real request.
            resolved.peek().is_some() && resolved.all(|addr| is_blocked_ip(&addr.ip()))
        }
        Err(_) => false,
    }
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
        .json(
            ArtifactType::Npm,
            request_url,
            true,
            registry.authorization.clone(),
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
    query: &ArtifactQuery,
    state: &ArtifactProviderState,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let offset = state.offset.unwrap_or(0);
    let size = query.page_size.unwrap_or(10);
    let terms = query
        .keywords
        .as_ref()
        .map(|v| v.join(" "))
        .unwrap_or_default();
    let url = endpoint(
        &format!("{}/-/v1/search", trim_registry(&registry.base)),
        &[
            ("text", Some(terms)),
            ("size", Some(size.to_string())),
            ("from", (offset > 0).then(|| offset.to_string())),
        ],
    )?;
    let response = client
        .json(
            ArtifactType::Npm,
            url,
            false,
            registry.authorization.clone(),
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
    use super::normalize_repository;
    use super::{
        ArtifactQuery, ArtifactType, ResolvedNpmRegistry, is_blocked_v4, is_blocked_v6,
        validate_registry,
    };
    use std::net::{Ipv4Addr, Ipv6Addr};
    use url::Url;

    fn npm_registry(url: &str) -> ResolvedNpmRegistry {
        ResolvedNpmRegistry {
            base: Url::parse(url).expect("valid url"),
            authorization: None,
            cache_identity: "test".into(),
        }
    }

    fn npm_query(registry: Option<&str>) -> ArtifactQuery {
        ArtifactQuery {
            artifact_type: ArtifactType::Npm,
            package_name: Some("left-pad".into()),
            keywords: None,
            page_size: None,
            cursor: None,
            registry: registry.map(str::to_owned),
        }
    }

    #[test]
    fn rejects_link_local_metadata_registry() {
        let registry = npm_registry("http://169.254.169.254");
        let query = npm_query(Some("http://169.254.169.254"));
        let err = validate_registry(&query, &registry).expect_err("must reject link-local host");
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
