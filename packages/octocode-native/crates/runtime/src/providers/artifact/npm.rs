use super::http::{DnsPin, JSON, NPM_INSTALL_JSON, RegistryClient};
use super::util::{
    canonical_repository, commit_sha, encode_component, endpoint, object_for, required, safe_url,
    string, total,
};
use super::versions::{VersionSpec, npm_resolve, version_not_found};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType, ResolvedNpmRegistry, TerminalLimit,
};
use crate::policy::network::is_public;
use crate::providers::artifact::types::artifact_search_query;
use octocode_contract_types::ErrorCode;
use serde_json::Value;
use std::collections::BTreeMap;
use std::net::{SocketAddr, ToSocketAddrs};
use url::{Host, Url};

pub(crate) async fn npm(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    allow_private_registry: bool,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let dns_pin = validate_registry(query, registry, allow_private_registry)?;
    if let Some(name) = artifact_search_query::bare_package_name(query) {
        exact(
            name,
            artifact_search_query::version(query),
            registry,
            client,
            dns_pin,
        )
        .await
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

fn trim_registry(url: &Url) -> String {
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

fn validate_resolved_addresses(addresses: &[SocketAddr]) -> Result<(), ArtifactError> {
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
    let Some(spec) =
        manifest_spec(name, &encoded, version, registry, client, dns_pin.clone()).await?
    else {
        return Ok(not_found_page(registry));
    };
    // Percent-encode the version/spec as its own path segment so ranges or
    // tags cannot alter the request path.
    let request_url = registry_url(
        &registry.base,
        &format!("{encoded}/{}", encode_component(&spec)),
    )?;
    let authorization = registry.authorization_for(&request_url);
    let response = client
        .json(
            ArtifactType::Npm,
            request_url,
            true,
            authorization,
            dns_pin.clone(),
            JSON,
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
    let (mut artifact, version) = manifest_row(name, row, registry)?;
    if let Some(attested) =
        attested_commit(&artifact, &version, row, registry, client, dns_pin).await
    {
        artifact.source_ref = Some(attested);
        artifact.source_attested = true;
    } else if let (Some(repository), Some(head)) =
        (artifact.repository.clone(), artifact.source_ref.clone())
        && super::release_ref::github_ref_missing(&repository, &head, client).await
    {
        // A `gitHead` GitHub does not have pins no lead: the default branch
        // is read instead, and the response names the dead ref.
        artifact.source_ref = None;
        artifact.missing_ref = Some(head);
    }
    // Without an attested or recorded commit, the upstream release tag pins
    // the source, as for every registry that names no commit.
    let name = artifact.name.clone();
    super::release_ref::pin_release_tag(&mut artifact, &name, &version, client).await;
    artifact.version = Some(version);
    Ok(ArtifactProviderPage {
        registry: Some(trim_registry(&registry.base)),
        ..ArtifactProviderPage::single(artifact)
    })
}

/// The manifest to read for `version`: `latest` when none, an exact
/// version or tag as given, and a range resolved against the abbreviated
/// packument, as `npm install` would. `None` when the package does not exist.
async fn manifest_spec(
    name: &str,
    encoded: &str,
    version: Option<&str>,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<Option<String>, ArtifactError> {
    Ok(Some(match version.map(VersionSpec::parse) {
        None => "latest".to_owned(),
        Some(VersionSpec::Exact(version) | VersionSpec::Tag(version)) => version,
        Some(VersionSpec::Range(range)) => {
            let Some(packument) = packument(encoded, registry, client, dns_pin).await? else {
                return Ok(None);
            };
            let (versions, latest) = published_versions(&packument);
            npm_resolve(
                &range,
                versions.iter().map(String::as_str),
                latest.as_deref(),
            )
            .ok_or_else(|| version_not_found(name, &range, &versions))?
        }
    }))
}

/// A version manifest's row and its version, before the release ref is
/// checked: the manifest must name the requested package.
fn manifest_row(
    name: &str,
    row: &serde_json::Map<String, Value>,
    registry: &ResolvedNpmRegistry,
) -> Result<(ArtifactItem, String), ArtifactError> {
    let returned = required(row.get("name"), ArtifactType::Npm)?;
    if returned != name {
        return Err(ArtifactError::new(
            ErrorCode::ProviderError,
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
        Some(Value::String(value)) => artifact.repository = canonical_repository(value),
        Some(Value::Object(value)) => {
            artifact.repository = value
                .get("url")
                .and_then(Value::as_str)
                .and_then(canonical_repository);
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
    Ok((artifact, version))
}

fn not_found_page(registry: &ResolvedNpmRegistry) -> ArtifactProviderPage {
    let mut page = ArtifactProviderPage::empty(Some(0));
    page.registry = Some(trim_registry(&registry.base));
    page
}

/// The published versions of a package and its `latest` tag, from the
/// abbreviated packument; `None` when the package does not exist. Only
/// that view is decoded and cached: a large packument (typescript's is
/// 8.7 MB) never lives in memory as a whole.
async fn packument(
    encoded: &str,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<Option<Value>, ArtifactError> {
    let url = registry_url(&registry.base, encoded)?;
    let authorization = registry.authorization_for(&url);
    client
        .json_view(
            ArtifactType::Npm,
            url,
            true,
            authorization,
            dns_pin,
            NPM_INSTALL_JSON,
            "versions",
            version_view,
        )
        .await
}

/// `{versions: [...], latest}` of an abbreviated packument body, versions
/// in the registry's order; the version manifests are skipped, never built
/// into values.
fn version_view(body: &[u8]) -> Option<Value> {
    /// The keys of a JSON object, in order, without their values.
    #[derive(Default)]
    struct Keys(Vec<String>);
    impl<'de> serde::Deserialize<'de> for Keys {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visit;
            impl<'de> serde::de::Visitor<'de> for Visit {
                type Value = Keys;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("an object")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Keys, A::Error> {
                    let mut keys = Vec::new();
                    while let Some(key) = map.next_key::<String>()? {
                        map.next_value::<serde::de::IgnoredAny>()?;
                        keys.push(key);
                    }
                    Ok(Keys(keys))
                }
            }
            deserializer.deserialize_map(Visit)
        }
    }
    #[derive(serde::Deserialize)]
    struct Packument {
        #[serde(default)]
        versions: Keys,
        #[serde(rename = "dist-tags", default)]
        dist_tags: BTreeMap<String, Value>,
    }
    let packument: Packument = serde_json::from_slice(body).ok()?;
    let latest = string(packument.dist_tags.get("latest"));
    Some(serde_json::json!({ "versions": packument.versions.0, "latest": latest }))
}

fn published_versions(view: &Value) -> (Vec<String>, Option<String>) {
    let versions = view
        .get("versions")
        .and_then(Value::as_array)
        .map(|versions| {
            versions
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    (versions, string(view.get("latest")))
}

/// Release facts the version manifest already carries.
fn release_facts(artifact: &mut ArtifactItem, row: &serde_json::Map<String, Value>) {
    // A manifest without `dependencies` declares none. npm also lists each
    // `optionalDependencies` entry under `dependencies`: those are not
    // required, so they are listed as optional instead.
    let optional = row.get("optionalDependencies").and_then(Value::as_object);
    let entry = |(name, range): (&String, &Value)| match range.as_str() {
        Some(range) => format!("{name}@{range}"),
        None => name.clone(),
    };
    let declared = |field: &str| -> Vec<String> {
        row.get(field)
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter(|(name, _)| optional.is_none_or(|optional| !optional.contains_key(*name)))
            .map(entry)
            .collect()
    };
    artifact.dependencies = Some(declared("dependencies"));
    artifact.optional_dependencies = optional
        .map(|optional| optional.iter().map(entry).collect::<Vec<_>>())
        .filter(|optional| !optional.is_empty());
    // Peers are rare: a version that declares none shows no list (its
    // `dependencies` list already says the manifest was read).
    let peers = declared("peerDependencies");
    artifact.peer_dependencies = (!peers.is_empty()).then_some(peers);
    artifact.deprecated = string(row.get("deprecated"));
    artifact.engines = row
        .get("engines")
        .and_then(Value::as_object)
        .map(|engines| {
            engines
                .iter()
                .filter_map(|(name, range)| Some((name.clone(), range.as_str()?.to_owned())))
                .collect::<BTreeMap<_, _>>()
        })
        .filter(|engines| !engines.is_empty());
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
        .map(crate::civil_date::date_millis)
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
    // Attestations must stay on the registry origin; credential path scope is checked below.
    if url.origin() != registry.base.origin()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let integrity = dist.get("integrity")?.as_str()?;
    let authorization = registry.authorization_for(&url);
    let bundle = client
        .json(ArtifactType::Npm, url, true, authorization, dns_pin, JSON)
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
        canonical_repository(uri).is_some_and(|uri| {
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
    let size = artifact_search_query::page_size(query);
    let window = |from: u64, size: usize| {
        search_window(query, registry, client, dns_pin.clone(), from, size)
    };
    let (total_found, objects) = window(offset, size).await?;
    // npm serves a deep window from an undocumented reach on and answers a
    // `from` past it with earlier rows instead of an error. The page is its
    // own rows only when the two rows ending just before it are the row
    // before its first and its first: a restarted or clamped window fails
    // that either way.
    if offset > 0
        && let Some(first) = objects.first()
    {
        let (_, probe) = window(offset - 1, 2).await?;
        let name = |entry: Option<&Value>| {
            entry
                .and_then(|entry| entry.pointer("/package/name"))
                .cloned()
        };
        let first = name(Some(first));
        if name(probe.first()) == first || name(probe.get(1)) != first {
            return Ok(ArtifactProviderPage {
                artifacts: Vec::new(),
                next_state: None,
                total: Some(total_found),
                terminal_limit: Some(unreachable_rows(offset, total_found)),
                registry: Some(trim_registry(&registry.base)),
            });
        }
    }
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
                ErrorCode::ProviderError,
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
                .and_then(canonical_repository);
        }
        artifacts.push(artifact);
    }
    let has_more = offset.saturating_add(artifacts.len() as u64) < total_found;
    let terminal_limit =
        (has_more && artifacts.is_empty()).then(|| unreachable_rows(offset, total_found));
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

/// One npm search window: the registry's total and its result objects.
async fn search_window(
    query: &ArtifactSearchQuery,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
    from: u64,
    size: usize,
) -> Result<(u64, Vec<Value>), ArtifactError> {
    let url = endpoint(
        &format!("{}/-/v1/search", trim_registry(&registry.base)),
        &[
            ("text", Some(artifact_search_query::terms(query))),
            ("size", Some(size.to_string())),
            ("from", (from > 0).then(|| from.to_string())),
        ],
    )?;
    let authorization = registry.authorization_for(&url);
    let mut response = client
        .json(ArtifactType::Npm, url, false, authorization, dns_pin, JSON)
        .await?
        .ok_or_else(|| {
            ArtifactError::new(ErrorCode::ProviderError, "npm registry search failed.")
        })?;
    let data = object_for(&response, ArtifactType::Npm)?;
    let total_found = total(data.get("total")).ok_or_else(|| {
        ArtifactError::new(
            ErrorCode::ProviderError,
            "npm registry search omitted a valid total; pagination cannot be determined.",
        )
    })?;
    match response.get_mut("objects").map(Value::take) {
        Some(Value::Array(objects)) => Ok((total_found, objects)),
        _ => Err(ArtifactError::new(
            ErrorCode::ProviderError,
            "Invalid npm registry search response; expected an object with results and a total.",
        )),
    }
}

/// The terminal limit of a page npm does not serve: the rows before it are
/// the most this search reaches.
fn unreachable_rows(offset: u64, total: u64) -> TerminalLimit {
    TerminalLimit::result_cap(format!(
        "npm serves no rows past the first {offset} for this search: at most {offset} of {total} are reachable."
    ))
}

fn registry_url(base: &Url, path: &str) -> Result<Url, ArtifactError> {
    Url::parse(&format!("{}/{}", trim_registry(base), path)).map_err(|_| invalid_registry_url())
}

#[cfg(test)]
mod tests;
