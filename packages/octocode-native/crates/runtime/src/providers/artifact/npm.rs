//! npm: exact package versions (manifest, release facts, provenance or
//! release-tag source ref) and keyword search, on the configured registry.
use super::http::{DnsPin, JSON, NPM_INSTALL_JSON, RegistryClient};
use super::util::{
    canonical_repository, commit_sha, encode_component, object_for, required, string,
};
use super::versions::{VersionSpec, npm_resolve, version_not_found};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ResolvedNpmRegistry,
};
use crate::providers::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use provenance::attested_commit;
use registry::{registry_url, trim_registry, validate_registry};
use search::search;
use serde_json::Value;
use std::collections::BTreeMap;

mod provenance;
pub(super) mod registry;
mod search;

pub(crate) async fn npm(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    allow_private_registry: bool,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let dns_pin = validate_registry(query, registry, allow_private_registry)?;
    if let Some(name) = artifact_search_query::package_name(query) {
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
    let name = artifact.package_name.clone();
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

#[cfg(test)]
mod tests;
