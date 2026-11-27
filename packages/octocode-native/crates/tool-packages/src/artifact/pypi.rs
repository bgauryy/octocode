//! PyPI: exact releases (PEP 440 pins and specifiers, yanked releases), the
//! project's license and requirements, and the upstream release tag.
use super::http::{JSON, RegistryClient};
use super::util::{
    date_prefix, github_slug, object_for, parse_url, repository_url, required, safe_url, string,
};
use super::versions::VersionSpec;
use super::{Advisory, ArtifactError, ArtifactItem, ArtifactProviderPage};
use crate::artifact::query::artifact_search_query;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use pep440::Specifiers;
use serde_json::Value;

mod pep440;

/// PEP 440 ordering (`2.34.0.dev1` before `2.34.0`).
pub(super) const ORDER: super::versions::VersionOrder = super::versions::VersionOrder {
    cmp: |left, right| Some(pep440::Version::parse(left)?.cmp(&pep440::Version::parse(right)?)),
    prerelease: |value| {
        pep440::Version::parse(value).is_some_and(|version| version.is_prerelease())
    },
};

pub(crate) async fn pypi(
    query: &ArtifactSearchQuery,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let Some(package_name) = artifact_search_query::package_name(query) else {
        return Err(ArtifactError::new(
            ErrorCode::CapabilityUnavailable,
            "PyPI has no keyword search; look up an exact packageName.",
        ));
    };
    let Some(response) =
        pypi_release_json(package_name, artifact_search_query::version(query), client).await?
    else {
        return Ok(ArtifactProviderPage::empty(Some(0)));
    };
    let body = object_for(&response, ArtifactType::Pypi)?;
    let info = object_for(
        body.get("info")
            .ok_or_else(|| super::util::invalid(ArtifactType::Pypi))?,
        ArtifactType::Pypi,
    )?;
    let name = required(info.get("name"), ArtifactType::Pypi)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Pypi,
        name.clone(),
        format!(
            "https://pypi.org/project/{}/",
            super::util::encode_component(&name)
        ),
    );
    artifact.version = string(info.get("version"));
    artifact.description = string(info.get("summary"));
    artifact.license = pypi_license(info);
    let links = info.get("project_urls").and_then(Value::as_object);
    artifact.homepage = safe_url(info.get("home_page"))
        .or_else(|| links.and_then(|value| safe_url(value.get("Homepage"))));
    if let Some(link) = source_link(info) {
        artifact.set_source_link(&link);
    }
    artifact.published_at = body
        .get("urls")
        .and_then(Value::as_array)
        .and_then(|files| files.first())
        .and_then(|file| date_prefix(file.get("upload_time_iso_8601")));
    artifact.yanked = (info.get("yanked") == Some(&Value::Bool(true))).then_some(true);
    artifact.yanked_reason = artifact.yanked.and(string(info.get("yanked_reason")));
    // PyPI states these for the release this document describes; `[]` lists none.
    // PyPI lists an advisory once per database (GHSA and PYSEC): one row each.
    artifact.advisories = body
        .get("vulnerabilities")
        .and_then(Value::as_array)
        .map(|rows| Advisory::merged(rows.iter().map(advisory).collect()));
    artifact.requires_python = string(info.get("requires_python"));
    // A requirement gated by an extra is optional, not required.
    let (optional, required): (Vec<String>, Vec<String>) = info
        .get("requires_dist")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .partition(|requirement| names_an_extra(requirement));
    artifact.dependencies = Some(required);
    artifact.optional_dependencies = (!optional.is_empty()).then_some(optional);
    // A release whose metadata names no repository (older releases often
    // name only a homepage) takes the one its project's latest release names.
    let latest = match artifact_search_query::version(query) {
        Some(_) if artifact.repository.is_none() => latest_source(&mut artifact, client).await,
        _ => None,
    };
    // PyPI names no commit; an upstream tag for this release pins the lead.
    if let Some(version) = artifact.version.clone() {
        let name = artifact.package_name.clone();
        super::release_ref::pin_release_tag(&mut artifact, &name, &version, None, client).await;
    }
    if let Some(latest) = latest {
        super::link_directory::place_at_pinned_ref(&mut artifact, PYPI_MANIFESTS, client).await;
        let package = match artifact.version.as_deref() {
            Some(version) => format!("{} {version}", artifact.package_name),
            None => artifact.package_name.clone(),
        };
        artifact.disclosures.push(format!(
            "{package}: its own metadata names no source repository; the repository is the one its latest release, {latest}, names."
        ));
    }
    Ok(ArtifactProviderPage::single(artifact))
}

/// The files a Python project's directory is built from, in any release.
const PYPI_MANIFESTS: &[&str] = &["pyproject.toml", "setup.py", "setup.cfg"];

/// The source link of a PyPI release's `info`: a labeled source link first;
/// else the first project link or the homepage that names a GitHub
/// repository (attrs labels it "GitHub", rich and six give only a GitHub
/// homepage).
fn source_link(info: &serde_json::Map<String, Value>) -> Option<String> {
    let links = info.get("project_urls").and_then(Value::as_object);
    links
        .and_then(|links| {
            links.iter().find_map(|(label, value)| {
                matches!(
                    label.to_ascii_lowercase().as_str(),
                    "source"
                        | "source code"
                        | "sourcecode"
                        | "repository"
                        | "repo"
                        | "code"
                        | "github"
                        | "gitlab"
                )
                .then(|| repository_url(Some(value)))
                .flatten()
            })
        })
        .or_else(|| {
            links
                .into_iter()
                .flat_map(|links| links.values())
                .chain(info.get("home_page"))
                .filter_map(Value::as_str)
                .find(|link| github_slug(link).is_some())
                .map(str::to_owned)
        })
}

/// Sets `artifact`'s source to the one the project's latest release names,
/// when that release is another one and names one; the latest version, when
/// it did. A failed read leaves the row as its own metadata left it.
async fn latest_source(artifact: &mut ArtifactItem, client: &RegistryClient<'_>) -> Option<String> {
    let url = parse_url(&format!(
        "https://pypi.org/pypi/{}/json",
        super::util::encode_component(&artifact.package_name)
    ))
    .ok()?;
    let project = client
        .json(ArtifactType::Pypi, url, true, None, None, JSON)
        .await
        .ok()??;
    let info = project.get("info").and_then(Value::as_object)?;
    let latest = string(info.get("version"))?;
    if artifact.version.as_deref() == Some(latest.as_str()) {
        return None;
    }
    artifact.set_source_link(&source_link(info)?);
    Some(latest)
}

/// One entry of a PyPI release's `vulnerabilities`: its ids, link and fixes
/// (the advisory text is `securityCheck`'s).
fn advisory(row: &Value) -> Advisory {
    let strings = |key: &str| {
        row.get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|value| string(Some(value)))
            .collect()
    };
    Advisory {
        id: string(row.get("id")),
        aliases: strings("aliases"),
        url: string(row.get("link")),
        severity: None,
        fixed_in: strings("fixed_in"),
        withdrawn: string(row.get("withdrawn")),
    }
}

/// Whether a PEP 508 requirement applies only with an extra
/// (`PySocks; extra == "socks"`, `x ; python_version<"4" and extra=='y'`):
/// an optional dependency, not a required one.
fn names_an_extra(requirement: &str) -> bool {
    let Some((_, marker)) = requirement.split_once(';') else {
        return false;
    };
    marker.match_indices("extra").any(|(at, word)| {
        let standalone = marker[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !(before.is_ascii_alphanumeric() || before == '_'));
        standalone && marker[at + word.len()..].trim_start().starts_with("==")
    })
}

/// A PyPI release's license as a short name: the SPDX `license_expression`,
/// else a one-line `license` field, else the last `License :: …`
/// classifier's name. Many projects put the full license text in `license`
/// (numpy: 46 KB); that is no license name and is never the row's license.
fn pypi_license(info: &serde_json::Map<String, Value>) -> Option<String> {
    const SHORT: usize = 100;
    string(info.get("license_expression"))
        .or_else(|| {
            string(info.get("license"))
                .map(|license| license.trim().to_owned())
                .filter(|license| {
                    !license.is_empty() && license.len() < SHORT && !license.contains('\n')
                })
        })
        .or_else(|| {
            info.get("classifiers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter_map(|classifier| classifier.strip_prefix("License :: "))
                .filter_map(|path| path.rsplit(" :: ").next())
                .map(str::trim)
                .rfind(|name| !name.is_empty())
                .map(str::to_owned)
        })
}

/// The release a PEP 440 `==`/`===` pin names (no wildcard, one clause):
/// such a pin selects a yanked release too (PEP 592), so it is an exact
/// lookup, not a specifier pip resolves.
fn pypi_pin(specifier: &str) -> Option<&str> {
    let specifier = specifier.trim();
    let pinned = specifier
        .strip_prefix("===")
        .or_else(|| specifier.strip_prefix("=="))?
        .trim();
    (!pinned.is_empty() && !pinned.contains(['<', '>', '=', '!', '~', ',', '*', ' ']))
        .then_some(pinned)
}

/// The PyPI release document for `version` (none: the latest release; a
/// bare number is exact; operators make a PEP 440 specifier), or `None`
/// when the project or the resolved release is unknown.
async fn pypi_release_json(
    package_name: &str,
    version: Option<&str>,
    client: &RegistryClient<'_>,
) -> Result<Option<Value>, ArtifactError> {
    let project = |suffix: &str| {
        parse_url(&format!(
            "https://pypi.org/pypi/{}/{suffix}json",
            super::util::encode_component(package_name)
        ))
    };
    // A bare release number or an `==`/`===` pin is exact; other operators
    // make a PEP 440 specifier.
    let requested = version.filter(|value| *value != "latest");
    let exact = requested.and_then(|value| {
        pypi_pin(value).or_else(|| {
            (!value.contains(['<', '>', '=', '!', '~', ',', '*'])
                && !matches!(VersionSpec::parse(value), VersionSpec::Tag(_)))
            .then_some(value)
        })
    });
    let response = match (requested, exact) {
        (None, _) => {
            client
                .json(ArtifactType::Pypi, project("")?, true, None, None, JSON)
                .await?
        }
        (Some(asked), Some(version)) => {
            let found = client
                .json(
                    ArtifactType::Pypi,
                    project(&format!("{}/", super::util::encode_component(version)))?,
                    true,
                    None,
                    None,
                    JSON,
                )
                .await?;
            if found.is_none()
                && let Some(project) = client
                    .json(ArtifactType::Pypi, project("")?, true, None, None, JSON)
                    .await?
            {
                return Err(pypi_version_not_found(&project, package_name, asked));
            }
            found
        }
        (Some(specifier), None) => {
            // A malformed specifier is the caller's to fix; it costs no request.
            let specifiers = Specifiers::parse(specifier).map_err(|clause| {
                ArtifactError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "version \"{specifier}\" is not a PEP 440 specifier (\"{clause}\"); write a release (2.31.0) or clauses such as >=2.31,<3, ~=2.31 or ==2.*"
                    ),
                )
            })?;
            let Some(project_json) = client
                .json(ArtifactType::Pypi, project("")?, true, None, None, JSON)
                .await?
            else {
                return Ok(None);
            };
            let releases = pypi_releases(&project_json, false);
            let Some(resolved) = specifiers.resolve(releases.iter().map(String::as_str)) else {
                let all = pypi_releases(&project_json, true);
                // Only yanked releases match: name the newest (a pin selects it).
                if let Some(yanked) = specifiers.resolve(all.iter().map(String::as_str)) {
                    let mut error = ArtifactError::version_not_found(format!(
                        "{package_name} has no unyanked version matching \"{specifier}\"; yanked {yanked} matches"
                    ));
                    error.nearest = vec![yanked.to_owned()];
                    error.yanked = error.nearest.clone();
                    return Err(error);
                }
                return Err(pypi_version_not_found(
                    &project_json,
                    package_name,
                    specifier,
                ));
            };
            client
                .json(
                    ArtifactType::Pypi,
                    project(&format!("{}/", super::util::encode_component(resolved)))?,
                    true,
                    None,
                    None,
                    JSON,
                )
                .await?
        }
    };
    Ok(response)
}

/// A project without the `requested` version: its releases nearest it, and
/// which of them are yanked (every file yanked: no release to move to).
fn pypi_version_not_found(project: &Value, name: &str, requested: &str) -> ArtifactError {
    let live = pypi_releases(project, false);
    let all = pypi_releases(project, true);
    let yanked = all
        .iter()
        .filter(|release| !live.contains(release))
        .cloned()
        .collect::<Vec<_>>();
    super::versions::discouraged_last(
        ArtifactError {
            yanked: yanked.clone(),
            ..super::versions::version_not_found(ArtifactType::Pypi, name, requested, &all)
        },
        |version| yanked.iter().any(|known| known == version),
    )
}

/// Release numbers of a PyPI project; `include_yanked` false drops releases
/// whose every file is yanked (pip skips them for specifiers).
fn pypi_releases(project: &Value, include_yanked: bool) -> Vec<String> {
    project
        .get("releases")
        .and_then(Value::as_object)
        .map(|releases| {
            releases
                .iter()
                .filter(|(_, files)| {
                    include_yanked
                        || files.as_array().is_some_and(|files| {
                            files.is_empty()
                                || files
                                    .iter()
                                    .any(|file| file.get("yanked") != Some(&Value::Bool(true)))
                        })
                })
                .map(|(version, _)| version.clone())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod specifier_tests;
#[cfg(test)]
mod tests;
