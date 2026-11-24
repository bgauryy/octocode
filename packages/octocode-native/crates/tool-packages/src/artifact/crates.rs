//! crates.io: exact and default crate versions, their dependencies, the
//! packaging commit (or release tag), and keyword discovery.
use super::http::{JSON, RegistryClient};
use super::util::{
    date_prefix, keyword_rows, keyword_url, license, object_for, paged, parse_url, repository_url,
    required, safe_url, string, total,
};
use super::versions::{VersionSpec, cargo_resolve};
use super::{ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState};
use crate::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType};
use serde_json::Value;

fn crate_item(row: &serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let name = required(
        row.get("name").or_else(|| row.get("id")),
        ArtifactType::Crates,
    )?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Crates,
        name.clone(),
        format!(
            "https://crates.io/crates/{}",
            super::util::encode_component(&name)
        ),
    );
    artifact.version =
        string(row.get("max_stable_version")).or_else(|| string(row.get("max_version")));
    artifact.description = string(row.get("description"));
    artifact.license = license(row.get("license"));
    artifact.homepage = safe_url(row.get("homepage"));
    if let Some(link) = repository_url(row.get("repository")) {
        artifact.set_source_link(&link);
    }
    Ok(artifact)
}

/// One version's facts on its crate row.
fn apply_crate_version(artifact: &mut ArtifactItem, entry: &Value) {
    artifact.version = string(entry.get("num"));
    artifact.published_at = date_prefix(entry.get("created_at"));
    artifact.yanked = (entry.get("yanked") == Some(&Value::Bool(true))).then_some(true);
    artifact.yanked_reason = artifact.yanked.and(string(entry.get("yank_message")));
    artifact.rust_version = string(entry.get("rust_version"));
    artifact.license = license(entry.get("license")).or(artifact.license.take());
}

/// An exact crate version from its own record plus the crate's metadata
/// without its version list (`include=`): the same row the crate document
/// gives, without downloading every version. `None` when either is unknown,
/// so the crate-document read reports not-found or the nearest versions.
async fn crate_release(
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<ArtifactItem>, ArtifactError> {
    let encoded = super::util::encode_component(name);
    let metadata = parse_url(&format!(
        "https://crates.io/api/v1/crates/{encoded}?include="
    ))?;
    let record = parse_url(&format!(
        "https://crates.io/api/v1/crates/{encoded}/{}",
        super::util::encode_component(version)
    ))?;
    let (metadata, record) = tokio::join!(
        client.json(ArtifactType::Crates, metadata, true, None, None, JSON),
        client.json(ArtifactType::Crates, record, true, None, None, JSON)
    );
    let (Some(metadata), Some(record)) = (metadata?, record?) else {
        return Ok(None);
    };
    let row = object_for(
        object_for(&metadata, ArtifactType::Crates)?
            .get("crate")
            .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?,
        ArtifactType::Crates,
    )?;
    let entry = record
        .get("version")
        .filter(|entry| {
            string(entry.get("num"))
                .is_some_and(|num| listed_exact(version, std::slice::from_ref(&num)).is_some())
        })
        .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?;
    let mut artifact = crate_item(row)?;
    apply_crate_version(&mut artifact, entry);
    Ok(Some(artifact))
}

/// A crate's latest release from its metadata plus the one version record
/// crates.io names its default (`include=default_version`): the highest
/// stable version, else the highest prerelease, else the highest yanked
/// one, which the row then marks yanked. The same row the crate document
/// gives, without downloading every version. `None` when the crate is
/// unknown or names no default, so the crate-document read decides.
async fn crate_default_release(
    name: &str,
    client: &RegistryClient<'_>,
) -> Result<Option<ArtifactItem>, ArtifactError> {
    let url = parse_url(&format!(
        "https://crates.io/api/v1/crates/{}?include=default_version",
        super::util::encode_component(name)
    ))?;
    let Some(response) = client
        .json(ArtifactType::Crates, url, true, None, None, JSON)
        .await?
    else {
        return Ok(None);
    };
    let body = object_for(&response, ArtifactType::Crates)?;
    let row = object_for(
        body.get("crate")
            .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?,
        ArtifactType::Crates,
    )?;
    let default = row.get("default_version").and_then(Value::as_str);
    let Some(entry) = body
        .get("versions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|entry| default.is_some() && entry.get("num").and_then(Value::as_str) == default)
    else {
        return Ok(None);
    };
    // This view counts no versions (`max_version` is "0.0.0"): the
    // version is the default entry's.
    let mut artifact = crate_item(row)?;
    apply_crate_version(&mut artifact, entry);
    Ok(Some(artifact))
}

/// The commit a GitHub-hosted crate version was packaged from (cargo's VCS
/// info, with the crate's directory in the repository), checked against the
/// version's upstream release tag as npm's recorded commit is; else that
/// release tag. A crate's link is its newest release's, so with no directory
/// in the VCS info (cargo before 1.56 wrote none) the link's directory is
/// placed in the tree at the pinned commit
/// ([`super::link_directory::place_at_pinned_ref`]).
async fn crate_source_ref(artifact: &mut ArtifactItem, client: &RegistryClient<'_>) {
    let (Some(repository), Some(version)) = (artifact.repository.clone(), artifact.version.clone())
    else {
        return;
    };
    if !repository.contains("github.com") {
        return;
    }
    let name = artifact.package_name.clone();
    if let Some((sha, directory)) = vcs::crate_vcs(&artifact.package_name, &version, client).await {
        artifact.source_ref = Some(sha.clone());
        match directory {
            // The release's own directory (empty: the root) decides; the
            // crate's link does not.
            Some(directory) => {
                artifact.repository_directory = (!directory.is_empty()).then_some(directory);
                artifact.link_below_ref = None;
            }
            None => {
                super::link_directory::place_at_pinned_ref(artifact, &["Cargo.toml"], client).await;
            }
        }
        super::release_ref::check_commit_against_tag(artifact, &name, &version, &sha, client).await;
        return;
    }
    super::release_ref::pin_release_tag(artifact, &name, &version, None, client).await;
    super::link_directory::place_at_pinned_ref(artifact, &["Cargo.toml"], client).await;
}

/// A crate version's normal dependencies as `name req`, plus `; target`
/// for a platform-specific one (a crate may declare one dependency per
/// target, like a PEP 508 marker): the required ones, and the optional ones
/// (features, like PyPI extras) apart. A failed read leaves both unknown;
/// it never fails the lookup.
async fn crate_dependencies(
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<(Vec<String>, Vec<String>)> {
    let url = parse_url(&format!(
        "https://crates.io/api/v1/crates/{}/{}/dependencies",
        super::util::encode_component(name),
        super::util::encode_component(version)
    ))
    .ok()?;
    let listing = client
        .json(ArtifactType::Crates, url, true, None, None, JSON)
        .await
        .ok()??;
    let (optional, required): (Vec<_>, Vec<_>) = listing
        .get("dependencies")?
        .as_array()?
        .iter()
        .filter(|dependency| dependency.get("kind").and_then(Value::as_str) == Some("normal"))
        .filter_map(|dependency| {
            let name = dependency.get("crate_id").and_then(Value::as_str)?;
            let mut entry = match dependency.get("req").and_then(Value::as_str) {
                Some(req) => format!("{name} {req}"),
                None => name.to_owned(),
            };
            if let Some(target) = dependency.get("target").and_then(Value::as_str) {
                entry = format!("{entry}; {target}");
            }
            Some((
                dependency.get("optional") == Some(&Value::Bool(true)),
                entry,
            ))
        })
        .partition(|(optional, _)| *optional);
    let entries = |rows: Vec<(bool, String)>| rows.into_iter().map(|(_, entry)| entry).collect();
    Some((entries(required), entries(optional)))
}

/// Release facts of a resolved crate version that need their own reads:
/// the source ref and the dependency lists, read concurrently.
async fn crate_release_facts(artifact: &mut ArtifactItem, client: &RegistryClient<'_>) {
    let Some(version) = artifact.version.clone() else {
        return;
    };
    let name = artifact.package_name.clone();
    let (dependencies, ()) = tokio::join!(
        crate_dependencies(&name, &version, client),
        crate_source_ref(artifact, client)
    );
    if let Some((required, optional)) = dependencies {
        artifact.dependencies = Some(required);
        artifact.optional_dependencies = (!optional.is_empty()).then_some(optional);
    }
}

pub(crate) async fn crates(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = artifact_search_query::package_name(query) {
        let release = match artifact_search_query::version(query).map(VersionSpec::parse) {
            Some(VersionSpec::Exact(version)) => crate_release(name, &version, client).await?,
            None => crate_default_release(name, client).await?,
            Some(VersionSpec::Tag(tag)) if tag == "latest" => {
                crate_default_release(name, client).await?
            }
            Some(_) => None,
        };
        if let Some(mut artifact) = release {
            crate_release_facts(&mut artifact, client).await;
            return Ok(ArtifactProviderPage::single(artifact));
        }
        let url = parse_url(&format!(
            "https://crates.io/api/v1/crates/{}",
            super::util::encode_component(name)
        ))?;
        let Some(response) = client
            .json(ArtifactType::Crates, url, true, None, None, JSON)
            .await?
        else {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        };
        let body = object_for(&response, ArtifactType::Crates)?;
        let row = object_for(
            body.get("crate")
                .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?,
            ArtifactType::Crates,
        )?;
        let mut artifact = crate_item(row)?;
        // The crate response lists every version: exact, tag, and range
        // lookups resolve here without another request.
        let versions = body
            .get("versions")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let num = |entry: &Value| entry.get("num").and_then(Value::as_str).map(str::to_owned);
        let all = versions.iter().filter_map(num).collect::<Vec<_>>();
        let (yanked, releases): (Vec<_>, Vec<_>) = versions
            .iter()
            .partition(|entry| entry.get("yanked") == Some(&Value::Bool(true)));
        let yanked = yanked.into_iter().filter_map(num).collect::<Vec<_>>();
        // The versions nearest a missing one, and which of them are yanked;
        // the message quotes the version as the request typed it.
        let asked = artifact_search_query::version(query).unwrap_or_default();
        let missing = || {
            super::versions::discouraged_last(
                ArtifactError {
                    yanked: yanked.clone(),
                    ..super::versions::version_not_found(
                        ArtifactType::Crates,
                        &artifact.package_name,
                        asked,
                        &all,
                    )
                },
                |version| yanked.iter().any(|known| known == version),
            )
        };
        let wanted = match artifact_search_query::version(query).map(VersionSpec::parse) {
            None => artifact.version.clone(),
            Some(VersionSpec::Tag(tag)) if tag == "latest" => artifact.version.clone(),
            Some(VersionSpec::Exact(version)) => {
                Some(listed_exact(&version, &all).ok_or_else(missing)?.to_owned())
            }
            Some(VersionSpec::Tag(spec) | VersionSpec::Range(spec)) => {
                let releases = releases.into_iter().filter_map(num).collect::<Vec<_>>();
                Some(
                    cargo_resolve(&spec, releases.iter().map(String::as_str))
                        .ok_or_else(missing)?,
                )
            }
        };
        if let Some(entry) = wanted.as_deref().and_then(|wanted| {
            versions
                .iter()
                .find(|entry| entry.get("num").and_then(Value::as_str) == Some(wanted))
        }) {
            apply_crate_version(&mut artifact, entry);
        }
        crate_release_facts(&mut artifact, client).await;
        return Ok(ArtifactProviderPage::single(artifact));
    }
    let page = state.page.unwrap_or(1);
    let size = artifact_search_query::page_size(query);
    let url = keyword_url("https://crates.io/api/v1/crates", query, page, size)?;
    let (data, artifacts) =
        keyword_rows(client, ArtifactType::Crates, url, "crates", crate_item).await?;
    let count = data
        .get("meta")
        .and_then(Value::as_object)
        .and_then(|meta| total(meta.get("total")));
    paged(artifacts, count, page, size, ArtifactType::Crates)
}

/// The listed version an exact request names: itself, or, when crates.io
/// lists it only with build metadata (`0.10.0+wasi-snapshot-preview1` for
/// `0.10.0`), the one listed version of the same precedence, as Cargo's
/// `=0.10.0` matches it. A request that states build metadata names only
/// that string. `None` when no version, or more than one, matches.
fn listed_exact<'a>(requested: &str, listed: &'a [String]) -> Option<&'a str> {
    if let Some(same) = listed.iter().find(|version| *version == requested) {
        return Some(same);
    }
    let wanted = semver::Version::parse(requested)
        .ok()
        .filter(|wanted| wanted.build.is_empty())?;
    let mut matching = listed.iter().filter(|version| {
        semver::Version::parse(version).is_ok_and(|version| version.cmp_precedence(&wanted).is_eq())
    });
    match (matching.next(), matching.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

/// crates.io serves a relevance-sorted keyword search only below this many
/// results (`RELEVANCE_CANDIDATE_LIMIT`; a page starting at or past it is
/// HTTP 400): the shared page bounds end discovery there.
pub(super) const CRATES_RELEVANCE_LIMIT: u64 = 1000;

/// crates.io refuses a page number past this one whatever `per_page` is
/// (HTTP 400 "Page 201 is unavailable for performance reasons", measured
/// 2026-11-24 for every user agent and sort), though its own `next_page`
/// still names page 201: the shared page bounds end small pages here,
/// before the relevance limit.
pub(super) const CRATES_MAX_PAGE: u64 = 200;

#[cfg(test)]
mod tests;
mod vcs;
