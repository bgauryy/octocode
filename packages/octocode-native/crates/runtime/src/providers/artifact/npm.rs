use super::http::{DnsPin, NPM_INSTALL_JSON, RegistryClient};
use super::util::{
    commit_sha, date_from_millis, encode_component, endpoint, object_for, required, safe_url,
    string, total,
};
use super::versions::{VersionSpec, nearest, npm_resolve};
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
    if let Some(name) = query.bare_package_name() {
        exact(name, query.version(), registry, client, dns_pin).await
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
    name: &str,
    version: Option<&str>,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let encoded = if let Some(scoped) = name.strip_prefix('@') {
        format!("@{}", encode_component(scoped))
    } else {
        encode_component(name)
    };
    // Exact versions and tags are one manifest request; a range resolves
    // against the abbreviated packument first, as `npm install` would.
    let spec = match version.map(VersionSpec::parse) {
        None => "latest".to_owned(),
        Some(VersionSpec::Exact(version) | VersionSpec::Tag(version)) => version,
        Some(VersionSpec::Range(range)) => {
            let Some(packument) = packument(&encoded, registry, client, dns_pin.clone()).await?
            else {
                return Ok(not_found_page(registry));
            };
            let (versions, latest) = published_versions(&packument);
            match npm_resolve(
                &range,
                versions.iter().map(String::as_str),
                latest.as_deref(),
            ) {
                Some(resolved) => resolved,
                None => return Err(version_not_found(name, &range, &versions)),
            }
        }
    };
    // Percent-encode the version/spec as its own path segment so ranges or
    // tags cannot alter the request path.
    let request_url = registry_url(
        &registry.base,
        &format!("{encoded}/{}", encode_component(&spec)),
    )?;
    let response = client
        .json_with_dns_pin(
            ArtifactType::Npm,
            request_url,
            true,
            registry.authorization.clone(),
            dns_pin.clone(),
        )
        .await?;
    let Some(response) = response else {
        // The package may exist without this version or tag: say which.
        if version.is_some()
            && let Some(packument) = packument(&encoded, registry, client, dns_pin).await?
        {
            let (versions, _) = published_versions(&packument);
            return Err(version_not_found(name, &spec, &versions));
        }
        return Ok(not_found_page(registry));
    };
    let row = object_for(&response, ArtifactType::Npm)?;
    let returned = required(row.get("name"), ArtifactType::Npm)?;
    if returned != name {
        return Err(ArtifactError::new(
            "provider_error",
            "npm registry returned a different package name.",
        ));
    }
    let version = required(row.get("version"), ArtifactType::Npm)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Npm,
        returned.clone(),
        format!(
            "{}/{}",
            trim_registry(&registry.base),
            encode_component(&returned)
        ),
    );
    artifact.description = string(row.get("description"));
    artifact.license = match row.get("license") {
        Some(Value::Object(value)) => string(value.get("type")),
        value => string(value),
    };
    artifact.homepage = string(row.get("homepage"));
    artifact.source_ref = commit_sha(row.get("gitHead"));
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
    release_facts(&mut artifact, row);
    if let Some(attested) =
        attested_commit(&artifact, &version, row, registry, client, dns_pin).await
    {
        artifact.source_ref = Some(attested);
        artifact.source_attested = true;
    }
    artifact.version = Some(version);
    Ok(ArtifactProviderPage {
        artifacts: vec![artifact],
        next_state: None,
        total: Some(1),
        terminal_limit: None,
        registry: Some(trim_registry(&registry.base)),
    })
}

fn not_found_page(registry: &ResolvedNpmRegistry) -> ArtifactProviderPage {
    let mut page = ArtifactProviderPage::empty(Some(0));
    page.registry = Some(trim_registry(&registry.base));
    page
}

/// The abbreviated packument (versions and dist-tags only); `None` when the
/// package does not exist.
async fn packument(
    encoded: &str,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<Option<Value>, ArtifactError> {
    client
        .json_as(
            ArtifactType::Npm,
            registry_url(&registry.base, encoded)?,
            true,
            registry.authorization.clone(),
            dns_pin,
            NPM_INSTALL_JSON,
        )
        .await
}

fn published_versions(packument: &Value) -> (Vec<String>, Option<String>) {
    let versions = packument
        .get("versions")
        .and_then(Value::as_object)
        .map(|versions| versions.keys().cloned().collect())
        .unwrap_or_default();
    let latest = string(packument.pointer("/dist-tags/latest"));
    (versions, latest)
}

/// A known package without the requested version: typed, with the nearest
/// published versions as the recovery.
pub(crate) fn version_not_found(name: &str, requested: &str, versions: &[String]) -> ArtifactError {
    let close = nearest(requested, versions.iter().map(String::as_str));
    let error = ArtifactError::new(
        "versionNotFound",
        format!("{name} has no published version matching \"{requested}\""),
    )
    .with_status(404);
    if close.is_empty() {
        error.with_hint("Omit version for the latest release.")
    } else {
        error.with_hint(format!("Nearest published: {}.", close.join(", ")))
    }
}

/// Release facts the version manifest already carries.
fn release_facts(artifact: &mut ArtifactItem, row: &serde_json::Map<String, Value>) {
    let count = |field: &str| {
        row.get(field)
            .and_then(Value::as_object)
            .map(|deps| deps.len())
    };
    artifact.dependencies = count("dependencies").or(Some(0));
    artifact.peer_dependencies = count("peerDependencies").filter(|count| *count > 0);
    artifact.deprecated = string(row.get("deprecated"));
    artifact.engines = string(row.get("engines").and_then(|engines| engines.get("node")));
    artifact.published_at = published_at(row);
}

/// npm's version manifest has no publish time, but its upload record does:
/// `_npmOperationalInternal.tmp` is `tmp/<name>_<version>_<epoch-ms>_<rand>`,
/// stamped at publish (the packument `time` field agrees to the second).
fn published_at(row: &serde_json::Map<String, Value>) -> Option<String> {
    let tmp = row.get("_npmOperationalInternal")?.get("tmp")?.as_str()?;
    tmp.rsplit('_')
        .find(|part| part.len() == 13 && part.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|millis| millis.parse::<i64>().ok())
        .map(date_from_millis)
}

/// The source commit of an npm provenance attestation, accepted only when the
/// attestation binds this exact tarball (subject digest = `dist.integrity`)
/// and its source repository is the manifest's repository. The registry
/// verifies the Sigstore bundle at publish; this does not re-verify
/// signatures. Any failure leaves the unverified `gitHead` lead.
async fn attested_commit(
    artifact: &ArtifactItem,
    version: &str,
    row: &serde_json::Map<String, Value>,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Option<String> {
    let dist = row.get("dist")?;
    let url = Url::parse(dist.pointer("/attestations/url")?.as_str()?).ok()?;
    // Never follow an attestation URL off the registry host.
    if url.host_str() != registry.base.host_str() || url.scheme() != registry.base.scheme() {
        return None;
    }
    let integrity = dist.get("integrity")?.as_str()?;
    let bundle = client
        .json_with_dns_pin(
            ArtifactType::Npm,
            url,
            true,
            registry.authorization.clone(),
            dns_pin,
        )
        .await
        .ok()??;
    provenance_commit(
        &bundle,
        &artifact.name,
        version,
        integrity,
        artifact.repository.as_deref()?,
    )
}

pub(crate) fn provenance_commit(
    bundle: &Value,
    name: &str,
    version: &str,
    integrity: &str,
    repository: &str,
) -> Option<String> {
    use base64::Engine as _;
    let engine = base64::engine::general_purpose::STANDARD;
    let tarball_sha512 = hex::encode(engine.decode(integrity.strip_prefix("sha512-")?).ok()?);
    let purl = format!("pkg:npm/{}@{version}", name.replacen('@', "%40", 1));
    let same_repo = |uri: &str| {
        let uri = uri.strip_prefix("git+").unwrap_or(uri);
        let uri = uri.split_once('@').map_or(uri, |(repo, _)| repo);
        normalize_repository(uri).is_some_and(|uri| {
            uri.trim_end_matches('/')
                .eq_ignore_ascii_case(repository.trim_end_matches('/'))
        })
    };
    bundle
        .get("attestations")?
        .as_array()?
        .iter()
        .filter(|attestation| {
            attestation.get("predicateType").and_then(Value::as_str)
                == Some("https://slsa.dev/provenance/v1")
        })
        .find_map(|attestation| {
            let payload = attestation
                .pointer("/bundle/dsseEnvelope/payload")?
                .as_str()?;
            let statement: Value = serde_json::from_slice(&engine.decode(payload).ok()?).ok()?;
            let bound = statement.get("subject")?.as_array()?.iter().any(|subject| {
                subject.get("name").and_then(Value::as_str) == Some(purl.as_str())
                    && subject
                        .pointer("/digest/sha512")
                        .and_then(Value::as_str)
                        .is_some_and(|digest| digest.eq_ignore_ascii_case(&tarball_sha512))
            });
            if !bound {
                return None;
            }
            let source = statement
                .pointer("/predicate/buildDefinition/resolvedDependencies")?
                .as_array()?
                .first()?;
            if !same_repo(source.get("uri")?.as_str()?) {
                return None;
            }
            commit_sha(source.pointer("/digest/gitCommit")).filter(|sha| sha.len() == 40)
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
        artifact.downloads_monthly = entry.pointer("/downloads/monthly").and_then(Value::as_u64);
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

    use super::super::http::{
        ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse, RegistryClient,
    };
    use crate::providers::RequestBudget;
    use base64::Engine as _;
    use serde_json::{Value, json};

    /// Answers by URL path and Accept type; anything else is a 404.
    struct RouteHttp {
        routes: Vec<(&'static str, &'static str, Value)>,
        seen: std::sync::Mutex<Vec<String>>,
    }

    impl ArtifactHttp for RouteHttp {
        fn get<'a>(
            &'a self,
            req: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            let path = req.url.path().to_owned();
            self.seen
                .lock()
                .expect("seen")
                .push(format!("{} {path}", req.accept));
            let found = self
                .routes
                .iter()
                .find(|(route, accept, _)| *route == path && *accept == req.accept)
                .map(|(_, _, body)| serde_json::to_vec(body).expect("body"));
            Box::pin(async move {
                Ok(match found {
                    Some(body) => ArtifactHttpResponse { status: 200, body },
                    None => ArtifactHttpResponse {
                        status: 404,
                        body: b"{}".to_vec(),
                    },
                })
            })
        }
    }

    const JSON: &str = "application/json";
    const INSTALL: &str = super::NPM_INSTALL_JSON;
    const COMMIT: &str = "59bbc03e10c636b9eb3c393dfeb552819774ec21";
    const TARBALL: &[u8] = b"zod tarball bytes";

    fn sha512(bytes: &[u8]) -> Vec<u8> {
        use sha2::Digest as _;
        sha2::Sha512::digest(bytes).to_vec()
    }

    fn manifest(version: &str, attested: bool) -> Value {
        let integrity = format!(
            "sha512-{}",
            base64::engine::general_purpose::STANDARD.encode(sha512(TARBALL))
        );
        let mut dist = json!({"integrity": integrity});
        if attested {
            dist["attestations"] = json!({
                "url": format!("https://registry.npmjs.org/-/npm/v1/attestations/zod@{version}"),
                "provenance": {"predicateType": "https://slsa.dev/provenance/v1"}
            });
        }
        json!({
            "name": "zod", "version": version, "license": "MIT",
            "description": "schemas", "homepage": "https://github.com/colinhacks/zod#readme",
            "repository": {"type": "git", "url": "git+https://github.com/colinhacks/zod.git"},
            "gitHead": "aaaaaaa1",
            "dependencies": {}, "peerDependencies": {"typescript": "*"},
            "engines": {"node": ">=18"},
            "deprecated": (version == "3.22.0").then_some("Use 3.22.4"),
            "_npmOperationalInternal": {"tmp": format!("tmp/zod_{version}_1789341914484_0.59")},
            "dist": dist
        })
    }

    fn bundle(version: &str, repo: &str, digest: &[u8]) -> Value {
        let statement = json!({
            "subject": [{"name": format!("pkg:npm/zod@{version}"), "digest": {"sha512": hex::encode(digest)}}],
            "predicateType": "https://slsa.dev/provenance/v1",
            "predicate": {"buildDefinition": {"resolvedDependencies": [
                {"uri": format!("git+{repo}@refs/heads/main"), "digest": {"gitCommit": COMMIT}}
            ]}}
        });
        let payload = base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_vec(&statement).expect("statement"));
        json!({"attestations": [
            {"predicateType": "https://github.com/npm/attestation/tree/main/specs/publish/v0.1", "bundle": {}},
            {"predicateType": "https://slsa.dev/provenance/v1", "bundle": {"dsseEnvelope": {"payload": payload}}}
        ]})
    }

    async fn lookup(
        http: &RouteHttp,
        fields: Value,
    ) -> Result<super::ArtifactProviderPage, super::ArtifactError> {
        let budget = RequestBudget::with_timeout(std::time::Duration::from_secs(10), 10_000_000);
        let client = RegistryClient {
            http,
            budget: &budget,
            cache_revision: 0,
            cache_enabled: false,
        };
        let mut query = json!({"type": "npm"});
        for (key, value) in fields.as_object().expect("fields") {
            query[key] = value.clone();
        }
        let query = artifact_query(query, None);
        super::npm(
            &query,
            &Default::default(),
            &npm_registry("https://registry.npmjs.org/"),
            &client,
            false,
        )
        .await
    }

    fn routes(extra: Vec<(&'static str, &'static str, Value)>) -> RouteHttp {
        let mut routes = vec![
            (
                "/zod",
                INSTALL,
                json!({"name":"zod","dist-tags":{"latest":"4.6.5"},
                "versions":{"3.22.0":{},"3.25.76":{},"4.6.5":{}}}),
            ),
            ("/zod/3.25.76", JSON, manifest("3.25.76", false)),
            ("/zod/3.22.0", JSON, manifest("3.22.0", false)),
        ];
        routes.extend(extra);
        RouteHttp {
            routes,
            seen: std::sync::Mutex::new(vec![]),
        }
    }

    #[tokio::test]
    async fn npm_versions_resolve_exact_range_tag_and_coordinate() {
        let http = routes(vec![("/zod/latest", JSON, manifest("4.6.5", false))]);
        let range = lookup(&http, json!({"packageName": "zod", "version": "^3"}))
            .await
            .expect("range");
        let item = &range.artifacts[0];
        assert_eq!(item.version.as_deref(), Some("3.25.76"));
        assert_eq!(item.published_at.as_deref(), Some("2026-09-13"));
        assert_eq!(item.dependencies, Some(0));
        assert_eq!(item.peer_dependencies, Some(1));
        assert_eq!(item.engines.as_deref(), Some(">=18"));
        let coordinate = lookup(&http, json!({"packageName": "zod@3.22.0"}))
            .await
            .expect("coordinate");
        assert_eq!(coordinate.artifacts[0].version.as_deref(), Some("3.22.0"));
        assert_eq!(
            coordinate.artifacts[0].deprecated.as_deref(),
            Some("Use 3.22.4")
        );
        let tag = lookup(&http, json!({"packageName": "zod", "version": "latest"}))
            .await
            .expect("tag");
        assert_eq!(tag.artifacts[0].version.as_deref(), Some("4.6.5"));
        let missing = lookup(&http, json!({"packageName": "zod", "version": "3.22.9"}))
            .await
            .expect_err("missing version");
        assert_eq!(missing.code, "versionNotFound");
        assert_eq!(missing.status, Some(404));
        assert!(missing.hints[0].contains("3.22.0"), "{missing:?}");
        let unmatched = lookup(&http, json!({"packageName": "zod", "version": "^9"}))
            .await
            .expect_err("no match");
        assert_eq!(unmatched.code, "versionNotFound");
        let beyond = lookup(&http, json!({"packageName": "zod", "version": "99.0.0"}))
            .await
            .expect_err("beyond every release");
        assert_eq!(beyond.code, "versionNotFound");
        assert_eq!(
            beyond.hints,
            ["Nearest published: 4.6.5, 3.25.76, 3.22.0."],
            "{beyond:?}"
        );
        let seen = http.seen.lock().expect("seen").clone();
        assert_eq!(
            seen[..2],
            [format!("{INSTALL} /zod"), format!("{JSON} /zod/3.25.76")],
            "a range reads the abbreviated packument, then one manifest"
        );
    }

    #[tokio::test]
    async fn npm_provenance_pins_the_attested_commit_only_when_bound() {
        let good = routes(vec![
            ("/zod/4.6.5", JSON, manifest("4.6.5", true)),
            (
                "/-/npm/v1/attestations/zod@4.6.5",
                JSON,
                bundle(
                    "4.6.5",
                    "https://github.com/colinhacks/zod",
                    &sha512(TARBALL),
                ),
            ),
        ]);
        let page = lookup(&good, json!({"packageName": "zod", "version": "4.6.5"}))
            .await
            .expect("attested");
        let item = &page.artifacts[0];
        assert_eq!(item.source_ref.as_deref(), Some(COMMIT));
        assert!(item.source_attested);

        for (repo, digest) in [
            ("https://github.com/evil/zod", sha512(TARBALL)),
            (
                "https://github.com/colinhacks/zod",
                sha512(b"other tarball"),
            ),
        ] {
            let spoofed = routes(vec![
                ("/zod/4.6.5", JSON, manifest("4.6.5", true)),
                (
                    "/-/npm/v1/attestations/zod@4.6.5",
                    JSON,
                    bundle("4.6.5", repo, &digest),
                ),
            ]);
            let page = lookup(&spoofed, json!({"packageName": "zod", "version": "4.6.5"}))
                .await
                .expect("unbound attestation");
            let item = &page.artifacts[0];
            assert!(!item.source_attested, "{repo}");
            assert_eq!(
                item.source_ref.as_deref(),
                Some("aaaaaaa1"),
                "gitHead lead stays"
            );
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
