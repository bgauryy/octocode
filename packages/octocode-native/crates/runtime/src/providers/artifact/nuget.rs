use super::http::{JSON, RegistryClient};
use super::util::{
    commit_sha, date_prefix, endpoint, object_for, parse_url, required, rows, string, total,
};
use super::{ArtifactError, ArtifactProviderPage, ArtifactProviderState, TerminalLimit};
use crate::providers::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use serde_json::{Map, Value};
use std::cmp::Ordering;

mod entries;
mod service;

use entries::{catalog, compare_versions, item};
use service::{official_url, service_endpoint};

pub(crate) async fn nuget(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = artifact_search_query::package_name(query) {
        return exact(
            name,
            requested_version(artifact_search_query::version(query))?,
            client,
        )
        .await;
    }
    // The shared page bounds ([`super::page_out_of_range`]) keep `offset`
    // below [`REACHABLE_END`] and end the last reachable page.
    let offset = state.offset.unwrap_or(0);
    let base = service_endpoint("SearchQueryService", client).await?;
    let size =
        artifact_search_query::page_size(query).min(REACHABLE_END.saturating_sub(offset) as usize);
    // NuGet rejects skip > 3000. Past it, fetch one window from skip=3000
    // (take stays <= 1000) and slice the saved page window out locally.
    let (skip, drop) = if offset > MAX_SKIP {
        (MAX_SKIP, (offset - MAX_SKIP) as usize)
    } else {
        (offset, 0)
    };
    let take = drop + size;
    let url = endpoint(
        base.as_str(),
        &[
            ("q", Some(artifact_search_query::terms(query))),
            ("skip", Some(skip.to_string())),
            ("take", Some(take.to_string())),
            ("prerelease", Some("true".into())),
            ("semVerLevel", Some("2.0.0".into())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Nuget, url, false, None, None, JSON)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?;
    let data = object_for(&response, ArtifactType::Nuget)?;
    let fetched = rows(
        data.get("data")
            .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?,
        ArtifactType::Nuget,
    )?;
    let artifacts = fetched
        .iter()
        .skip(drop)
        .take(size)
        .map(|value| item(object_for(value, ArtifactType::Nuget)?))
        .collect::<Result<Vec<_>, _>>()?;
    let count = total(data.get("totalHits"));
    let next_offset = offset + artifacts.len() as u64;
    let more = count
        .map(|count| next_offset < count)
        .unwrap_or(fetched.len() >= take);
    let limit = (more && artifacts.is_empty()).then(|| TerminalLimit::short_page("NuGet"));
    Ok(ArtifactProviderPage {
        next_state: (more && limit.is_none()).then(|| ArtifactProviderState {
            offset: Some(next_offset),
            ..Default::default()
        }),
        artifacts,
        total: count,
        terminal_limit: limit,
        registry: None,
    })
}

/// NuGet search accepts `skip <= 3000` and `take <= 1000`, so result 4000 is
/// the last one any request can reach.
const MAX_SKIP: u64 = 3000;
pub(super) const REACHABLE_END: u64 = MAX_SKIP + 1000;

/// The exact NuGet version a lookup asks for: `None` for the latest release
/// (no version, or `latest`). NuGet versions are numeric (up to four parts)
/// with an optional prerelease label; ranges are not resolved.
fn requested_version(version: Option<&str>) -> Result<Option<&str>, ArtifactError> {
    let Some(version) = version
        .map(str::trim)
        .filter(|version| !version.eq_ignore_ascii_case("latest"))
    else {
        return Ok(None);
    };
    let version = version.trim_start_matches(['v', 'V']);
    if compare_versions(version, version).is_err() {
        return Err(ArtifactError::new(
            ErrorCode::CapabilityUnavailable,
            format!("NuGet lookups take an exact version or latest, not \"{version}\"."),
        ));
    }
    Ok(Some(version))
}

/// A registration page's leaves: inline, or from the page document it names.
async fn page_items(
    page: &Map<String, Value>,
    client: &RegistryClient<'_>,
) -> Result<Vec<Value>, ArtifactError> {
    if let Some(items) = page.get("items") {
        return rows(items, ArtifactType::Nuget).cloned();
    }
    let advertised = official_url(page.get("@id"))?;
    let owned_page = client
        .json(ArtifactType::Nuget, advertised, false, None, None, JSON)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?;
    rows(
        object_for(&owned_page, ArtifactType::Nuget)?
            .get("items")
            .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?,
        ArtifactType::Nuget,
    )
    .cloned()
}

async fn exact(
    package_name: &str,
    version: Option<&str>,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let base = service_endpoint("RegistrationsBaseUrl", client).await?;
    let url = parse_url(&format!(
        "{}/{}/index.json",
        base.as_str().trim_end_matches('/'),
        super::util::encode_component(&package_name.to_ascii_lowercase())
    ))?;
    let Some(response) = client
        .json(ArtifactType::Nuget, url, true, None, None, JSON)
        .await?
    else {
        return Ok(ArtifactProviderPage::empty(Some(0)));
    };
    let pages = rows(
        object_for(&response, ArtifactType::Nuget)?
            .get("items")
            .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?,
        ArtifactType::Nuget,
    )?;
    if pages.is_empty() {
        return Ok(ArtifactProviderPage::empty(Some(0)));
    }
    let mut pages = pages
        .iter()
        .map(|page| object_for(page, ArtifactType::Nuget))
        .collect::<Result<Vec<_>, _>>()?;
    let bound = |page: &Map<String, Value>, edge: &str| {
        required(page.get(edge), ArtifactType::Nuget).unwrap_or_default()
    };
    pages.sort_by(|a, b| {
        compare_versions(&bound(b, "upper"), &bound(a, "upper")).unwrap_or(Ordering::Equal)
    });
    let release = match version {
        Some(wanted) => {
            // An exact version: the registration page whose bounds hold it.
            let mut seen = Vec::new();
            let mut found = None;
            for page in &pages {
                let holds = compare_versions(&bound(page, "lower"), wanted)
                    .is_ok_and(|order| order != Ordering::Greater)
                    && compare_versions(wanted, &bound(page, "upper"))
                        .is_ok_and(|order| order != Ordering::Greater);
                if !holds {
                    continue;
                }
                for leaf in page_items(page, client).await? {
                    let candidate = catalog(&leaf)?;
                    let listed = required(candidate.get("version"), ArtifactType::Nuget)?;
                    if compare_versions(&listed, wanted)? == Ordering::Equal {
                        found = Some(candidate.clone());
                        break;
                    }
                    seen.push(listed);
                }
                if found.is_some() {
                    break;
                }
            }
            match found {
                Some(found) => found,
                None => {
                    if seen.is_empty() {
                        seen = pages
                            .iter()
                            .flat_map(|page| [bound(page, "lower"), bound(page, "upper")])
                            .filter(|edge| !edge.is_empty())
                            .collect();
                    }
                    return Err(super::versions::version_not_found(
                        package_name,
                        wanted,
                        &seen,
                    ));
                }
            }
        }
        None => latest(&pages, client).await?,
    };
    let mut artifact = item(&release)?;
    artifact.published_at =
        date_prefix(release.get("published")).filter(|day| !day.starts_with(UNLISTED_YEAR));
    artifact.dependencies = Some(declared_dependencies(&release));
    // The registration catalogEntry usually omits `repository`; the package's
    // own nuspec declares it.
    if artifact.repository.is_none()
        && let Some(version) = artifact.version.clone()
        && let Some((repository, commit)) = nuspec_repository(package_name, &version, client).await
    {
        artifact.repository = Some(repository);
        artifact.source_ref = commit;
    }
    Ok(ArtifactProviderPage::single(artifact))
}

/// The `published` year NuGet gives an unlisted release: no release day.
const UNLISTED_YEAR: &str = "1900-";

/// A catalog entry's dependencies as `Id range`, each once, in declared
/// order. One that only some target frameworks declare names them
/// (`System.ValueTuple [4.5.0, ) (.NETFramework4.6.2)`); a group without
/// a framework applies to all. No group, or only empty ones, declares none.
fn declared_dependencies(entry: &Map<String, Value>) -> Vec<String> {
    let groups = entry
        .get("dependencyGroups")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    // Each dependency, with the frameworks declaring it (`None`: all).
    let mut declared: Vec<(String, Option<Vec<String>>)> = Vec::new();
    for group in groups {
        let framework = string(group.get("targetFramework"));
        let dependencies = group.get("dependencies").and_then(Value::as_array);
        for dependency in dependencies.into_iter().flatten() {
            let Some(id) = string(dependency.get("id")) else {
                continue;
            };
            let name = match string(dependency.get("range")) {
                Some(range) => format!("{id} {range}"),
                None => id,
            };
            let index = declared
                .iter()
                .position(|(known, _)| *known == name)
                .unwrap_or_else(|| {
                    declared.push((name, Some(Vec::new())));
                    declared.len() - 1
                });
            let frameworks = &mut declared[index].1;
            match (&framework, frameworks.as_mut()) {
                (Some(framework), Some(list)) => list.push(framework.clone()),
                (None, _) => *frameworks = None,
                (Some(_), None) => {}
            }
        }
    }
    declared
        .into_iter()
        .map(|(name, frameworks)| match frameworks {
            Some(list) if list.len() < groups.len() => format!("{name} ({})", list.join(", ")),
            _ => name,
        })
        .collect()
}

/// Newest registration pages first. The highest listed stable version wins;
/// a prerelease only when the package has no stable release (as NuGet clients
/// resolve "latest").
async fn latest(
    pages: &[&Map<String, Value>],
    client: &RegistryClient<'_>,
) -> Result<Map<String, Value>, ArtifactError> {
    let mut stable: Option<Map<String, Value>> = None;
    let mut prerelease: Option<Map<String, Value>> = None;
    for page in pages.iter().take(MAX_REGISTRATION_PAGES) {
        for leaf in page_items(page, client).await? {
            let candidate = catalog(&leaf)?;
            if candidate.get("listed") == Some(&Value::Bool(false)) {
                continue;
            }
            let version = required(candidate.get("version"), ArtifactType::Nuget)?;
            let slot = if version.contains('-') {
                &mut prerelease
            } else {
                &mut stable
            };
            let newer = match slot.as_ref() {
                None => true,
                Some(current) => {
                    let current = required(current.get("version"), ArtifactType::Nuget)?;
                    compare_versions(&version, &current)? == Ordering::Greater
                }
            };
            if newer {
                *slot = Some(candidate.clone());
            }
        }
        if stable.is_some() {
            break;
        }
    }
    stable
        .or(prerelease)
        .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))
}

/// Registration pages read (newest first) while looking for a stable release.
const MAX_REGISTRATION_PAGES: usize = 4;

/// `<repository url="…" commit="…">` from the package's nuspec in the flat
/// container: the source repository and the commit the package was built from.
async fn nuspec_repository(
    package_name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<(String, Option<String>)> {
    let base = service_endpoint("PackageBaseAddress", client).await.ok()?;
    let id = super::util::encode_component(&package_name.to_ascii_lowercase());
    let url = parse_url(&format!(
        "{}/{id}/{}/{id}.nuspec",
        base.as_str().trim_end_matches('/'),
        super::util::encode_component(&version.to_ascii_lowercase())
    ))
    .ok()?;
    let nuspec = client
        .text(ArtifactType::Nuget, url, true)
        .await
        .ok()
        .flatten()?;
    let tag = nuspec.split("<repository").nth(1)?.split('>').next()?;
    let attribute = |name: &str| {
        tag.split(&format!(" {name}=\""))
            .nth(1)?
            .split('"')
            .next()
            .map(str::to_owned)
    };
    let url = super::util::canonical_repository(&attribute("url")?)?;
    Some((
        url,
        commit_sha(attribute("commit").map(Value::String).as_ref()),
    ))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod paging_tests;
