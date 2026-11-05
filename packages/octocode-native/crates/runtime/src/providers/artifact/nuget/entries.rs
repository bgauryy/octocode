//! NuGet catalog rows: the artifact a catalog entry describes and NuGet version order.
use super::super::util::{object_for, required, safe_url, string};
use super::super::{Advisory, ArtifactError, ArtifactItem};
use octocode_contract_types::ArtifactType;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use url::Url;

pub(super) fn catalog(value: &Value) -> Result<&Map<String, Value>, ArtifactError> {
    object_for(value, ArtifactType::Nuget)?
        .get("catalogEntry")
        .ok_or_else(|| super::super::util::invalid(ArtifactType::Nuget))
        .and_then(|value| object_for(value, ArtifactType::Nuget))
}

pub(super) fn item(row: &Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let name = required(row.get("id"), ArtifactType::Nuget)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Nuget,
        name.clone(),
        format!(
            "https://www.nuget.org/packages/{}",
            super::super::util::encode_component(&name)
        ),
    );
    artifact.version = string(row.get("version"));
    artifact.description = string(row.get("description"));
    artifact.homepage = safe_url(row.get("projectUrl"));
    artifact.license = string(row.get("licenseExpression"));
    // NuGet serializes `repository` as either an object with `url` or a plain
    // (often empty) string, and the registration API's inline catalogEntry
    // usually omits it entirely even when the nuspec carries one. projectUrl
    // is only a repository when it names a repo on a source host; any other
    // projectUrl stays a homepage.
    artifact.repository = repository_url(row.get("repository"))
        .or_else(|| artifact.homepage.clone().filter(|url| is_source_repo(url)));
    Ok(artifact)
}

/// A search row: its `version` is the newest of everything, since a search asks
/// with prerelease results on so a prerelease-only package stays findable. The
/// row's own list, oldest to newest, names the newest stable one when any is.
pub(super) fn search_item(row: &Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let mut artifact = item(row)?;
    let stable = row
        .get("versions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .rev()
        .filter_map(|entry| string(entry.get("version")))
        .find(|version| !is_prerelease(version));
    if stable.is_some() {
        artifact.version = stable;
    }
    Ok(artifact)
}

/// Whether a NuGet version carries a prerelease label.
pub(super) fn is_prerelease(version: &str) -> bool {
    version.split('+').next().unwrap_or(version).contains('-')
}

/// What a catalog entry states about a version's safety: its advisories (NuGet
/// omits an empty list, so none listed is `[]`) and its deprecation.
pub(super) fn safety_facts(artifact: &mut ArtifactItem, entry: &Map<String, Value>) {
    artifact.advisories = Some(
        entry
            .get("vulnerabilities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|row| Advisory {
                url: string(row.get("advisoryUrl")),
                severity: string(row.get("severity")).map(severity_name),
                ..Advisory::default()
            })
            .collect(),
    );
    artifact.deprecated = entry.get("deprecation").and_then(deprecation);
}

/// A vulnerability's `severity` as NuGet documents its codes; an unknown code
/// stays as stated.
fn severity_name(code: String) -> String {
    match code.as_str() {
        "0" => "low".to_owned(),
        "1" => "moderate".to_owned(),
        "2" => "high".to_owned(),
        "3" => "critical".to_owned(),
        _ => code,
    }
}

/// A `deprecation` as one message: the maintainer's words, else its reasons,
/// and the package it names instead.
fn deprecation(entry: &Value) -> Option<String> {
    let reasons = entry
        .get("reasons")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let text = string(entry.get("message")).unwrap_or(reasons);
    let alternate = entry
        .get("alternatePackage")
        .and_then(|package| string(package.get("id")));
    match (text.is_empty(), alternate) {
        (false, Some(alternate)) => Some(format!("{text} Alternate package: {alternate}.")),
        (true, Some(alternate)) => Some(format!("Alternate package: {alternate}.")),
        (false, None) => Some(text),
        (true, None) => Some("Deprecated.".to_owned()),
    }
}

/// `https://<source host>/<owner>/<repo>…` on a known code host.
fn is_source_repo(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    matches!(
        host,
        "github.com" | "gitlab.com" | "bitbucket.org" | "codeberg.org"
    ) && url
        .path_segments()
        .is_some_and(|segments| segments.filter(|part| !part.is_empty()).count() >= 2)
}

pub(super) fn repository_url(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Object(row) => super::super::util::repository_url(row.get("url")),
        value => super::super::util::repository_url(Some(value)),
    }
}

pub(super) fn compare_versions(left: &str, right: &str) -> Result<Ordering, ArtifactError> {
    let parse = |value: &str| -> Result<(Vec<u64>, Option<Vec<String>>), ArtifactError> {
        let (core, prerelease) = value
            .split_once('-')
            .map_or((value, None), |(a, b)| (a, Some(b)));
        let core = core.split('+').next().unwrap_or(core);
        let values = core
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| super::super::util::invalid(ArtifactType::Nuget))?;
        if values.is_empty() || values.len() > 4 {
            return Err(super::super::util::invalid(ArtifactType::Nuget));
        }
        Ok((
            values,
            prerelease.map(|value| {
                value
                    .split('+')
                    .next()
                    .unwrap_or(value)
                    .to_ascii_lowercase()
                    .split('.')
                    .map(str::to_owned)
                    .collect()
            }),
        ))
    };
    let (a, ap) = parse(left)?;
    let (b, bp) = parse(right)?;
    for index in 0..4 {
        let ordering = a.get(index).unwrap_or(&0).cmp(b.get(index).unwrap_or(&0));
        if ordering != Ordering::Equal {
            return Ok(ordering);
        }
    }
    match (ap, bp) {
        (None, None) => Ok(Ordering::Equal),
        (None, Some(_)) => Ok(Ordering::Greater),
        (Some(_), None) => Ok(Ordering::Less),
        (Some(a), Some(b)) => {
            for index in 0..a.len().max(b.len()) {
                let Some(x) = a.get(index) else {
                    return Ok(Ordering::Less);
                };
                let Some(y) = b.get(index) else {
                    return Ok(Ordering::Greater);
                };
                if x == y {
                    continue;
                }
                let ordering = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                return Ok(ordering);
            }
            Ok(Ordering::Equal)
        }
    }
}
