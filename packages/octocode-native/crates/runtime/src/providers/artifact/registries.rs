use super::http::RegistryClient;
use super::util::{
    commit_sha, coordinate_path, date_prefix, endpoint, license, object_for, parse_url, required,
    rows, safe_url, string, total,
};
use super::versions::{VersionSpec, cargo_resolve, pypi_resolve};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType,
};
use serde_json::Value;

pub(crate) async fn pypi(
    query: &ArtifactSearchQuery,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let Some(package_name) = query.bare_package_name() else {
        return Err(ArtifactError::new(
            "unsupported_capability",
            "PyPI does not provide keyword search. Use type:\"pypi\" with an exact packageName.",
        )
        .with_hint("Use type:pypi with packageName for exact Python package lookup."));
    };
    let project = |suffix: &str| {
        parse_url(&format!(
            "https://pypi.org/pypi/{}/{suffix}json",
            super::util::encode_component(package_name)
        ))
    };
    // A bare release number is exact; operators make a PEP 440 specifier.
    let requested = query.version().filter(|value| *value != "latest");
    let exact = requested.filter(|value| {
        !value.contains(['<', '>', '=', '!', '~', ',', '*'])
            && !matches!(VersionSpec::parse(value), VersionSpec::Tag(_))
    });
    let response = match (requested, exact) {
        (None, _) => {
            client
                .json(ArtifactType::Pypi, project("")?, true, None)
                .await?
        }
        (Some(_), Some(version)) => {
            let found = client
                .json(
                    ArtifactType::Pypi,
                    project(&format!("{}/", super::util::encode_component(version)))?,
                    true,
                    None,
                )
                .await?;
            if found.is_none()
                && let Some(project) = client
                    .json(ArtifactType::Pypi, project("")?, true, None)
                    .await?
            {
                return Err(super::npm::version_not_found(
                    package_name,
                    version,
                    &pypi_releases(&project, true),
                ));
            }
            found
        }
        (Some(specifier), None) => {
            let Some(project_json) = client
                .json(ArtifactType::Pypi, project("")?, true, None)
                .await?
            else {
                return Ok(ArtifactProviderPage::empty(Some(0)));
            };
            let releases = pypi_releases(&project_json, false);
            let Some(resolved) = pypi_resolve(specifier, releases.iter().map(String::as_str))
            else {
                return Err(super::npm::version_not_found(
                    package_name,
                    specifier,
                    &pypi_releases(&project_json, true),
                ));
            };
            client
                .json(
                    ArtifactType::Pypi,
                    project(&format!("{}/", super::util::encode_component(&resolved)))?,
                    true,
                    None,
                )
                .await?
        }
    };
    let Some(response) = response else {
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
    artifact.license =
        string(info.get("license_expression")).or_else(|| string(info.get("license")));
    let links = info.get("project_urls").and_then(Value::as_object);
    artifact.homepage = safe_url(info.get("home_page"))
        .or_else(|| links.and_then(|value| safe_url(value.get("Homepage"))));
    artifact.repository = links.and_then(|links| {
        links.iter().find_map(|(label, value)| {
            matches!(
                label.to_ascii_lowercase().as_str(),
                "source" | "source code" | "repository" | "code"
            )
            .then(|| safe_url(Some(value)))
            .flatten()
        })
    });
    artifact.published_at = body
        .get("urls")
        .and_then(Value::as_array)
        .and_then(|files| files.first())
        .and_then(|file| date_prefix(file.get("upload_time_iso_8601")));
    artifact.yanked = (info.get("yanked") == Some(&Value::Bool(true))).then_some(true);
    artifact.requires_python = string(info.get("requires_python"));
    artifact.dependencies = Some(info.get("requires_dist").and_then(Value::as_array).map_or(
        0,
        |requirements| {
            requirements
                .iter()
                .filter_map(Value::as_str)
                .filter(|requirement| !requirement.contains("extra =="))
                .count()
        },
    ));
    Ok(single(artifact))
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
    artifact.repository = safe_url(row.get("repository"));
    Ok(artifact)
}

pub(crate) async fn crates(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.bare_package_name() {
        let url = parse_url(&format!(
            "https://crates.io/api/v1/crates/{}",
            super::util::encode_component(name)
        ))?;
        let Some(response) = client.json(ArtifactType::Crates, url, true, None).await? else {
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
        let wanted = match query.version().map(VersionSpec::parse) {
            None => artifact.version.clone(),
            Some(VersionSpec::Tag(tag)) if tag == "latest" => artifact.version.clone(),
            Some(VersionSpec::Exact(version)) => {
                if !all.contains(&version) {
                    return Err(super::npm::version_not_found(
                        &artifact.name,
                        &version,
                        &all,
                    ));
                }
                Some(version)
            }
            Some(VersionSpec::Tag(spec) | VersionSpec::Range(spec)) => {
                let releases = versions
                    .iter()
                    .filter(|entry| entry.get("yanked") != Some(&Value::Bool(true)))
                    .filter_map(num)
                    .collect::<Vec<_>>();
                Some(
                    cargo_resolve(&spec, releases.iter().map(String::as_str)).ok_or_else(|| {
                        super::npm::version_not_found(&artifact.name, &spec, &all)
                    })?,
                )
            }
        };
        if let Some(entry) = wanted.as_deref().and_then(|wanted| {
            versions
                .iter()
                .find(|entry| entry.get("num").and_then(Value::as_str) == Some(wanted))
        }) {
            artifact.version = num(entry);
            artifact.published_at = date_prefix(entry.get("created_at"));
            artifact.yanked = (entry.get("yanked") == Some(&Value::Bool(true))).then_some(true);
            artifact.rust_version = string(entry.get("rust_version"));
            artifact.license = license(entry.get("license")).or(artifact.license);
        }
        return Ok(single(artifact));
    }
    let page = state.page.unwrap_or(1);
    let size = query.page_size().unwrap_or(10);
    let url = endpoint(
        "https://crates.io/api/v1/crates",
        &[
            ("q", Some(terms(query))),
            ("page", Some(page.to_string())),
            ("per_page", Some(size.to_string())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Crates, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?;
    let data = object_for(&response, ArtifactType::Crates)?;
    let artifacts = rows(
        data.get("crates")
            .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?,
        ArtifactType::Crates,
    )?
    .iter()
    .map(|row| crate_item(object_for(row, ArtifactType::Crates)?))
    .collect::<Result<Vec<_>, _>>()?;
    let count = data
        .get("meta")
        .and_then(Value::as_object)
        .and_then(|meta| total(meta.get("total")));
    paged(artifacts, count, page, size, ArtifactType::Crates)
}

pub(crate) async fn go(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.package_name() {
        let path = coordinate_path(name);
        let module_url = parse_url(&format!("https://pkg.go.dev/v1/module/{path}"))?;
        let mut response = client
            .json(ArtifactType::Go, module_url, true, None)
            .await?;
        let mut is_package = false;
        if response.is_none() {
            let package_url = parse_url(&format!("https://pkg.go.dev/v1/package/{path}"))?;
            response = client
                .json(ArtifactType::Go, package_url, true, None)
                .await?;
            is_package = true;
        }
        let Some(response) = response else {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        };
        let row = object_for(&response, ArtifactType::Go)?;
        let returned = required(row.get("path"), ArtifactType::Go)?;
        let mut artifact = ArtifactItem::new(
            ArtifactType::Go,
            returned.clone(),
            format!("https://pkg.go.dev/{}", coordinate_path(&returned)),
        );
        artifact.version = string(row.get("version"));
        artifact.description = string(row.get("synopsis"));
        artifact.repository = safe_url(row.get("repoUrl"));
        artifact.module_path = if is_package {
            string(row.get("modulePath"))
        } else {
            Some(returned.clone())
        };
        artifact.package_path = is_package.then_some(returned);
        artifact.source_ref = go_source_ref(&artifact);
        return Ok(single(artifact));
    }
    let url = endpoint(
        "https://pkg.go.dev/v1/search",
        &[
            ("q", Some(terms(query))),
            ("limit", Some(query.page_size().unwrap_or(10).to_string())),
            ("token", state.token.clone()),
        ],
    )?;
    let response = client
        .json(ArtifactType::Go, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Go))?;
    let data = object_for(&response, ArtifactType::Go)?;
    let artifacts = rows(
        data.get("items")
            .ok_or_else(|| super::util::invalid(ArtifactType::Go))?,
        ArtifactType::Go,
    )?
    .iter()
    .map(|value| {
        let row = object_for(value, ArtifactType::Go)?;
        let name = required(row.get("packagePath"), ArtifactType::Go)?;
        let mut artifact = ArtifactItem::new(
            ArtifactType::Go,
            name.clone(),
            format!("https://pkg.go.dev/{}", coordinate_path(&name)),
        );
        artifact.module_path = string(row.get("modulePath"));
        artifact.package_path = Some(name);
        artifact.version = string(row.get("version"));
        artifact.description = string(row.get("synopsis"));
        Ok(artifact)
    })
    .collect::<Result<Vec<_>, ArtifactError>>()?;
    Ok(ArtifactProviderPage {
        artifacts,
        next_state: string(data.get("nextPageToken")).map(|token| ArtifactProviderState {
            token: Some(token),
            ..Default::default()
        }),
        total: total(data.get("total")),
        terminal_limit: None,
        registry: None,
    })
}

/// The VCS ref a Go module version names: a pseudo-version's commit, or the
/// release tag (prefixed by the module's directory inside its repository;
/// a major-version suffix is never part of that prefix).
fn go_source_ref(artifact: &ArtifactItem) -> Option<String> {
    let version = artifact.version.as_deref()?;
    let version = version.strip_suffix("+incompatible").unwrap_or(version);
    // vX.Y.Z-yyyymmddhhmmss-abcdefabcdef (also with a .0./-0. base).
    if let Some(commit) = version
        .rsplit_once('-')
        .map(|(_, tail)| tail)
        .filter(|tail| tail.len() == 12 && version.matches('-').count() >= 2)
        .and_then(|tail| commit_sha(Some(&Value::String(tail.to_owned()))))
    {
        return Some(commit);
    }
    let module = artifact.module_path.as_deref()?;
    let repository = artifact.repository.as_deref()?;
    let root = repository
        .split_once("://")
        .map_or(repository, |(_, rest)| rest)
        .trim_end_matches('/')
        .trim_end_matches(".git");
    let dir = module.strip_prefix(root)?;
    if !(dir.is_empty() || dir.starts_with('/')) {
        return None;
    }
    let mut segments: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    if segments.last().is_some_and(|last| {
        last.strip_prefix('v')
            .is_some_and(|major| !major.is_empty() && major.bytes().all(|b| b.is_ascii_digit()))
    }) {
        segments.pop();
    }
    segments.push(version);
    Some(segments.join("/"))
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
    artifact.repository = row
        .get("source")
        .and_then(Value::as_object)
        .and_then(|value| safe_url(value.get("url")))
        .or_else(|| safe_url(row.get("repository")));
    artifact.source_ref = row
        .get("source")
        .and_then(Value::as_object)
        .and_then(|source| commit_sha(source.get("reference")));
    artifact.homepage = safe_url(row.get("homepage"));
    Ok(artifact)
}

pub(crate) async fn packagist(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(package_name) = query.package_name() {
        let name = package_name.to_ascii_lowercase();
        let tagged_url = parse_url(&format!(
            "https://repo.packagist.org/p2/{}.json",
            coordinate_path(&name)
        ))?;
        let mut response = client
            .json(ArtifactType::Packagist, tagged_url, true, None)
            .await?;
        let mut entries = packagist_entries(response.as_ref(), &name)?;
        if entries.is_empty() {
            let dev_url = parse_url(&format!(
                "https://repo.packagist.org/p2/{}~dev.json",
                coordinate_path(&name)
            ))?;
            response = client
                .json(ArtifactType::Packagist, dev_url, true, None)
                .await?;
            entries = packagist_entries(response.as_ref(), &name)?;
        }
        return if let Some(first) = entries.first() {
            Ok(single(composer(object_for(
                first,
                ArtifactType::Packagist,
            )?)?))
        } else {
            Ok(ArtifactProviderPage::empty(Some(0)))
        };
    }
    let page = state.page.unwrap_or(1);
    let size = query.page_size().unwrap_or(10);
    let url = endpoint(
        "https://packagist.org/search.json",
        &[
            ("q", Some(terms(query))),
            ("page", Some(page.to_string())),
            ("per_page", Some(size.to_string())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Packagist, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Packagist))?;
    let data = object_for(&response, ArtifactType::Packagist)?;
    let artifacts = rows(
        data.get("results")
            .ok_or_else(|| super::util::invalid(ArtifactType::Packagist))?,
        ArtifactType::Packagist,
    )?
    .iter()
    .map(|row| composer(object_for(row, ArtifactType::Packagist)?))
    .collect::<Result<Vec<_>, _>>()?;
    Ok(ArtifactProviderPage {
        next_state: string(data.get("next")).map(|_| ArtifactProviderState {
            page: Some(page + 1),
            ..Default::default()
        }),
        artifacts,
        total: total(data.get("total")),
        terminal_limit: None,
        registry: None,
    })
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

fn gem(row: &serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let name = required(row.get("name"), ArtifactType::Rubygems)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Rubygems,
        name.clone(),
        format!(
            "https://rubygems.org/gems/{}",
            super::util::encode_component(&name)
        ),
    );
    artifact.version = string(row.get("version"));
    artifact.description = string(row.get("info"));
    artifact.license = license(row.get("licenses"));
    artifact.homepage = safe_url(row.get("homepage_uri"));
    artifact.repository = safe_url(row.get("source_code_uri")).or_else(|| {
        row.get("metadata")
            .and_then(Value::as_object)
            .and_then(|value| safe_url(value.get("source_code_uri")))
    });
    Ok(artifact)
}

pub(crate) async fn rubygems(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.package_name() {
        let url = parse_url(&format!(
            "https://rubygems.org/api/v1/gems/{}.json",
            super::util::encode_component(name)
        ))?;
        let Some(response) = client.json(ArtifactType::Rubygems, url, true, None).await? else {
            return Ok(ArtifactProviderPage::empty(Some(0)));
        };
        return Ok(single(gem(object_for(&response, ArtifactType::Rubygems)?)?));
    }
    let page = state.page.unwrap_or(1);
    let url = endpoint(
        "https://rubygems.org/api/v1/search.json",
        &[
            ("query", Some(terms(query))),
            ("page", Some(page.to_string())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Rubygems, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Rubygems))?;
    let fetched = rows(&response, ArtifactType::Rubygems)?;
    // rubygems.org ignores per-page sizing (fixed ~30 rows per API page), so
    // honor pageSize by windowing within the fetched page via the cursor
    // offset and advancing to the next API page once it is drained.
    let skip = state.offset.unwrap_or(0) as usize;
    let size = query.page_size().unwrap_or(10);
    let artifacts = fetched
        .iter()
        .skip(skip)
        .take(size)
        .map(|row| gem(object_for(row, ArtifactType::Rubygems)?))
        .collect::<Result<Vec<_>, _>>()?;
    let consumed = skip + artifacts.len();
    let next_state = if artifacts.is_empty() {
        None
    } else if consumed < fetched.len() {
        Some(ArtifactProviderState {
            page: Some(page),
            offset: Some(consumed as u64),
            ..Default::default()
        })
    } else {
        Some(ArtifactProviderState {
            page: Some(page + 1),
            ..Default::default()
        })
    };
    Ok(ArtifactProviderPage {
        next_state,
        artifacts,
        total: None,
        terminal_limit: None,
        registry: None,
    })
}

fn single(artifact: ArtifactItem) -> ArtifactProviderPage {
    ArtifactProviderPage {
        artifacts: vec![artifact],
        next_state: None,
        total: Some(1),
        terminal_limit: None,
        registry: None,
    }
}

fn terms(query: &ArtifactSearchQuery) -> String {
    query.terms()
}

fn paged(
    artifacts: Vec<ArtifactItem>,
    total: Option<u64>,
    page: u64,
    size: usize,
    artifact_type: ArtifactType,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let more = total
        .map(|count| page.saturating_mul(size as u64) < count)
        .unwrap_or(artifacts.len() == size);
    let terminal_limit = (more && artifacts.is_empty()).then(|| {
        format!(
            "{} returned an empty page before its reported total.",
            if artifact_type == ArtifactType::Crates {
                "crates.io"
            } else {
                artifact_type.as_str()
            }
        )
    });
    Ok(ArtifactProviderPage {
        next_state: (more && !artifacts.is_empty()).then(|| ArtifactProviderState {
            page: Some(page + 1),
            ..Default::default()
        }),
        artifacts,
        total,
        terminal_limit,
        registry: None,
    })
}

#[cfg(test)]
mod tests {
    use super::super::types::artifact_query;
    use super::*;
    use crate::providers::RequestBudget;
    use crate::providers::artifact::http::{
        ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse,
    };
    use serde_json::json;
    use std::time::Duration;

    struct MockHttp {
        status: u16,
        body: Vec<u8>,
    }

    impl MockHttp {
        fn ok(body: serde_json::Value) -> Self {
            Self {
                status: 200,
                body: serde_json::to_vec(&body).expect("registry test data should serialize"),
            }
        }
    }

    impl ArtifactHttp for MockHttp {
        fn get<'a>(
            &'a self,
            _req: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            let body = self.body.clone();
            let status = self.status;
            Box::pin(async move { Ok(ArtifactHttpResponse { status, body }) })
        }
    }

    fn budget() -> RequestBudget {
        RequestBudget::with_timeout(Duration::from_secs(10), 10_000_000)
    }

    /// Answers by URL path (exact match); anything else is a 404. Records
    /// every requested path.
    struct RouteHttp {
        routes: Vec<(&'static str, serde_json::Value)>,
        seen: std::sync::Mutex<Vec<String>>,
    }

    impl RouteHttp {
        fn new(routes: Vec<(&'static str, serde_json::Value)>) -> Self {
            Self {
                routes,
                seen: std::sync::Mutex::new(vec![]),
            }
        }
        fn seen(&self) -> Vec<String> {
            self.seen.lock().expect("seen").clone()
        }
    }

    impl ArtifactHttp for RouteHttp {
        fn get<'a>(
            &'a self,
            req: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            let path = req.url.path().to_owned();
            self.seen.lock().expect("seen").push(path.clone());
            let found = self
                .routes
                .iter()
                .find(|(route, _)| *route == path)
                .map(|(_, body)| serde_json::to_vec(body).expect("body"));
            Box::pin(async move {
                Ok(match found {
                    Some(body) => ArtifactHttpResponse { status: 200, body },
                    None => ArtifactHttpResponse {
                        status: 404,
                        body: b"{}".to_vec(),
                    },
                })
            })
        }
    }

    fn versioned(artifact_type: ArtifactType, name: &str, version: &str) -> ArtifactSearchQuery {
        artifact_query(
            json!({"type": artifact_type, "packageName": name, "version": version}),
            None,
        )
    }

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
        let http = RouteHttp::new(vec![
            ("/pypi/requests/json", pypi_project()),
            ("/pypi/requests/2.31.0/json", pypi_release("2.31.0")),
            ("/pypi/requests/2.32.3/json", pypi_release("2.32.3")),
        ]);
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
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
        assert_eq!(item.dependencies, Some(2), "extras are not dependencies");
        assert_eq!(item.yanked, None);
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
        let missing = pypi(
            &versioned(ArtifactType::Pypi, "requests", "2.31.9"),
            &client,
        )
        .await
        .expect_err("missing version");
        assert_eq!(missing.code, "versionNotFound");
        assert!(missing.hints[0].contains("2.31.0"), "{missing:?}");
        assert_eq!(
            http.seen()[..2],
            ["/pypi/requests/2.31.0/json", "/pypi/requests/2.31.0/json"],
            "exact versions are one request"
        );
    }

    #[tokio::test]
    async fn crates_versions_resolve_from_the_crate_response() {
        let http = RouteHttp::new(vec![(
            "/api/v1/crates/serde",
            json!({
                "crate": {"name": "serde", "max_stable_version": "1.0.228",
                    "description": "serialization", "repository": "https://github.com/serde-rs/serde"},
                "versions": [
                    {"num": "1.0.228", "created_at": "2025-09-27T00:00:00Z", "yanked": false, "license": "MIT OR Apache-2.0", "rust_version": "1.61"},
                    {"num": "1.0.200", "created_at": "2024-05-01T00:00:00Z", "yanked": true, "license": "MIT OR Apache-2.0"},
                    {"num": "1.0.100", "created_at": "2019-09-08T01:56:06Z", "yanked": false, "license": "MIT OR Apache-2.0"},
                    {"num": "1.0.99", "created_at": "2019-08-01T00:00:00Z", "yanked": false}
                ]
            }),
        )]);
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let state = ArtifactProviderState::default();
        let lookup = |query: ArtifactSearchQuery| {
            let client = &client;
            let state = &state;
            async move { crates(&query, state, client).await }
        };
        let exact = lookup(versioned(ArtifactType::Crates, "serde", "1.0.100"))
            .await
            .expect("exact");
        let item = &exact.artifacts[0];
        assert_eq!(item.version.as_deref(), Some("1.0.100"));
        assert_eq!(item.published_at.as_deref(), Some("2019-09-08"));
        let coordinate = lookup(exact_query(ArtifactType::Crates, "serde@1.0.100"))
            .await
            .expect("@");
        assert_eq!(coordinate.artifacts[0].version.as_deref(), Some("1.0.100"));
        let latest = lookup(exact_query(ArtifactType::Crates, "serde"))
            .await
            .expect("latest");
        assert_eq!(latest.artifacts[0].rust_version.as_deref(), Some("1.61"));
        let range = lookup(versioned(
            ArtifactType::Crates,
            "serde",
            ">=1.0.100, <1.0.228",
        ))
        .await
        .expect("range");
        assert_eq!(
            range.artifacts[0].version.as_deref(),
            Some("1.0.100"),
            "yanked 1.0.200 is skipped"
        );
        let missing = lookup(versioned(ArtifactType::Crates, "serde", "1.0.101"))
            .await
            .expect_err("missing");
        assert_eq!(missing.code, "versionNotFound");
        assert!(
            http.seen()
                .iter()
                .all(|path| path == "/api/v1/crates/serde")
        );
    }

    fn exact_query(artifact_type: ArtifactType, name: &str) -> ArtifactSearchQuery {
        artifact_query(
            serde_json::json!({"type": artifact_type, "packageName": name.to_string()}),
            None,
        )
    }

    #[test]
    fn go_source_ref_names_the_tag_or_pseudo_version_commit() {
        let item = |module: &str, version: &str| {
            let mut item = ArtifactItem::new(ArtifactType::Go, module.into(), String::new());
            item.module_path = Some(module.into());
            item.version = Some(version.into());
            item.repository = Some("https://github.com/o/r".into());
            go_source_ref(&item)
        };
        assert_eq!(
            item("github.com/o/r", "v1.10.2").as_deref(),
            Some("v1.10.2")
        );
        assert_eq!(
            item("github.com/o/r/v2", "v2.3.0").as_deref(),
            Some("v2.3.0")
        );
        assert_eq!(
            item("github.com/o/r/sub/mod", "v0.4.1").as_deref(),
            Some("sub/mod/v0.4.1")
        );
        assert_eq!(
            item("github.com/o/r", "v2.0.0+incompatible").as_deref(),
            Some("v2.0.0")
        );
        assert_eq!(
            item("github.com/o/r", "v0.0.0-20240101120000-abcdef123456").as_deref(),
            Some("abcdef123456")
        );
        assert_eq!(item("github.com/o/rx", "v1.0.0"), None);
    }

    #[test]
    fn composer_source_reference_is_the_release_commit() {
        let row = json!({"name": "o/r", "version": "v1.0.0",
            "source": {"type": "git", "url": "https://github.com/o/r.git",
                       "reference": "0a2e291c0d9c0c7675d445703e51750363a549ef"}});
        let item = composer(row.as_object().unwrap()).unwrap();
        assert_eq!(
            item.source_ref.as_deref(),
            Some("0a2e291c0d9c0c7675d445703e51750363a549ef")
        );
    }

    #[tokio::test]
    async fn pypi_parses_exact_lookup() {
        let http = MockHttp::ok(json!({
            "info": {
                "name": "requests",
                "version": "2.31.0",
                "summary": "Python HTTP for Humans.",
                "license": "Apache-2.0",
                "home_page": "https://requests.readthedocs.io",
                "project_urls": {"Source": "https://github.com/psf/requests"}
            }
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let q = exact_query(ArtifactType::Pypi, "requests");
        let page = pypi(&q, &client).await.expect("pypi");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "requests");
        assert_eq!(item.version.as_deref(), Some("2.31.0"));
        assert!(
            item.registry_url.contains("pypi.org"),
            "{}",
            item.registry_url
        );
    }

    #[tokio::test]
    async fn crates_parses_exact_lookup() {
        let http = MockHttp::ok(json!({
            "crate": {
                "id": "serde",
                "name": "serde",
                "max_stable_version": "1.0.200",
                "description": "A generic serialization/deserialization framework",
                "license": "MIT OR Apache-2.0",
                "repository": "https://github.com/serde-rs/serde"
            }
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let q = exact_query(ArtifactType::Crates, "serde");
        let page = crates(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("crates");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "serde");
        assert_eq!(item.version.as_deref(), Some("1.0.200"));
        assert!(
            item.registry_url.contains("crates.io"),
            "{}",
            item.registry_url
        );
    }

    #[tokio::test]
    async fn go_parses_exact_module_lookup() {
        let http = MockHttp::ok(json!({
            "path": "github.com/gin-gonic/gin",
            "version": "v1.9.1",
            "synopsis": "HTTP web framework for Go"
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let q = exact_query(ArtifactType::Go, "github.com/gin-gonic/gin");
        let page = go(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("go");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "github.com/gin-gonic/gin");
        assert_eq!(item.version.as_deref(), Some("v1.9.1"));
        assert!(
            item.registry_url.contains("pkg.go.dev"),
            "{}",
            item.registry_url
        );
    }

    #[tokio::test]
    async fn packagist_parses_exact_lookup() {
        // p2/{name}.json response with entries for the tagged URL
        let http = MockHttp::ok(json!({
            "packages": {
                "laravel/framework": [
                    {
                        "name": "laravel/framework",
                        "version": "10.0.0",
                        "description": "The Laravel Framework.",
                        "license": ["MIT"]
                    }
                ]
            }
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let q = exact_query(ArtifactType::Packagist, "laravel/framework");
        let page = packagist(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("packagist");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "laravel/framework");
        assert_eq!(item.version.as_deref(), Some("10.0.0"));
        assert!(
            item.registry_url.contains("packagist.org"),
            "{}",
            item.registry_url
        );
    }

    #[tokio::test]
    async fn rubygems_parses_exact_lookup() {
        let http = MockHttp::ok(json!({
            "name": "rails",
            "version": "7.0.6",
            "info": "Full-stack web application framework.",
            "homepage_uri": "https://rubyonrails.org",
            "source_code_uri": "https://github.com/rails/rails"
        }));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let q = exact_query(ArtifactType::Rubygems, "rails");
        let page = rubygems(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("rubygems");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "rails");
        assert_eq!(item.version.as_deref(), Some("7.0.6"));
        assert!(
            item.registry_url.contains("rubygems.org"),
            "{}",
            item.registry_url
        );
    }
}
