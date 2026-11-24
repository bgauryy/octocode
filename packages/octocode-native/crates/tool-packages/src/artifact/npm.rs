//! npm: exact package versions (manifest, release facts, provenance or
//! release-tag source ref) and keyword search, on the configured registry.
use super::http::{DnsPin, JSON, NPM_INSTALL_JSON, RegistryClient};
use super::util::{commit_sha, encode_component, github_slug, object_for, required, string};
use super::versions::{VersionSpec, npm_resolve, version_not_found};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, RepositoryStatus,
    ResolvedNpmRegistry,
};
use crate::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use provenance::attested_commit;
use registry::{package_page, registry_url, trim_registry, validate_registry};
pub(super) use search::NPM_REACHABLE_END;
use search::search;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

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
        if let Some(asked) = version
            && let Some(packument) = packument(&encoded, registry, client, dns_pin).await?
        {
            let published = Published::of(&packument);
            return Err(super::versions::discouraged_last(
                version_not_found(ArtifactType::Npm, name, asked, &published.versions),
                |version| published.deprecated.contains(version),
            ));
        }
        return Ok(not_found_page(registry));
    };
    let row = object_for(&response, ArtifactType::Npm)?;
    let (mut artifact, version) = manifest_row(name, row, registry)?;
    // An older manifest has no upload record to date it: npm keeps every
    // version's publish time in the full packument. A packument that cannot
    // be read leaves the date unknown, and the row says so. The date and the
    // attestation are separate registry reads: they run together.
    let undated = artifact.published_at.is_none();
    let (dated, attested) = tokio::join!(
        async {
            match undated {
                true => Some(
                    packument_published_at(&encoded, &version, registry, client, dns_pin.clone())
                        .await,
                ),
                false => None,
            }
        },
        attested_commit(&artifact, &version, row, registry, client, dns_pin.clone()),
    );
    match dated {
        Some(Ok(date)) => artifact.published_at = date,
        Some(Err(error)) if error.code == ErrorCode::Cancelled => return Err(error),
        Some(Err(error)) => note_unread(
            &mut artifact,
            &version,
            "publish date unread, the full packument could not be read",
            &error,
            "publishedAt is unknown, not absent",
        ),
        None => {}
    }
    let attested = match attested {
        Ok(attested) => attested,
        Err(error) if error.code == ErrorCode::Cancelled => return Err(error),
        Err(error) => {
            note_unread(
                &mut artifact,
                &version,
                "provenance unchecked, the attestation bundle could not be read",
                &error,
                "the source ref is not attested",
            );
            None
        }
    };
    let package = artifact.package_name.clone();
    // The release tag a recorded commit is checked against, looked up beside
    // the check that the commit exists: neither waits for the other.
    let mut searched = None;
    if let Some(attested) = attested {
        artifact.source_ref = Some(attested);
        artifact.source_attested = true;
    } else if let (Some(repository), Some(head)) =
        (artifact.repository.clone(), artifact.source_ref.clone())
    {
        let (check, search) = tokio::join!(
            super::release_ref::commit_check(&repository, &head, client),
            super::release_ref::release_tag_search(&artifact, &package, &version, &head, client),
        );
        searched = search;
        match check {
            Ok(check) => {
                if check == super::release_ref::CommitCheck::Missing {
                    // A `gitHead` GitHub does not have pins no lead: the default
                    // branch is read instead, and the response names the dead ref.
                    artifact.source_ref = None;
                    artifact.missing_ref = Some(head);
                }
                // A repository that is gone or moved is said on the row as well.
                artifact.note_repository(check);
            }
            Err(failure) => {
                artifact.note_unchecked(&format!("the recorded commit {head}"), failure);
            }
        }
    }
    // Without an attested or recorded commit, the upstream release tag pins
    // the source, as for every registry that names no commit; a recorded
    // commit is checked against that tag.
    super::release_ref::pin_release_tag(&mut artifact, &package, &version, None, client).await;
    // Both checks that can find the repository gone (the recorded commit's
    // above, the release-tag path's check of `HEAD`) have run: a gone
    // repository names where the latest release says the project lives. A
    // lookup of `latest` itself already read that release.
    if artifact.repository_status == Some(RepositoryStatus::Missing) && spec != "latest" {
        artifact.successor_repository =
            current_repository(name, &encoded, &artifact, registry, client, dns_pin).await;
    }
    if !artifact.source_attested
        && !artifact.source_tag
        && let Some(commit) = artifact.source_ref.clone()
    {
        match &searched {
            Some(search) => super::release_ref::settle_commit_against_tag(
                &mut artifact,
                &version,
                &commit,
                search,
            ),
            None => {
                super::release_ref::check_commit_against_tag(
                    &mut artifact,
                    &package,
                    &version,
                    &commit,
                    client,
                )
                .await;
            }
        }
    }
    artifact.version = Some(version);
    Ok(ArtifactProviderPage {
        registry: Some(trim_registry(&registry.base)),
        ..ArtifactProviderPage::single(artifact)
    })
}

/// The manifest to read for `version`: `latest` when none, an exact
/// version or tag as given, and a range resolved against the abbreviated
/// packument, as `npm install` would (a deprecated version only when every
/// match is). A range no version matches that is a dist-tag of the package
/// reads that tag's version. `None` when the package does not exist.
async fn manifest_spec(
    name: &str,
    encoded: &str,
    version: Option<&str>,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<Option<String>, ArtifactError> {
    Ok(Some(
        match version.map(|asked| (asked, VersionSpec::parse_npm(asked))) {
            None => "latest".to_owned(),
            Some((_, VersionSpec::Exact(version) | VersionSpec::Tag(version))) => version,
            Some((asked, VersionSpec::Range(range))) => {
                let Some(packument) = packument(encoded, registry, client, dns_pin).await? else {
                    return Ok(None);
                };
                let published = Published::of(&packument);
                match npm_resolve(
                    &range,
                    published.versions.iter().map(String::as_str),
                    published.tags.get("latest").map(String::as_str),
                    |version| published.deprecated.contains(version),
                ) {
                    Some(resolved) => resolved,
                    None => published.tags.get(asked.trim()).cloned().ok_or_else(|| {
                        version_not_found(ArtifactType::Npm, name, asked, &published.versions)
                    })?,
                }
            }
        },
    ))
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
        package_page(registry, &returned),
    );
    artifact.description = string(row.get("description"));
    artifact.license = match row.get("license") {
        Some(Value::Object(value)) => string(value.get("type")),
        value => string(value),
    };
    artifact.homepage = string(row.get("homepage"));
    artifact.source_ref = commit_sha(row.get("gitHead"));
    match row.get("repository") {
        Some(Value::String(value)) => artifact.set_source_link(value),
        Some(Value::Object(value)) => {
            // The registry's own `directory` is stated: a link's page never
            // replaces it, and an absent one never erases what the link gave.
            artifact.repository_directory = string(value.get("directory")).map(|value| {
                value
                    .trim_start_matches("./")
                    .trim_start_matches('/')
                    .to_owned()
            });
            artifact.repository_directory = artifact
                .repository_directory
                .take()
                .filter(|directory| !directory.is_empty());
            if let Some(link) = value.get("url").and_then(Value::as_str) {
                artifact.set_source_link(link);
            }
        }
        _ => {}
    }
    release_facts(&mut artifact, row);
    Ok((artifact, version))
}

/// Where the registry's latest release says the package's source lives, when
/// that is a GitHub repository other than the one `artifact` names (which
/// GitHub no longer has). A lookup that fails or names nothing gives no hint.
async fn current_repository(
    name: &str,
    encoded: &str,
    artifact: &ArtifactItem,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Option<String> {
    let url = registry_url(&registry.base, &format!("{encoded}/latest")).ok()?;
    let authorization = registry.authorization_for(&url);
    let response = client
        .json(ArtifactType::Npm, url, true, authorization, dns_pin, JSON)
        .await
        .ok()??;
    let (latest, _) = manifest_row(
        name,
        object_for(&response, ArtifactType::Npm).ok()?,
        registry,
    )
    .ok()?;
    let (owner, repo) = github_slug(latest.repository.as_deref()?)?;
    let stale = github_slug(artifact.repository.as_deref()?)?;
    (!owner.eq_ignore_ascii_case(&stale.0) || !repo.eq_ignore_ascii_case(&stale.1))
        .then(|| format!("{owner}/{repo}"))
}

/// States on the row a registry read that failed, so the fact it would have
/// given is unknown rather than absent: `what` names the fact and the read,
/// `so` what the row's field means now.
fn note_unread(
    artifact: &mut ArtifactItem,
    version: &str,
    what: &str,
    error: &ArtifactError,
    so: &str,
) {
    artifact.disclosures.push(format!(
        "{} {version}: {what} ({}); {so}.",
        artifact.package_name,
        error.message.trim_end_matches('.')
    ));
}

fn not_found_page(registry: &ResolvedNpmRegistry) -> ArtifactProviderPage {
    let mut page = ArtifactProviderPage::empty(Some(0));
    page.registry = Some(trim_registry(&registry.base));
    page
}

/// The published versions of a package and its `latest` tag, from the
/// abbreviated packument; `None` when the package does not exist. Only
/// that view is decoded, so unrelated manifest fields are skipped.
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
            version_view,
        )
        .await
}

/// The publish date of `version` from the full packument's `time` map,
/// which npm keeps for every version (the abbreviated packument has none).
/// Read only for a manifest that does not date itself. `Ok(None)`: the
/// packument records no well-formed date for the version; a failed read is
/// the caller's to state.
async fn packument_published_at(
    encoded: &str,
    version: &str,
    registry: &ResolvedNpmRegistry,
    client: &RegistryClient<'_>,
    dns_pin: Option<DnsPin>,
) -> Result<Option<String>, ArtifactError> {
    let url = registry_url(&registry.base, encoded)?;
    let authorization = registry.authorization_for(&url);
    let times = client
        .json_view(
            ArtifactType::Npm,
            url,
            true,
            authorization,
            dns_pin,
            JSON,
            time_view,
        )
        .await?;
    Ok(times
        .as_ref()
        .and_then(|times| publish_date(times, version)))
}

/// The date part of `version`'s publish instant in a packument `time` map.
fn publish_date(times: &Value, version: &str) -> Option<String> {
    let stamp = times.get(version)?.as_str()?;
    // npm states UTC instants (`2015-08-24T13:56:27.000Z`): the date part.
    let (date, clock) = (stamp.get(..10)?, stamp.get(10..)?);
    let shaped = date.bytes().enumerate().all(|(index, byte)| match index {
        4 | 7 => byte == b'-',
        _ => byte.is_ascii_digit(),
    }) && clock.starts_with('T');
    shaped.then(|| date.to_owned())
}

/// The `time` map of a full packument (version → publish instant); the
/// version manifests are skipped, never built into values.
fn time_view(body: &[u8]) -> Option<Value> {
    #[derive(serde::Deserialize)]
    struct Times {
        #[serde(default)]
        time: serde_json::Map<String, Value>,
    }
    let times: Times = serde_json::from_slice(body).ok()?;
    Some(Value::Object(times.time))
}

/// `{versions: [...], tags: {...}, deprecated: [...]}` of an abbreviated
/// packument body: the versions in the registry's order, its dist-tags, and
/// the versions whose manifest states a deprecation. Of each version
/// manifest only `deprecated` is read; the rest is skipped, never built into
/// values.
fn version_view(body: &[u8]) -> Option<Value> {
    /// Whether one version manifest is deprecated: npm writes the message
    /// (`true` counts too; an empty message does not). A manifest that is not
    /// an object states none.
    struct Deprecation(bool);
    impl<'de> serde::Deserialize<'de> for Deprecation {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visit;
            impl<'de> serde::de::Visitor<'de> for Visit {
                type Value = Deprecation;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("a version manifest")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Deprecation, A::Error> {
                    let mut deprecated = false;
                    while let Some(key) = map.next_key::<String>()? {
                        if key == "deprecated" {
                            deprecated = match map.next_value::<Value>()? {
                                Value::String(message) => !message.is_empty(),
                                Value::Bool(flag) => flag,
                                _ => false,
                            };
                        } else {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                    Ok(Deprecation(deprecated))
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(
                    self,
                    mut seq: A,
                ) -> Result<Deprecation, A::Error> {
                    while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                    Ok(Deprecation(false))
                }
                fn visit_bool<E>(self, _: bool) -> Result<Deprecation, E> {
                    Ok(Deprecation(false))
                }
                fn visit_i64<E>(self, _: i64) -> Result<Deprecation, E> {
                    Ok(Deprecation(false))
                }
                fn visit_u64<E>(self, _: u64) -> Result<Deprecation, E> {
                    Ok(Deprecation(false))
                }
                fn visit_f64<E>(self, _: f64) -> Result<Deprecation, E> {
                    Ok(Deprecation(false))
                }
                fn visit_str<E>(self, _: &str) -> Result<Deprecation, E> {
                    Ok(Deprecation(false))
                }
                fn visit_unit<E>(self) -> Result<Deprecation, E> {
                    Ok(Deprecation(false))
                }
            }
            deserializer.deserialize_any(Visit)
        }
    }
    /// The keys of a JSON object, in order, and those whose manifest is
    /// deprecated.
    #[derive(Default)]
    struct Versions {
        keys: Vec<String>,
        deprecated: Vec<String>,
    }
    impl<'de> serde::Deserialize<'de> for Versions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visit;
            impl<'de> serde::de::Visitor<'de> for Visit {
                type Value = Versions;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("an object")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Versions, A::Error> {
                    let mut versions = Versions::default();
                    while let Some(key) = map.next_key::<String>()? {
                        if map.next_value::<Deprecation>()?.0 {
                            versions.deprecated.push(key.clone());
                        }
                        versions.keys.push(key);
                    }
                    Ok(versions)
                }
            }
            deserializer.deserialize_map(Visit)
        }
    }
    #[derive(serde::Deserialize)]
    struct Packument {
        #[serde(default)]
        versions: Versions,
        #[serde(rename = "dist-tags", default)]
        dist_tags: BTreeMap<String, Value>,
    }
    let packument: Packument = serde_json::from_slice(body).ok()?;
    let tags = packument
        .dist_tags
        .into_iter()
        .filter_map(|(tag, version)| Some((tag, Value::String(string(Some(&version))?))))
        .collect::<serde_json::Map<_, _>>();
    Some(serde_json::json!({
        "versions": packument.versions.keys,
        "tags": tags,
        "deprecated": packument.versions.deprecated,
    }))
}

/// What [`version_view`] read of a packument.
struct Published {
    /// Every published version, in the registry's order.
    versions: Vec<String>,
    /// Dist-tag to version.
    tags: BTreeMap<String, String>,
    /// The versions whose manifest states a deprecation.
    deprecated: BTreeSet<String>,
}

impl Published {
    fn of(view: &Value) -> Self {
        let strings = |field: &str| {
            view.get(field)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
        };
        Self {
            versions: strings("versions").collect(),
            tags: view
                .get("tags")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
                .filter_map(|(tag, version)| Some((tag.clone(), version.as_str()?.to_owned())))
                .collect(),
            deprecated: strings("deprecated").collect(),
        }
    }
}

/// What a peer `peerDependenciesMeta` marks optional ends with.
pub(super) const OPTIONAL_PEER: &str = " (optional)";

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
    // `dependencies` list already says the manifest was read). A peer that
    // `peerDependenciesMeta` marks optional says so.
    let meta = row.get("peerDependenciesMeta").and_then(Value::as_object);
    let optional_peer = |name: &str| {
        meta.and_then(|meta| meta.get(name))
            .and_then(|entry| entry.get("optional"))
            .and_then(Value::as_bool)
            == Some(true)
    };
    let peers = row
        .get("peerDependencies")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, range)| {
            let mut peer = entry((name, range));
            if optional_peer(name) {
                peer.push_str(OPTIONAL_PEER);
            }
            peer
        })
        .collect::<Vec<_>>();
    artifact.peer_dependencies = (!peers.is_empty()).then_some(peers);
    artifact.deprecated = string(row.get("deprecated"));
    artifact.engines = row
        .get("engines")
        .and_then(Value::as_object)
        .map(|engines| {
            engines
                .iter()
                .filter_map(|(name, range)| {
                    Some(octocode_contract_types::ArtifactSearchRowEnginesItem {
                        name: name.clone(),
                        range: range.as_str()?.to_owned(),
                    })
                })
                .collect::<Vec<_>>()
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
        .map(octocode_runtime_base::civil_date::date_millis)
}

#[cfg(test)]
mod tests;
