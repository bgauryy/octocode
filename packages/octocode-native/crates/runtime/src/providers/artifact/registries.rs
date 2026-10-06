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
    let Some(response) = pypi_release_json(package_name, query.version(), client).await? else {
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
    artifact.dependency_list = info
        .get("requires_dist")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|requirement| !requirement.contains("extra =="))
        .map(str::to_owned)
        .collect();
    artifact.dependencies = Some(artifact.dependency_list.len());
    // PyPI names no commit; an upstream tag for this release pins the lead.
    if let (Some(repository), Some(version)) = (&artifact.repository, &artifact.version) {
        artifact.source_ref =
            super::release_ref::github_release_tag(repository, version, client).await;
        artifact.source_tag = artifact.source_ref.is_some();
    }
    Ok(single(artifact))
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
    // A bare release number is exact; operators make a PEP 440 specifier.
    let requested = version.filter(|value| *value != "latest");
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
                return Ok(None);
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

/// One version's facts on its crate row.
fn apply_crate_version(artifact: &mut ArtifactItem, entry: &Value) {
    artifact.version = string(entry.get("num"));
    artifact.published_at = date_prefix(entry.get("created_at"));
    artifact.yanked = (entry.get("yanked") == Some(&Value::Bool(true))).then_some(true);
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
        client.json(ArtifactType::Crates, metadata, true, None),
        client.json(ArtifactType::Crates, record, true, None)
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
        .filter(|entry| entry.get("num").and_then(Value::as_str) == Some(version))
        .ok_or_else(|| super::util::invalid(ArtifactType::Crates))?;
    let mut artifact = crate_item(row)?;
    apply_crate_version(&mut artifact, entry);
    Ok(Some(artifact))
}

/// The commit a GitHub-hosted crate version was packaged from (cargo's VCS
/// info, with the crate's directory in the repository), else its upstream
/// release tag.
async fn crate_source_ref(artifact: &mut ArtifactItem, client: &RegistryClient<'_>) {
    let (Some(repository), Some(version)) = (artifact.repository.clone(), artifact.version.clone())
    else {
        return;
    };
    if !repository.contains("github.com") {
        return;
    }
    if let Some((sha, directory)) =
        super::release_ref::crate_vcs(&artifact.name, &version, client).await
    {
        artifact.source_ref = Some(sha);
        artifact.repository_directory = directory.filter(|directory| !directory.is_empty());
        return;
    }
    artifact.source_ref =
        super::release_ref::github_release_tag(&repository, &version, client).await;
    artifact.source_tag = artifact.source_ref.is_some();
}

/// The runtime dependency count of a crate version: normal, non-optional
/// dependencies (optional ones are features, like PyPI extras). A failed
/// read leaves the count unset; it never fails the lookup.
async fn crate_dependencies(
    name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<Vec<String>> {
    let url = parse_url(&format!(
        "https://crates.io/api/v1/crates/{}/{}/dependencies",
        super::util::encode_component(name),
        super::util::encode_component(version)
    ))
    .ok()?;
    let listing = client
        .json(ArtifactType::Crates, url, true, None)
        .await
        .ok()??;
    Some(
        listing
            .get("dependencies")?
            .as_array()?
            .iter()
            .filter(|dependency| {
                dependency.get("kind").and_then(Value::as_str) == Some("normal")
                    && dependency.get("optional") != Some(&Value::Bool(true))
            })
            .filter_map(|dependency| {
                let name = dependency.get("crate_id").and_then(Value::as_str)?;
                Some(match dependency.get("req").and_then(Value::as_str) {
                    Some(req) => format!("{name} {req}"),
                    None => name.to_owned(),
                })
            })
            .collect(),
    )
}

/// Release facts of a resolved crate version that need their own reads:
/// the source ref and the dependency count, read concurrently.
async fn crate_release_facts(artifact: &mut ArtifactItem, client: &RegistryClient<'_>) {
    let Some(version) = artifact.version.clone() else {
        return;
    };
    let name = artifact.name.clone();
    let (dependencies, ()) = tokio::join!(
        crate_dependencies(&name, &version, client),
        crate_source_ref(artifact, client)
    );
    artifact.dependencies = dependencies.as_ref().map(Vec::len);
    artifact.dependency_list = dependencies.unwrap_or_default();
}

pub(crate) async fn crates(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.bare_package_name() {
        if let Some(VersionSpec::Exact(version)) = query.version().map(VersionSpec::parse)
            && let Some(mut artifact) = crate_release(name, &version, client).await?
        {
            crate_release_facts(&mut artifact, client).await;
            return Ok(single(artifact));
        }
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
            apply_crate_version(&mut artifact, entry);
        }
        crate_release_facts(&mut artifact, client).await;
        return Ok(single(artifact));
    }
    let page = state.page.unwrap_or(1);
    let size = query.page_size().unwrap_or(10);
    let url = keyword_url("https://crates.io/api/v1/crates", query, page, size)?;
    let (data, artifacts) =
        keyword_rows(client, ArtifactType::Crates, url, "crates", crate_item).await?;
    let count = data
        .get("meta")
        .and_then(Value::as_object)
        .and_then(|meta| total(meta.get("total")));
    paged(artifacts, count, page, size, ArtifactType::Crates)
}

/// An exact Go module or package lookup on pkg.go.dev, pinned to
/// `query.version()` when one is given.
async fn go_exact(
    name: &str,
    query: &ArtifactSearchQuery,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let path = coordinate_path(name);
    let version = go_version(query.version())?;
    let lookup = |kind: &str| {
        endpoint(
            &format!("https://pkg.go.dev/v1/{kind}/{path}"),
            &[("version", version.clone())],
        )
    };
    let mut response = client
        .json(ArtifactType::Go, lookup("module")?, true, None)
        .await?;
    let mut is_package = false;
    if response.is_none() {
        response = client
            .json(ArtifactType::Go, lookup("package")?, true, None)
            .await?;
        is_package = true;
    }
    // A pinned lookup answers with that version or not at all.
    let response = response.filter(|row| {
        version
            .as_deref()
            .is_none_or(|version| row.get("version").and_then(Value::as_str) == Some(version))
    });
    let Some(response) = response else {
        // A pinned version the module does not publish is not an
        // unknown module: name the nearest published versions.
        if let Some(version) = version.as_deref() {
            let versions_url = parse_url(&format!("https://pkg.go.dev/v1/versions/{path}"))?;
            if let Some(listing) = client
                .json(ArtifactType::Go, versions_url, true, None)
                .await?
            {
                let published = listing
                    .get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|item| string(item.get("version")))
                    .map(|published| published.trim_start_matches('v').to_owned())
                    .collect::<Vec<_>>();
                if !published.is_empty() {
                    return Err(super::npm::version_not_found(
                        name,
                        version.trim_start_matches('v'),
                        &published,
                    ));
                }
            }
        }
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
    // A vanity module path (`go.opentelemetry.io/otel`) does not name its
    // repository directory or tag; the module proxy records the commit
    // the version was built from, and the directory inside the repo.
    if artifact.source_ref.is_none()
        && let (Some(module), Some(version)) =
            (artifact.module_path.clone(), artifact.version.clone())
    {
        let info = parse_url(&format!(
            "https://proxy.golang.org/{}/@v/{}.info",
            go_proxy_escape(&module),
            super::util::encode_component(&version)
        ))?;
        // The proxy is a lead source only: its failure leaves the lookup.
        if let Some(origin) = client
            .json(ArtifactType::Go, info, true, None)
            .await
            .ok()
            .flatten()
            .as_ref()
            .and_then(|info| info.get("Origin"))
        {
            artifact.source_ref = commit_sha(origin.get("Hash"));
            if artifact.repository_directory.is_none() {
                artifact.repository_directory =
                    string(origin.get("Subdir")).filter(|dir| !dir.is_empty());
            }
        }
    }
    Ok(single(artifact))
}

pub(crate) async fn go(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.package_name() {
        return go_exact(name, query, client).await;
    }
    // pkg.go.dev pages by opaque token only: page N follows the tokens of
    // the N-1 pages before it, each the same size, so pages tile the list.
    let mut token: Option<String> = None;
    let mut hop = 1;
    let data = loop {
        let url = endpoint(
            "https://pkg.go.dev/v1/search",
            &[
                ("q", Some(terms(query))),
                ("limit", Some(query.page_size().unwrap_or(10).to_string())),
                ("token", token.clone()),
            ],
        )?;
        let response = client
            .json(ArtifactType::Go, url, false, None)
            .await?
            .ok_or_else(|| super::util::invalid(ArtifactType::Go))?;
        let data = object_for(&response, ArtifactType::Go)?.clone();
        if hop >= state.page.unwrap_or(1) {
            break data;
        }
        let Some(next) = string(data.get("nextPageToken")) else {
            return Ok(ArtifactProviderPage::empty(total(data.get("total"))));
        };
        token = Some(next);
        hop += 1;
    };
    let data = &data;
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
        next_state: string(data.get("nextPageToken")).map(|_| ArtifactProviderState {
            page: Some(state.page.unwrap_or(1) + 1),
            ..Default::default()
        }),
        total: total(data.get("total")),
        terminal_limit: None,
        registry: None,
    })
}

/// A module path in the module proxy's case encoding: each upper-case letter
/// becomes `!` plus its lower-case form; segments are URL-encoded.
fn go_proxy_escape(module: &str) -> String {
    let folded = module
        .chars()
        .flat_map(|c| {
            if c.is_ascii_uppercase() {
                vec!['!', c.to_ascii_lowercase()]
            } else {
                vec![c]
            }
        })
        .collect::<String>();
    coordinate_path(&folded)
}

/// The pkg.go.dev `version` of a Go lookup: `None` for the latest release
/// (no version, or `latest`), the `v`-prefixed exact version otherwise.
/// Ranges and other tags need a module version list: not supported.
fn go_version(requested: Option<&str>) -> Result<Option<String>, ArtifactError> {
    match requested.map(VersionSpec::parse) {
        None => Ok(None),
        Some(VersionSpec::Tag(tag)) if tag == "latest" => Ok(None),
        Some(VersionSpec::Exact(version)) => Ok(Some(format!("v{version}"))),
        Some(VersionSpec::Tag(spec) | VersionSpec::Range(spec)) => Err(ArtifactError::new(
            "unsupported_capability",
            format!("Go lookups take an exact version or latest, not \"{spec}\"."),
        )
        .with_hint("Pass an exact module version, e.g. v1.2.3.")),
    }
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
    // rubygems.org ignores per-page sizing (a fixed number of rows per API
    // page), so the item offset picks the API page and the rows within it,
    // and a page that crosses an API page boundary reads the next one.
    let size = query.page_size().unwrap_or(10);
    let offset = state.offset.unwrap_or(0) as usize;
    let mut api_page = 1;
    let mut skip = offset;
    let mut artifacts = Vec::new();
    let mut more = false;
    loop {
        let url = endpoint(
            "https://rubygems.org/api/v1/search.json",
            &[
                ("query", Some(terms(query))),
                ("page", Some(api_page.to_string())),
            ],
        )?;
        let response = client
            .json(ArtifactType::Rubygems, url, false, None)
            .await?
            .ok_or_else(|| super::util::invalid(ArtifactType::Rubygems))?;
        let fetched = rows(&response, ArtifactType::Rubygems)?;
        if fetched.is_empty() {
            break;
        }
        if skip >= fetched.len() {
            skip -= fetched.len();
            api_page += 1;
            continue;
        }
        let wanted = size - artifacts.len();
        for row in fetched.iter().skip(skip).take(wanted) {
            artifacts.push(gem(object_for(row, ArtifactType::Rubygems)?)?);
        }
        skip = 0;
        if artifacts.len() == size {
            // A full page may have more after it; a later empty page ends.
            more = true;
            break;
        }
        api_page += 1;
    }
    let next_state = (more && !artifacts.is_empty()).then(|| ArtifactProviderState {
        offset: Some((offset + artifacts.len()) as u64),
        ..Default::default()
    });
    Ok(ArtifactProviderPage {
        next_state,
        artifacts,
        total: None,
        terminal_limit: None,
        registry: None,
    })
}

/// A `q`/`page`/`per_page` keyword search URL on `base`.
fn keyword_url(
    base: &str,
    query: &ArtifactSearchQuery,
    page: u64,
    size: usize,
) -> Result<url::Url, ArtifactError> {
    endpoint(
        base,
        &[
            ("q", Some(terms(query))),
            ("page", Some(page.to_string())),
            ("per_page", Some(size.to_string())),
        ],
    )
}

/// A keyword search response object and its rows under `key`, each mapped
/// by `item`.
async fn keyword_rows(
    client: &RegistryClient<'_>,
    artifact_type: ArtifactType,
    url: url::Url,
    key: &str,
    item: fn(&serde_json::Map<String, Value>) -> Result<ArtifactItem, ArtifactError>,
) -> Result<(serde_json::Map<String, Value>, Vec<ArtifactItem>), ArtifactError> {
    let invalid = || super::util::invalid(artifact_type);
    let Some(Value::Object(data)) = client.json(artifact_type, url, false, None).await? else {
        return Err(invalid());
    };
    let artifacts = rows(data.get(key).ok_or_else(invalid)?, artifact_type)?
        .iter()
        .map(|row| item(object_for(row, artifact_type)?))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((data, artifacts))
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
    use super::super::types::{StaticHttp, artifact_query, test_budget};
    use super::*;
    use crate::providers::RequestBudget;
    use crate::providers::artifact::http::{
        ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse,
    };
    use serde_json::json;

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
            let full = match req.url.query() {
                Some(query) => format!("{path}?{query}"),
                None => path.clone(),
            };
            self.seen.lock().expect("seen").push(full.clone());
            let found = self
                .routes
                .iter()
                .find(|(route, _)| *route == full)
                .or_else(|| self.routes.iter().find(|(route, _)| *route == path))
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

    /// Upstream tags that exist (`owner/repo@tag`); records every check.
    struct FakeTags {
        known: Vec<&'static str>,
        seen: std::sync::Mutex<Vec<String>>,
    }

    impl FakeTags {
        fn new(known: Vec<&'static str>) -> Self {
            Self {
                known,
                seen: std::sync::Mutex::new(vec![]),
            }
        }
        fn seen(&self) -> Vec<String> {
            self.seen.lock().expect("seen").clone()
        }
    }

    impl super::super::ReleaseTags for FakeTags {
        fn exists<'a>(
            &'a self,
            owner: &'a str,
            repo: &'a str,
            tag: &'a str,
        ) -> super::super::TagFuture<'a> {
            let name = format!("{owner}/{repo}@{tag}");
            let found = self.known.contains(&name.as_str());
            self.seen.lock().expect("seen").push(name);
            Box::pin(async move { Some(found) })
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
        let tags = FakeTags::new(vec!["psf/requests@v2.31.0"]);
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
        assert_eq!(item.dependencies, Some(2), "extras are not dependencies");
        assert_eq!(item.dependency_list.len(), 2, "{:?}", item.dependency_list);
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
        assert!(missing.hints[0].contains("2.31.0"), "{missing:?}");
        let registry: Vec<String> = http
            .seen()
            .into_iter()
            .filter(|path| path.starts_with("/pypi/"))
            .collect();
        assert_eq!(
            registry[..2],
            ["/pypi/requests/2.31.0/json", "/pypi/requests/2.31.0/json"],
            "exact versions are one registry request"
        );
        assert_eq!(
            tags.seen()[0],
            "psf/requests@v2.31.0",
            "the `v` tag is checked first and ends the check"
        );
        assert_eq!(tags.seen()[1], "psf/requests@v2.31.0");
    }

    fn serde_crate_document() -> Value {
        json!({
            "crate": {"name": "serde", "max_stable_version": "1.0.228",
                "description": "serialization", "homepage": "https://serde.rs",
                "repository": "https://github.com/serde-rs/serde"},
            "versions": [
                {"num": "1.0.228", "created_at": "2025-09-27T00:00:00Z", "yanked": false, "license": "MIT OR Apache-2.0", "rust_version": "1.61"},
                {"num": "1.0.200", "created_at": "2024-05-01T00:00:00Z", "yanked": true, "license": "MIT OR Apache-2.0", "rust_version": "1.31"},
                {"num": "1.0.100", "created_at": "2019-09-08T01:56:06Z", "yanked": false, "license": "MIT OR Apache-2.0"},
                {"num": "1.0.99", "created_at": "2019-08-01T00:00:00Z", "yanked": false}
            ]
        })
    }

    /// Serves one `.crate` archive by path; everything else goes to routes.
    struct ArchiveHttp {
        archive_path: &'static str,
        archive: Vec<u8>,
        routes: RouteHttp,
    }

    impl ArtifactHttp for ArchiveHttp {
        fn get<'a>(
            &'a self,
            req: ArtifactHttpRequest,
            budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            if req.url.path() == self.archive_path {
                let body = self.archive.clone();
                return Box::pin(async move { Ok(ArtifactHttpResponse { status: 200, body }) });
            }
            self.routes.get(req, budget)
        }
    }

    /// A crate version's source ref is the commit cargo packaged it from,
    /// with its directory in the repository; without VCS info it is the
    /// upstream release tag, when one exists.
    #[tokio::test]
    async fn crates_exact_versions_carry_the_packaging_commit_or_release_tag() {
        let info =
            br#"{"git":{"sha1":"b6a77c4413f902523646be0d7f5520631df53ff6"},"path_in_vcs":"serde"}"#;
        let http = ArchiveHttp {
            archive_path: "/crates/serde/serde-1.0.100.crate",
            archive: super::super::release_ref::test_archive(&[
                ("serde-1.0.100/Cargo.toml", b"[package]", b'0'),
                ("serde-1.0.100/.cargo_vcs_info.json", info, b'0'),
            ]),
            routes: RouteHttp::new(vec![("/api/v1/crates/serde", serde_crate_document())]),
        };
        let tags = FakeTags::new(vec!["serde-rs/serde@v1.0.228"]);
        let b = test_budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache: None,
            tags: Some(&tags),
        };
        let state = ArtifactProviderState::default();
        let pinned = crates(
            &versioned(ArtifactType::Crates, "serde", "1.0.100"),
            &state,
            &client,
        )
        .await
        .expect("pinned")
        .artifacts
        .remove(0);
        assert_eq!(
            pinned.source_ref.as_deref(),
            Some("b6a77c4413f902523646be0d7f5520631df53ff6")
        );
        assert_eq!(pinned.repository_directory.as_deref(), Some("serde"));
        let latest = crates(&exact_query(ArtifactType::Crates, "serde"), &state, &client)
            .await
            .expect("latest")
            .artifacts
            .remove(0);
        assert_eq!(latest.source_ref.as_deref(), Some("v1.0.228"));
        assert!(latest.source_tag, "an upstream tag is checked to exist");
        assert!(!pinned.source_tag, "cargo VCS info is not a tag");
        assert_eq!(latest.repository_directory, None);
    }

    /// A crate row counts its runtime dependencies like the other
    /// ecosystems: normal and non-optional, on both exact-version paths.
    #[tokio::test]
    async fn crates_rows_count_runtime_dependencies() {
        let dependencies = json!({"dependencies": [
            {"crate_id": "a", "kind": "normal", "optional": false, "req": "^1.0"},
            {"crate_id": "b", "kind": "normal", "optional": true},
            {"crate_id": "c", "kind": "dev", "optional": false},
            {"crate_id": "d", "kind": "build", "optional": false},
            {"crate_id": "e", "kind": "normal", "optional": false}
        ]});
        let document = serde_crate_document();
        let mut metadata = json!({"crate": document["crate"].clone(), "versions": null});
        metadata["crate"]["max_stable_version"] = Value::Null;
        let record = json!({"version": document["versions"][2].clone()});
        let small = RouteHttp::new(vec![
            ("/api/v1/crates/serde?include=", metadata),
            ("/api/v1/crates/serde/1.0.100", record),
            (
                "/api/v1/crates/serde/1.0.100/dependencies",
                dependencies.clone(),
            ),
        ]);
        let whole = RouteHttp::new(vec![
            ("/api/v1/crates/serde", document),
            ("/api/v1/crates/serde/1.0.228/dependencies", dependencies),
        ]);
        let b = test_budget();
        let pinned = crate_lookup(
            &small,
            &b,
            &versioned(ArtifactType::Crates, "serde", "1.0.100"),
        )
        .await
        .expect("pinned")
        .artifacts
        .remove(0);
        assert_eq!(pinned.dependencies, Some(2));
        let latest = crate_lookup(&whole, &b, &exact_query(ArtifactType::Crates, "serde"))
            .await
            .expect("latest")
            .artifacts
            .remove(0);
        assert_eq!(latest.dependencies, Some(2));
        assert_eq!(
            serde_json::to_value(&latest).expect("row")["dependencies"],
            2
        );
        // E19: the names ride the row, not only their count.
        assert_eq!(
            serde_json::to_value(&latest).expect("row")["dependencyList"],
            json!(["a ^1.0", "e"])
        );
    }

    #[tokio::test]
    async fn crates_versions_resolve_from_the_crate_response() {
        let http = RouteHttp::new(vec![("/api/v1/crates/serde", serde_crate_document())]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let state = ArtifactProviderState::default();
        let lookup = |query: ArtifactSearchQuery| {
            let client = &client;
            let state = &state;
            async move { crates(&query, state, client).await }
        };
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
        // The crate document resolves every version; only the resolved
        // version's dependency count is a second read.
        assert!(
            http.seen()
                .iter()
                .filter(|path| path.starts_with("/api/"))
                .all(|path| path == "/api/v1/crates/serde" || path.ends_with("/dependencies"))
        );
        // A version the registry does not know reads the crate document for
        // its nearest versions.
        let missing = lookup(versioned(ArtifactType::Crates, "serde", "1.0.101"))
            .await
            .expect_err("missing");
        assert_eq!(missing.code, "versionNotFound");
        assert!(missing.hints[0].contains("1.0.100"), "{missing:?}");
    }

    async fn crate_lookup(
        http: &RouteHttp,
        budget: &RequestBudget,
        query: &ArtifactSearchQuery,
    ) -> Result<ArtifactProviderPage, ArtifactError> {
        let client = RegistryClient::uncached(http, budget);
        crates(query, &ArtifactProviderState::default(), &client).await
    }

    /// An exact version reads its own record and the crate's metadata, not
    /// the crate document that lists every version, and returns the same
    /// row the document would.
    #[tokio::test]
    async fn crates_exact_versions_read_the_version_record_with_the_same_fields() {
        let document = serde_crate_document();
        let record = |num: &str| {
            json!({"version": document["versions"].as_array().expect("versions").iter()
                .find(|entry| entry["num"] == num).expect("entry")})
        };
        // `include=` leaves the version-derived fields empty, as crates.io does.
        let mut metadata = json!({"crate": document["crate"].clone(), "versions": null});
        metadata["crate"]["max_stable_version"] = Value::Null;
        metadata["crate"]["max_version"] = json!("0.0.0");
        let small = RouteHttp::new(vec![
            ("/api/v1/crates/serde?include=", metadata),
            ("/api/v1/crates/serde/1.0.100", record("1.0.100")),
            ("/api/v1/crates/serde/1.0.200", record("1.0.200")),
        ]);
        let whole = RouteHttp::new(vec![("/api/v1/crates/serde", document.clone())]);
        let b = test_budget();
        for (query, version) in [
            (
                versioned(ArtifactType::Crates, "serde", "1.0.100"),
                "1.0.100",
            ),
            (
                exact_query(ArtifactType::Crates, "serde@1.0.100"),
                "1.0.100",
            ),
            (
                versioned(ArtifactType::Crates, "serde", "1.0.200"),
                "1.0.200",
            ),
        ] {
            let item = crate_lookup(&small, &b, &query)
                .await
                .expect("exact")
                .artifacts
                .remove(0);
            assert_eq!(item.version.as_deref(), Some(version));
            let via_document = crate_lookup(&whole, &b, &query)
                .await
                .expect("document")
                .artifacts
                .remove(0);
            assert_eq!(
                serde_json::to_value(&item).expect("item"),
                serde_json::to_value(&via_document).expect("item"),
                "{version}"
            );
        }
        // Registry reads only (dependency counts aside); the release-ref
        // reads are lead sources.
        let mut seen: Vec<String> = small
            .seen()
            .into_iter()
            .filter(|path| path.starts_with("/api/") && !path.ends_with("/dependencies"))
            .collect();
        seen.sort();
        seen.dedup();
        assert_eq!(
            seen,
            [
                "/api/v1/crates/serde/1.0.100",
                "/api/v1/crates/serde/1.0.200",
                "/api/v1/crates/serde?include=",
            ]
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
        let http = StaticHttp::json(json!({
            "info": {
                "name": "requests",
                "version": "2.31.0",
                "summary": "Python HTTP for Humans.",
                "license": "Apache-2.0",
                "home_page": "https://requests.readthedocs.io",
                "project_urls": {"Source": "https://github.com/psf/requests"}
            }
        }));
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
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
        let http = StaticHttp::json(json!({
            "crate": {
                "id": "serde",
                "name": "serde",
                "max_stable_version": "1.0.200",
                "description": "A generic serialization/deserialization framework",
                "license": "MIT OR Apache-2.0",
                "repository": "https://github.com/serde-rs/serde"
            }
        }));
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
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
        let http = StaticHttp::json(json!({
            "path": "github.com/gin-gonic/gin",
            "version": "v1.9.1",
            "synopsis": "HTTP web framework for Go"
        }));
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
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

    /// A pinned Go module version is looked up as that version, so its
    /// release ref names the pinned tag, not the latest one; an unpublished
    /// version names the nearest published ones, and ranges are refused.
    #[tokio::test]
    async fn go_pinned_versions_resolve_the_pinned_release() {
        let module = "/v1/module/github.com/open-telemetry/opentelemetry-go";
        let http = RouteHttp::new(vec![
            (
                "/v1/module/github.com/open-telemetry/opentelemetry-go?version=v0.71.0",
                json!({"path":"github.com/open-telemetry/opentelemetry-go","version":"v0.71.0",
                    "repoUrl":"https://github.com/open-telemetry/opentelemetry-go"}),
            ),
            (
                module,
                json!({"path":"github.com/open-telemetry/opentelemetry-go","version":"v0.72.0",
                    "repoUrl":"https://github.com/open-telemetry/opentelemetry-go"}),
            ),
            (
                "/v1/versions/github.com/open-telemetry/opentelemetry-go",
                json!({"items":[{"version":"v0.72.0"},{"version":"v0.71.0"},{"version":"v0.70.0"}]}),
            ),
        ]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let lookup = |version: Option<&str>| {
            let mut fields =
                json!({"type":"go","packageName":"github.com/open-telemetry/opentelemetry-go"});
            if let Some(version) = version {
                fields["version"] = json!(version);
            }
            artifact_query(fields, None)
        };
        let state = ArtifactProviderState::default();
        for pinned in ["v0.71.0", "0.71.0"] {
            let page = go(&lookup(Some(pinned)), &state, &client)
                .await
                .expect("pinned");
            let item = &page.artifacts[0];
            assert_eq!(item.version.as_deref(), Some("v0.71.0"));
            assert_eq!(item.source_ref.as_deref(), Some("v0.71.0"));
        }
        let latest = go(&lookup(Some("latest")), &state, &client)
            .await
            .expect("latest");
        assert_eq!(latest.artifacts[0].version.as_deref(), Some("v0.72.0"));
        let missing = go(&lookup(Some("v0.69.9")), &state, &client)
            .await
            .expect_err("unpublished");
        assert_eq!(missing.code, "versionNotFound");
        assert!(missing.hints[0].contains("0.70.0"), "{missing:?}");
        let range = go(&lookup(Some("^0.71")), &state, &client)
            .await
            .expect_err("range");
        assert_eq!(range.code, "unsupported_capability");
    }

    /// A vanity module path names neither its repository directory nor its
    /// tag: the release commit and directory come from the module proxy.
    #[tokio::test]
    async fn go_vanity_modules_take_the_release_commit_from_the_proxy() {
        let http = RouteHttp::new(vec![
            (
                "/v1/module/go.opentelemetry.io/otel/sdk?version=v1.30.0",
                json!({"path":"go.opentelemetry.io/otel/sdk","version":"v1.30.0",
                    "repoUrl":"https://github.com/open-telemetry/opentelemetry-go"}),
            ),
            (
                "/go.opentelemetry.io/otel/sdk/@v/v1.30.0.info",
                json!({"Version":"v1.30.0","Origin":{"VCS":"git",
                    "URL":"https://github.com/open-telemetry/opentelemetry-go","Subdir":"sdk",
                    "Hash":"ed4fc757583a88b4da51b1fe1c3f0703ac27a487","Ref":"refs/tags/sdk/v1.30.0"}}),
            ),
        ]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let query = artifact_query(
            json!({"type":"go","packageName":"go.opentelemetry.io/otel/sdk","version":"v1.30.0"}),
            None,
        );
        let page = go(&query, &ArtifactProviderState::default(), &client)
            .await
            .expect("vanity");
        let item = &page.artifacts[0];
        assert_eq!(
            item.source_ref.as_deref(),
            Some("ed4fc757583a88b4da51b1fe1c3f0703ac27a487")
        );
        assert_eq!(item.repository_directory.as_deref(), Some("sdk"));
        assert_eq!(
            go_proxy_escape("github.com/BurntSushi/toml"),
            "github.com/!burnt!sushi/toml"
        );
    }

    #[tokio::test]
    async fn packagist_parses_exact_lookup() {
        // p2/{name}.json response with entries for the tagged URL
        let http = StaticHttp::json(json!({
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
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
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
        let http = StaticHttp::json(json!({
            "name": "rails",
            "version": "7.0.6",
            "info": "Full-stack web application framework.",
            "homepage_uri": "https://rubyonrails.org",
            "source_code_uri": "https://github.com/rails/rails"
        }));
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
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
