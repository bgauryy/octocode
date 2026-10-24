//! NuGet catalog rows: the artifact a catalog entry describes and NuGet version order.
use super::super::util::{object_for, required, safe_url, string};
use super::super::{ArtifactError, ArtifactItem};
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
