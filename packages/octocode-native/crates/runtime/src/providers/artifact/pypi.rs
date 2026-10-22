//! PyPI: exact releases (PEP 440 pins and specifiers, yanked releases), the
//! project's license and requirements, and the upstream release tag.
use super::http::{JSON, RegistryClient};
use super::util::{
    date_prefix, github_slug, object_for, parse_url, repository_url, required, safe_url, string,
};
use super::versions::{VersionSpec, pypi_resolve};
use super::{ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactSearchQuery, ArtifactType};
use crate::providers::artifact::types::artifact_search_query;
use serde_json::Value;

pub(crate) async fn pypi(
    query: &ArtifactSearchQuery,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let Some(package_name) = artifact_search_query::bare_package_name(query) else {
        return Err(ArtifactError::new(
            "capabilityUnavailable",
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
    // A labeled source link first; else the first project link or the
    // homepage that names a GitHub repository (attrs labels it "GitHub",
    // rich and six give only a GitHub homepage).
    artifact.repository = links
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
                .find_map(|value| github_slug(value.as_str()?))
                .map(|(owner, repo)| format!("https://github.com/{owner}/{repo}"))
        });
    artifact.published_at = body
        .get("urls")
        .and_then(Value::as_array)
        .and_then(|files| files.first())
        .and_then(|file| date_prefix(file.get("upload_time_iso_8601")));
    artifact.yanked = (info.get("yanked") == Some(&Value::Bool(true))).then_some(true);
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
    // PyPI names no commit; an upstream tag for this release pins the lead.
    if let Some(version) = artifact.version.clone() {
        let name = artifact.name.clone();
        super::release_ref::pin_release_tag(&mut artifact, &name, &version, client).await;
    }
    Ok(ArtifactProviderPage::single(artifact))
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
        (Some(_), Some(version)) => {
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
                return Err(super::versions::version_not_found(
                    package_name,
                    version,
                    &pypi_releases(&project, true),
                ));
            }
            found
        }
        (Some(specifier), None) => {
            let Some(project_json) = client
                .json(ArtifactType::Pypi, project("")?, true, None, None, JSON)
                .await?
            else {
                return Ok(None);
            };
            let releases = pypi_releases(&project_json, false);
            let Some(resolved) = pypi_resolve(specifier, releases.iter().map(String::as_str))
            else {
                let all = pypi_releases(&project_json, true);
                // Only yanked releases match: name the newest (a pin selects it).
                if let Some(yanked) = pypi_resolve(specifier, all.iter().map(String::as_str)) {
                    let mut error = ArtifactError::new(
                        "versionNotFound",
                        format!(
                            "{package_name} has no unyanked version matching \"{specifier}\"; yanked {yanked} matches"
                        ),
                    )
                    .with_status(404);
                    error.nearest = vec![yanked];
                    return Err(error);
                }
                return Err(super::versions::version_not_found(
                    package_name,
                    specifier,
                    &all,
                ));
            };
            client
                .json(
                    ArtifactType::Pypi,
                    project(&format!("{}/", super::util::encode_component(&resolved)))?,
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
mod tests {
    use super::super::test_http::{KnownTags, RouteHttp};
    use super::super::types::{exact_query, test_budget, versioned};
    use super::*;

    use serde_json::json;

    fn pypi_release(version: &str) -> serde_json::Value {
        json!({
            "info": {"name": "requests", "version": version, "summary": "HTTP",
                "requires_python": ">=3.7", "yanked": false,
                "requires_dist": ["idna<4,>=2.5", "urllib3<3", "PySocks!=1.5.7; extra == \"socks\""],
                "project_urls": {"Source": "https://github.com/psf/requests"}},
            "urls": [{"upload_time_iso_8601": "2023-05-22T15:12:42.313790Z"}]
        })
    }

    fn pypi_project() -> serde_json::Value {
        let mut project = pypi_release("2.32.3");
        project["releases"] = json!({
            "2.30.0": [{"yanked": false}], "2.31.0": [{"yanked": false}],
            "2.32.0": [{"yanked": true}], "2.32.3": [{"yanked": false}], "3.0.0rc1": [{"yanked": false}]
        });
        project
    }

    #[tokio::test]
    async fn pypi_version_lookups_pin_exact_specifier_and_coordinate_forms() {
        let http = RouteHttp::routes(vec![
            ("/pypi/requests/json", pypi_project()),
            ("/pypi/requests/2.31.0/json", pypi_release("2.31.0")),
            ("/pypi/requests/2.32.3/json", pypi_release("2.32.3")),
        ]);
        let tags = KnownTags::new(vec!["psf/requests@v2.31.0"]);
        let b = test_budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache: None,
            tags: Some(&tags),
        };
        let exact = pypi(
            &versioned(ArtifactType::Pypi, "requests", "2.31.0"),
            &client,
        )
        .await
        .expect("exact");
        let item = &exact.artifacts[0];
        assert_eq!(item.version.as_deref(), Some("2.31.0"));
        assert_eq!(item.published_at.as_deref(), Some("2023-05-22"));
        assert_eq!(item.requires_python.as_deref(), Some(">=3.7"));
        assert_eq!(
            item.dependencies.as_ref().map(Vec::len),
            Some(2),
            "extras are not dependencies: {:?}",
            item.dependencies
        );
        assert_eq!(
            item.optional_dependencies,
            Some(vec!["PySocks!=1.5.7; extra == \"socks\"".to_owned()]),
            "extras are optional dependencies"
        );
        assert_eq!(item.yanked, None);
        // The upstream release tag pins the source lead.
        assert_eq!(item.source_ref.as_deref(), Some("v2.31.0"));
        let coordinate = pypi(
            &exact_query(ArtifactType::Pypi, "requests==2.31.0"),
            &client,
        )
        .await
        .expect("== coordinate");
        assert_eq!(coordinate.artifacts[0].version.as_deref(), Some("2.31.0"));
        let range = pypi(
            &versioned(ArtifactType::Pypi, "requests", ">=2.31,<3"),
            &client,
        )
        .await
        .expect("specifier");
        assert_eq!(range.artifacts[0].version.as_deref(), Some("2.32.3"));
        // Neither `v2.32.3` nor `2.32.3` exists upstream: no release ref.
        assert_eq!(range.artifacts[0].source_ref, None);
        let missing = pypi(
            &versioned(ArtifactType::Pypi, "requests", "2.31.9"),
            &client,
        )
        .await
        .expect_err("missing version");
        assert_eq!(missing.code, "versionNotFound");
        assert!(
            missing.nearest.iter().any(|version| version == "2.31.0"),
            "{missing:?}"
        );
        let registry: Vec<String> = http
            .targets()
            .into_iter()
            .filter(|path| path.starts_with("/pypi/"))
            .collect();
        assert_eq!(
            registry[..2],
            ["/pypi/requests/2.31.0/json", "/pypi/requests/2.31.0/json"],
            "exact versions are one registry request"
        );
        assert_eq!(
            tags.seen()[..2],
            ["psf/requests@v2.31.0", "psf/requests@2.31.0"],
            "the plain tags are checked together, and the `v` tag wins"
        );
        assert_eq!(tags.seen()[2], "psf/requests@v2.31.0");
    }

    /// PEP 592: an `==`/`===` pin selects a yanked release; a range that
    /// only yanked releases match says so and offers the newest one.
    #[tokio::test]
    async fn pypi_pins_select_yanked_releases_and_ranges_name_them() {
        let mut yanked = pypi_release("2.32.0");
        yanked["info"]["yanked"] = json!(true);
        let http = RouteHttp::routes(vec![
            ("/pypi/requests/json", pypi_project()),
            ("/pypi/requests/2.32.0/json", yanked),
        ]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        for pin in ["==2.32.0", "===2.32.0", "== 2.32.0"] {
            let page = pypi(&versioned(ArtifactType::Pypi, "requests", pin), &client)
                .await
                .unwrap_or_else(|error| panic!("{pin}: {error:?}"));
            assert_eq!(
                page.artifacts[0].version.as_deref(),
                Some("2.32.0"),
                "{pin}"
            );
            assert_eq!(page.artifacts[0].yanked, Some(true), "{pin}");
        }
        let only_yanked = pypi(
            &versioned(ArtifactType::Pypi, "requests", ">=2.32.0,<2.32.2"),
            &client,
        )
        .await
        .expect_err("only a yanked release matches");
        assert_eq!(only_yanked.code, "versionNotFound");
        assert!(
            only_yanked.message.contains("yanked 2.32.0"),
            "{only_yanked:?}"
        );
        assert_eq!(only_yanked.nearest, ["2.32.0"]);
    }

    /// The row's license is a short identifier: the SPDX expression, a
    /// one-line license field, else the license classifier; never the full
    /// license text some projects put in `license`.
    #[tokio::test]
    async fn pypi_license_is_short_never_the_license_text() {
        let text = format!(
            "Copyright (c) 2005-2024, NumPy Developers.\nAll rights reserved.\n{}",
            "Redistribution and use in source and binary forms. ".repeat(40)
        );
        let cases = [
            (
                json!({"license_expression": "BSD-3-Clause", "license": text}),
                Some("BSD-3-Clause"),
            ),
            (json!({"license": "Apache 2.0"}), Some("Apache 2.0")),
            (
                json!({"license": text, "classifiers": [
                    "Programming Language :: Python",
                    "License :: OSI Approved :: BSD License"
                ]}),
                Some("BSD License"),
            ),
            (json!({"license": text}), None),
        ];
        for (fields, expected) in cases {
            let mut release = pypi_release("2.31.0");
            for (key, value) in fields.as_object().expect("fields") {
                release["info"][key] = value.clone();
            }
            let http = RouteHttp::routes(vec![("/pypi/requests/2.31.0/json", release)]);
            let b = test_budget();
            let client = RegistryClient::uncached(&http, &b);
            let page = pypi(
                &versioned(ArtifactType::Pypi, "requests", "2.31.0"),
                &client,
            )
            .await
            .expect("release");
            assert_eq!(page.artifacts[0].license.as_deref(), expected, "{fields}");
        }
    }

    /// A crate version's source ref is the commit cargo packaged it from,
    /// with its directory in the repository; without VCS info it is the
    /// upstream release tag, when one exists.
    /// A monorepo package whose release tag carries a prefix other than its
    /// full name (pydantic-core: `core-v2.50.1` in pydantic/pydantic) still
    /// pins the release lead to that tag.
    #[tokio::test]
    async fn a_monorepo_pypi_release_is_pinned_to_its_prefixed_tag() {
        let mut release = pypi_release("2.50.1");
        release["info"]["name"] = json!("pydantic_core");
        release["info"]["project_urls"] = json!({
            "Source": "https://github.com/pydantic/pydantic/tree/main/pydantic-core"
        });
        let http = RouteHttp::routes(vec![("/pypi/pydantic-core/2.50.1/json", release)]);
        let tags = KnownTags::new(vec!["pydantic/pydantic@core-v2.50.1"]);
        let b = test_budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache: None,
            tags: Some(&tags),
        };
        let page = pypi(
            &versioned(ArtifactType::Pypi, "pydantic-core", "2.50.1"),
            &client,
        )
        .await
        .expect("exact");
        assert_eq!(
            page.artifacts[0].source_ref.as_deref(),
            Some("core-v2.50.1")
        );
        assert!(page.artifacts[0].source_tag);
    }

    /// Only requirements an extra gates are optional; an environment
    /// marker alone keeps a requirement required.
    #[test]
    fn pypi_requirements_gated_by_an_extra_are_optional() {
        for optional in [
            "PySocks!=1.5.7; extra == \"socks\"",
            "x; extra=='y'",
            "x ; python_version < \"4\" and extra == \"y\"",
        ] {
            assert!(names_an_extra(optional), "{optional}");
        }
        for required in [
            "idna<4,>=2.5",
            "tzdata; sys_platform == \"win32\"",
            "x; my_extra == \"y\"",
        ] {
            assert!(!names_an_extra(required), "{required}");
        }
    }

    /// A found release tag is a release fact: the same release looked up
    /// again checks no tag on GitHub.
    #[tokio::test]
    async fn found_release_tags_are_remembered() {
        let http = RouteHttp::routes(vec![
            ("/pypi/requests/json", pypi_project()),
            ("/pypi/requests/2.31.0/json", pypi_release("2.31.0")),
        ]);
        let tags = KnownTags::new(vec!["psf/requests@v2.31.0"]);
        let cache = super::super::ArtifactCache::new(None);
        let b = test_budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache: Some(&cache),
            tags: Some(&tags),
        };
        let query = versioned(ArtifactType::Pypi, "requests", "2.31.0");
        for _ in 0..2 {
            let page = pypi(&query, &client).await.expect("exact");
            assert_eq!(page.artifacts[0].source_ref.as_deref(), Some("v2.31.0"));
            assert!(page.artifacts[0].source_tag);
        }
        assert_eq!(
            tags.seen(),
            ["psf/requests@v2.31.0", "psf/requests@2.31.0"],
            "one round of tag checks"
        );
    }
}
