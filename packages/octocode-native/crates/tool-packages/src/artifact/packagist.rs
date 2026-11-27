//! Packagist: exact Composer releases and keyword discovery.
mod exact;

use super::http::{JSON, RegistryClient};
use super::util::{
    commit_sha, coordinate_path, date_prefix, keyword_rows, keyword_url, license, object_for,
    parse_url, repository_url, required, safe_url, string, total,
};
use super::{ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState};
use crate::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType};
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

/// A row's `abandoned`: Composer's deprecation of a package, naming the
/// replacement when the maintainer did.
fn abandoned(row: &serde_json::Map<String, Value>) -> Option<String> {
    match row.get("abandoned")? {
        Value::String(replacement) if !replacement.trim().is_empty() => {
            Some(format!("Abandoned; use {} instead.", replacement.trim()))
        }
        Value::String(_) | Value::Bool(true) => Some("Abandoned.".to_owned()),
        _ => None,
    }
}

/// Composer's order: release numbers, then a dev, alpha, beta or RC
/// release before the stable one before a patch release.
pub(super) const ORDER: super::versions::VersionOrder = super::versions::VersionOrder {
    cmp: |left, right| super::versions::cmp_segmented(left, right, |value| !is_stable(value)),
    prerelease: |value| !is_stable(value),
};

/// Composer's `VersionParser::parseStability`: a version is stable unless it is
/// a dev branch or ends in an alpha, beta or RC modifier. `patch`, `pl` and `p`
/// modifiers are stable.
fn is_stable(version: &str) -> bool {
    static MODIFIER: LazyLock<Regex> = LazyLock::new(|| {
        #[expect(clippy::expect_used, reason = "static pattern, valid by construction")]
        Regex::new(r"[._-]?(?:(stable|beta|b|rc|alpha|a|patch|pl|p)((?:[.-]?\d+)*)?)?([.-]?dev)?(?:\+.*)?$")
            .expect("composer stability pattern")
    });
    let version = version.split('#').next().unwrap_or(version);
    if version.starts_with("dev-") || version.ends_with("-dev") {
        return false;
    }
    let version = version.to_ascii_lowercase();
    MODIFIER.captures(&version).is_none_or(|modifier| {
        modifier.get(3).is_none()
            && !matches!(
                modifier.get(1).map(|name| name.as_str()),
                Some("beta" | "b" | "alpha" | "a" | "rc")
            )
    })
}

/// The p2 entry at `index` whole. A `composer/2.0` document is minified: only
/// its first entry is complete, and each later one states what changed from the
/// entry before it, `__unset` removing a field (`MetadataMinifier::expand`).
fn expanded(
    entries: &[Value],
    index: usize,
    minified: bool,
) -> Result<serde_json::Map<String, Value>, ArtifactError> {
    let at = |entry: &Value| object_for(entry, ArtifactType::Packagist).cloned();
    let Some(chosen) = entries.get(index) else {
        return Err(super::util::invalid(ArtifactType::Packagist));
    };
    if !minified {
        return at(chosen);
    }
    let mut row = serde_json::Map::new();
    for entry in &entries[..=index] {
        for (key, value) in at(entry)? {
            if value.as_str() == Some("__unset") {
                row.remove(&key);
            } else {
                row.insert(key, value);
            }
        }
    }
    Ok(row)
}

fn composer(row: &serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let name = required(row.get("name"), ArtifactType::Packagist)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Packagist,
        name.clone(),
        format!("https://packagist.org/packages/{}", coordinate_path(&name)),
    );
    artifact.version = string(row.get("version"));
    artifact.description = string(row.get("description"));
    artifact.license = license(row.get("license"));
    let source = row
        .get("source")
        .and_then(Value::as_object)
        .and_then(|value| repository_url(value.get("url")))
        .or_else(|| repository_url(row.get("repository")));
    if let Some(link) = source {
        artifact.set_source_link(&link);
    }
    artifact.source_ref = row
        .get("source")
        .and_then(Value::as_object)
        .and_then(|source| commit_sha(source.get("reference")));
    artifact.homepage = safe_url(row.get("homepage"));
    artifact.deprecated = abandoned(row);
    Ok(artifact)
}

/// An exact Packagist release, whole ([`expanded`]): its row, the day it was
/// tagged (`time`), and its `require` entries as `name constraint` (platform
/// ones such as `php` included, as declared). A missing `require` declares none.
fn composer_release(row: &serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let mut artifact = composer(row)?;
    artifact.published_at = date_prefix(row.get("time"));
    artifact.dependencies = Some(
        row.get("require")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(name, constraint)| match constraint.as_str() {
                Some(constraint) => format!("{name} {constraint}"),
                None => name.clone(),
            })
            .collect(),
    );
    Ok(artifact)
}

pub(crate) async fn packagist(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(package_name) = artifact_search_query::package_name(query) {
        let name = package_name.to_ascii_lowercase();
        let tagged_url = parse_url(&format!(
            "https://repo.packagist.org/p2/{}.json",
            coordinate_path(&name)
        ))?;
        let mut response = client
            .json(ArtifactType::Packagist, tagged_url, true, None, None, JSON)
            .await?;
        let mut entries = packagist_entries(response.as_ref(), &name)?;
        if entries.is_empty() {
            let dev_url = parse_url(&format!(
                "https://repo.packagist.org/p2/{}~dev.json",
                coordinate_path(&name)
            ))?;
            response = client
                .json(ArtifactType::Packagist, dev_url, true, None, None, JSON)
                .await?;
            entries = packagist_entries(response.as_ref(), &name)?;
        }
        if entries.is_empty() {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        }
        if let Some(wanted) = artifact_search_query::version(query).filter(|v| *v != "latest") {
            let mut page = exact::exact_release(&name, wanted, response.as_ref(), client).await?;
            for artifact in &mut page.artifacts {
                tag_checked(artifact, client).await;
            }
            return Ok(page);
        }
        // What Composer's default `minimum-stability` installs: entries run
        // newest first, and a package with no stable one names its newest.
        let stable = entries.iter().position(|entry| {
            string(entry.get("version")).is_some_and(|version| is_stable(&version))
        });
        let minified = response
            .as_ref()
            .is_some_and(|body| body.get("minified").is_some());
        let row = expanded(entries, stable.unwrap_or(0), minified)?;
        let mut artifact = composer_release(&row)?;
        artifact.only_prerelease = stable.is_none();
        tag_checked(&mut artifact, client).await;
        return Ok(ArtifactProviderPage::single(artifact));
    }
    let page = state.page.unwrap_or(1);
    let size = artifact_search_query::page_size(query);
    let url = keyword_url("https://packagist.org/search.json", query, page, size)?;
    let (data, artifacts) =
        keyword_rows(client, ArtifactType::Packagist, url, "results", composer).await?;
    Ok(ArtifactProviderPage {
        next_state: string(data.get("next")).map(|_| ArtifactProviderState {
            page: Some(page + 1),
            ..Default::default()
        }),
        artifacts,
        total: total(data.get("total")),
        terminal_limit: None,
        registry: None,
        any_keyword_total: None,
    })
}

/// A release whose source names a commit, checked against the version's
/// upstream release tag: the tag naming that commit proves it
/// ([`super::release_ref::check_commit_against_tag`]).
async fn tag_checked(artifact: &mut ArtifactItem, client: &RegistryClient<'_>) {
    if let (Some(commit), Some(version)) = (artifact.source_ref.clone(), artifact.version.clone()) {
        let name = artifact.package_name.clone();
        super::release_ref::check_commit_against_tag(artifact, &name, &version, &commit, client)
            .await;
    }
}

fn packagist_entries<'a>(
    response: Option<&'a Value>,
    name: &str,
) -> Result<&'a [Value], ArtifactError> {
    let Some(response) = response else {
        return Ok(&[]);
    };
    let packages = object_for(response, ArtifactType::Packagist)?
        .get("packages")
        .ok_or_else(|| super::util::invalid(ArtifactType::Packagist))?;
    Ok(packages
        .as_object()
        .and_then(|value| value.get(name))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]))
}

#[cfg(test)]
mod tests;
