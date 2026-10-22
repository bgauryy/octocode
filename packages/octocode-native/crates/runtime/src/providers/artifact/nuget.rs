use super::http::{JSON, RegistryClient};
use super::util::{
    commit_sha, date_prefix, endpoint, object_for, parse_url, required, rows, safe_url, string,
    total,
};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType, TerminalLimit,
};
use crate::providers::artifact::types::artifact_search_query;
use octocode_contract_types::ErrorCode;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use url::Url;

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
    // (take stays <= 1000) and slice the caller's page out locally.
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

async fn service_endpoint(kind: &str, client: &RegistryClient<'_>) -> Result<Url, ArtifactError> {
    let index = client
        .json(
            ArtifactType::Nuget,
            parse_url("https://api.nuget.org/v3/index.json")?,
            false,
            None,
            None,
            JSON,
        )
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?;
    let resources = rows(
        object_for(&index, ArtifactType::Nuget)?
            .get("resources")
            .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?,
        ArtifactType::Nuget,
    )?;
    let preferred = if kind == "RegistrationsBaseUrl" {
        "RegistrationsBaseUrl/3.6.0"
    } else {
        "SearchQueryService/3.5.0"
    };
    let selected = resources.iter().find_map(|value| {
        let row = value.as_object()?;
        let service_type = row.get("@type")?.as_str()?;
        (service_type == preferred
            || service_type == kind
            || service_type_starts(service_type, kind))
        .then_some(row)
    });
    official_url(selected.and_then(|row| row.get("@id")))
}

fn service_type_starts(service_type: &str, kind: &str) -> bool {
    service_type
        .strip_prefix(kind)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn official_url(value: Option<&Value>) -> Result<Url, ArtifactError> {
    let value = value.and_then(Value::as_str).ok_or_else(|| {
        ArtifactError::new(
            ErrorCode::ProviderError,
            "NuGet did not advertise a supported official metadata endpoint.",
        )
    })?;
    let url = Url::parse(value).map_err(|_| {
        ArtifactError::new(
            ErrorCode::ProviderError,
            "NuGet did not advertise a supported official metadata endpoint.",
        )
    })?;
    let official = url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host == "nuget.org" || host.ends_with(".nuget.org"));
    if official {
        Ok(url)
    } else {
        Err(ArtifactError::new(
            ErrorCode::ProviderError,
            "NuGet did not advertise a supported official metadata endpoint.",
        ))
    }
}

fn catalog(value: &Value) -> Result<&Map<String, Value>, ArtifactError> {
    object_for(value, ArtifactType::Nuget)?
        .get("catalogEntry")
        .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))
        .and_then(|value| object_for(value, ArtifactType::Nuget))
}

fn item(row: &Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let name = required(row.get("id"), ArtifactType::Nuget)?;
    let mut artifact = ArtifactItem::new(
        ArtifactType::Nuget,
        name.clone(),
        format!(
            "https://www.nuget.org/packages/{}",
            super::util::encode_component(&name)
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

fn repository_url(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Object(row) => super::util::repository_url(row.get("url")),
        value => super::util::repository_url(Some(value)),
    }
}

fn compare_versions(left: &str, right: &str) -> Result<Ordering, ArtifactError> {
    let parse = |value: &str| -> Result<(Vec<u64>, Option<Vec<String>>), ArtifactError> {
        let (core, prerelease) = value
            .split_once('-')
            .map_or((value, None), |(a, b)| (a, Some(b)));
        let core = core.split('+').next().unwrap_or(core);
        let values = core
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| super::util::invalid(ArtifactType::Nuget))?;
        if values.is_empty() || values.len() > 4 {
            return Err(super::util::invalid(ArtifactType::Nuget));
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

#[cfg(test)]
mod tests {
    use super::super::test_http::RouteHttp;
    use super::super::types::{artifact_query, test_budget};
    use super::*;
    use crate::providers::artifact::http::{
        ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse,
    };
    use octocode_io::budget::RequestBudget;
    use serde_json::json;

    /// The exact `Newtonsoft.Json` lookup against `http`.
    async fn newtonsoft_exact(http: &RouteHttp) -> ArtifactProviderPage {
        let b = test_budget();
        let client = RegistryClient::uncached(http, &b);
        let q = artifact_query(
            serde_json::json!({"ecosystem": ArtifactType::Nuget, "packageName": "Newtonsoft.Json".to_string()}),
            None,
        );
        nuget(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("nuget exact")
    }

    const REGISTRATION: &str = "/v3/registration5/newtonsoft.json/index.json";

    fn service_index() -> serde_json::Value {
        json!({
            "resources": [
                {
                    "@type": "RegistrationsBaseUrl/3.6.0",
                    "@id": "https://api.nuget.org/v3/registration5/"
                }
            ]
        })
    }

    #[tokio::test]
    async fn nuget_exact_prefers_the_latest_stable_release_and_reads_the_nuspec_repository() {
        let registration = json!({"items": [{
            "upper": "14.0.1-beta2",
            "items": [
                {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.4"}},
                {"catalogEntry": {"id": "Newtonsoft.Json", "version": "14.0.1-beta2"}},
                {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.5", "listed": false}}
            ]
        }]});
        let mut index = service_index();
        index["resources"].as_array_mut().unwrap().push(json!({
            "@type": "PackageBaseAddress/3.0.0",
            "@id": "https://api.nuget.org/v3-flatcontainer/"
        }));
        let http = RouteHttp::routes(vec![
            (REGISTRATION, registration),
            ("/v3/index.json", index),
        ])
        .bytes(
            "/v3-flatcontainer/newtonsoft.json/13.0.4/newtonsoft.json.nuspec",
            br#"<package><metadata><repository type="git" url="https://github.com/JamesNK/Newtonsoft.Json" commit="0A2E291C0D9C0C7675D445703E51750363A549EF" /></metadata></package>"#.to_vec(),
        );
        let page = newtonsoft_exact(&http).await;
        let item = &page.artifacts[0];
        // 13.0.5 is unlisted; 14.0.1-beta2 is a prerelease.
        assert_eq!(item.version.as_deref(), Some("13.0.4"));
        // The nuspec's build commit pins the upstream-source lead.
        assert_eq!(
            item.source_ref.as_deref(),
            Some("0a2e291c0d9c0c7675d445703e51750363a549ef")
        );
        assert_eq!(
            item.repository.as_deref(),
            Some("https://github.com/JamesNK/Newtonsoft.Json")
        );
    }

    #[tokio::test]
    async fn nuget_parses_exact_inline_items() {
        // GET https://api.nuget.org/v3/registration5/newtonsoft.json/index.json
        // Items are inline so no further HTTP call is needed.
        let registration = json!({
            "items": [
                {
                    "upper": "13.0.3",
                    "items": [
                        {
                            "catalogEntry": {
                                "id": "Newtonsoft.Json",
                                "version": "13.0.3",
                                "description": "Json.NET is a popular high-performance JSON framework",
                                "projectUrl": "https://www.newtonsoft.com/json",
                                "licenseExpression": "MIT"
                            }
                        }
                    ]
                }
            ]
        });
        let http = RouteHttp::routes(vec![
            (REGISTRATION, registration),
            ("/v3/index.json", service_index()),
        ]);
        let page = newtonsoft_exact(&http).await;
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "Newtonsoft.Json");
        assert_eq!(item.version.as_deref(), Some("13.0.3"));
        assert_eq!(
            item.registry_url,
            "https://www.nuget.org/packages/Newtonsoft.Json"
        );
        // Live catalogEntry payloads omit `repository` (verified against
        // registration5-gz-semver2 for Newtonsoft.Json). A marketing
        // projectUrl is a homepage, never a verified source repository.
        assert_eq!(item.repository, None);
        assert_eq!(
            item.homepage.as_deref(),
            Some("https://www.newtonsoft.com/json")
        );
    }

    /// An exact version reads the registration page whose bounds hold it
    /// (inline or linked), even when a newer release exists; a missing one
    /// names the nearest published versions; a range is refused.
    #[tokio::test]
    async fn nuget_exact_version_reads_the_page_that_holds_it() {
        let registration = json!({"items": [
            {"lower": "13.0.1", "upper": "13.0.4", "items": [
                {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.1"}},
                {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.3", "licenseExpression": "MIT"}},
                {"catalogEntry": {"id": "Newtonsoft.Json", "version": "13.0.4"}}
            ]},
            {"lower": "9.0.1", "upper": "12.0.3",
             "@id": "https://api.nuget.org/v3/registration5/newtonsoft.json/page/9.0.1/12.0.3.json"}
        ]});
        let old_page = json!({"items": [
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "9.0.1"}},
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "12.0.3"}}
        ]});
        let http = RouteHttp::routes(vec![
            (
                "/v3/registration5/newtonsoft.json/page/9.0.1/12.0.3.json",
                old_page,
            ),
            (REGISTRATION, registration),
            ("/v3/index.json", service_index()),
        ]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let lookup = |version: &str| {
            artifact_query(
                json!({"ecosystem": ArtifactType::Nuget, "packageName": "Newtonsoft.Json", "version": version}),
                None,
            )
        };
        let state = ArtifactProviderState::default();
        for (requested, expected) in [
            ("13.0.3", "13.0.3"),
            ("12.0.3", "12.0.3"),
            ("v13.0.3", "13.0.3"),
            ("13.0.3.0", "13.0.3"),
            ("latest", "13.0.4"),
        ] {
            let page = nuget(&lookup(requested), &state, &client)
                .await
                .unwrap_or_else(|error| panic!("{requested}: {error:?}"));
            assert_eq!(
                page.artifacts[0].version.as_deref(),
                Some(expected),
                "{requested}"
            );
        }
        let missing = nuget(&lookup("13.0.2"), &state, &client)
            .await
            .expect_err("missing version");
        assert_eq!(missing.code, ErrorCode::VersionNotFound, "{missing:?}");
        assert!(
            missing.nearest.iter().any(|version| version == "13.0.3"),
            "{missing:?}"
        );
        let range = nuget(&lookup("[13.0,14.0)"), &state, &client)
            .await
            .expect_err("range");
        assert_eq!(range.code, ErrorCode::CapabilityUnavailable, "{range:?}");
    }

    /// An exact row states its publish day and its dependency groups'
    /// entries once each; one only some frameworks declare names them. An
    /// unlisted release's placeholder day (1900) is no day, and a release
    /// without dependency groups declares none.
    #[tokio::test]
    async fn nuget_exact_rows_state_the_publish_day_and_dependencies() {
        let dependency = |id: &str, range: &str| json!({"id": id, "range": range});
        let registration = json!({"items": [{"upper": "4.4.0", "items": [
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "4.4.0",
                "published": "2026-07-10T03:35:24.047+00:00",
                "dependencyGroups": [
                    {"targetFramework": "net10.0"},
                    {"targetFramework": ".NETFramework4.6.2", "dependencies": [
                        dependency("System.Diagnostics.DiagnosticSource", "[8.0.1, )"),
                        dependency("System.ValueTuple", "[4.5.0, )")]},
                    {"targetFramework": "net8.0", "dependencies": [
                        dependency("System.Diagnostics.DiagnosticSource", "[8.0.1, )")]},
                    {"dependencies": [dependency("Everywhere", "[1.0.0, )")]}
                ]}}
        ]}]});
        let http = RouteHttp::routes(vec![
            (REGISTRATION, registration),
            ("/v3/index.json", service_index()),
        ]);
        let item = &newtonsoft_exact(&http).await.artifacts[0];
        assert_eq!(item.published_at.as_deref(), Some("2026-07-10"));
        assert_eq!(
            item.dependencies,
            Some(vec![
                "System.Diagnostics.DiagnosticSource [8.0.1, ) (.NETFramework4.6.2, net8.0)"
                    .to_owned(),
                "System.ValueTuple [4.5.0, ) (.NETFramework4.6.2)".to_owned(),
                "Everywhere [1.0.0, )".to_owned(),
            ])
        );
        let unlisted = json!({"items": [{"upper": "1.0.0", "items": [
            {"catalogEntry": {"id": "Newtonsoft.Json", "version": "1.0.0",
                "published": "1900-01-01T00:00:00+00:00"}}
        ]}]});
        let http = RouteHttp::routes(vec![
            (REGISTRATION, unlisted),
            ("/v3/index.json", service_index()),
        ]);
        let item = &newtonsoft_exact(&http).await.artifacts[0];
        assert_eq!(item.published_at, None);
        assert_eq!(item.dependencies, Some(Vec::new()));
    }

    #[test]
    fn project_url_is_a_repository_only_on_a_source_host() {
        let row = |project: &str| {
            json!({"id": "P", "projectUrl": project})
                .as_object()
                .cloned()
                .unwrap()
        };
        let github = item(&row("https://github.com/o/r")).unwrap();
        assert_eq!(github.repository.as_deref(), Some("https://github.com/o/r"));
        for homepage in [
            "https://www.newtonsoft.com/json",
            "https://github.com/o",
            "https://github.com.evil.example/o/r",
        ] {
            let artifact = item(&row(homepage)).unwrap();
            assert_eq!(artifact.repository, None, "{homepage}");
            assert_eq!(artifact.homepage.as_deref(), Some(homepage));
        }
    }

    /// Search mock: echoes `take` synthetic rows starting at `skip` and
    /// records every search request's (skip, take).
    struct SearchMock(std::sync::Mutex<Vec<(u64, u64)>>);

    impl ArtifactHttp for SearchMock {
        fn get<'a>(
            &'a self,
            req: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            let body = if req.url.path().ends_with("/v3/index.json") {
                json!({"resources": [{"@type": "SearchQueryService/3.5.0", "@id": "https://azuresearch-usnc.nuget.org/query"}]})
            } else {
                let param = |name: &str| {
                    req.url
                        .query_pairs()
                        .find(|(key, _)| key == name)
                        .and_then(|(_, value)| value.parse::<u64>().ok())
                        .unwrap()
                };
                let (skip, take) = (param("skip"), param("take"));
                self.0.lock().unwrap().push((skip, take));
                let data: Vec<_> = (skip..skip + take)
                    .map(|index| json!({"id": format!("pkg{index}"), "version": "1.0.0"}))
                    .collect();
                json!({"totalHits": 50_000, "data": data})
            };
            Box::pin(async move {
                Ok(ArtifactHttpResponse {
                    status: 200,
                    body: serde_json::to_vec(&body).unwrap(),
                })
            })
        }
    }

    #[tokio::test]
    async fn nuget_tail_pages_respect_page_size_past_the_skip_limit() {
        let http = SearchMock(Default::default());
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let q = artifact_query(
            json!({"ecosystem": ArtifactType::Nuget, "keywords": ["json"], "pageSize": 2}),
            None,
        );
        let at = |offset: u64| ArtifactProviderState {
            offset: Some(offset),
            ..Default::default()
        };
        let names = |page: &ArtifactProviderPage| {
            page.artifacts
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
        };
        let page = nuget(&q, &at(3000), &client).await.unwrap();
        assert_eq!(names(&page), ["pkg3000", "pkg3001"]);
        assert_eq!(page.next_state.as_ref().unwrap().offset, Some(3002));
        // Past skip=3000 the tail is sliced locally from one bounded window.
        let page = nuget(&q, &at(3002), &client).await.unwrap();
        assert_eq!(names(&page), ["pkg3002", "pkg3003"]);
        // The final reachable page serves the last rows; the shared page
        // bounds end it with an honest terminal limit, and no request
        // starts at or past result 4000.
        let page = nuget(&q, &at(3998), &client).await.unwrap();
        assert_eq!(names(&page), ["pkg3998", "pkg3999"]);
        let mut paged = q.clone();
        artifact_search_query::set_page(&mut paged, 2000);
        assert!(super::super::page_out_of_range(&paged).is_none());
        artifact_search_query::set_page(&mut paged, 2001);
        assert_eq!(
            super::super::page_out_of_range(&paged).map(|limit| limit.reason),
            Some("providerResultCap")
        );
        assert!(
            super::super::validate_cursor_state(
                ArtifactType::Nuget,
                &at(3999),
                artifact_search_query::page_size(&q)
            )
            .is_ok()
        );
        assert_eq!(
            super::super::validate_cursor_state(
                ArtifactType::Nuget,
                &at(4000),
                artifact_search_query::page_size(&q)
            )
            .expect_err("past the reach")
            .code,
            ErrorCode::InvalidInput
        );
        assert!(
            http.0
                .lock()
                .unwrap()
                .iter()
                .all(|&(skip, take)| skip <= 3000 && take <= 1000)
        );
    }

    #[tokio::test]
    async fn nuget_prefers_repository_object_over_project_url() {
        let registration = json!({
            "items": [
                {
                    "upper": "4.4.0",
                    "items": [
                        {
                            "catalogEntry": {
                                "id": "Serilog",
                                "version": "4.4.0",
                                "projectUrl": "https://serilog.net/",
                                "repository": {
                                    "type": "git",
                                    "url": "https://github.com/serilog/serilog"
                                }
                            }
                        }
                    ]
                }
            ]
        });
        let http = RouteHttp::routes(vec![
            ("/v3/registration5/serilog/index.json", registration),
            ("/v3/index.json", service_index()),
        ]);
        let b = test_budget();
        let client = RegistryClient::uncached(&http, &b);
        let q = artifact_query(
            serde_json::json!({"ecosystem": ArtifactType::Nuget, "packageName": "Serilog".to_string()}),
            None,
        );
        let page = nuget(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("nuget exact");
        let item = &page.artifacts[0];
        assert_eq!(
            item.repository.as_deref(),
            Some("https://github.com/serilog/serilog")
        );
        assert_eq!(item.homepage.as_deref(), Some("https://serilog.net/"));
    }

    #[test]
    fn repository_url_accepts_object_and_string_shapes() {
        assert_eq!(
            repository_url(Some(&json!({"url": "https://github.com/a/b"}))).as_deref(),
            Some("https://github.com/a/b")
        );
        assert_eq!(
            repository_url(Some(&json!("https://github.com/a/b"))).as_deref(),
            Some("https://github.com/a/b")
        );
        // Empty-string repository (common in NuGet catalog data) yields None.
        assert_eq!(repository_url(Some(&json!(""))), None);
        assert_eq!(repository_url(None), None);
    }

    #[test]
    fn compare_versions_orders_semver_correctly() {
        use std::cmp::Ordering;
        // stable > prerelease
        assert_eq!(
            compare_versions("13.0.3", "13.0.3-beta1").expect("NuGet test data should be valid"),
            Ordering::Greater
        );
        // higher patch wins
        assert_eq!(
            compare_versions("2.0.1", "2.0.0").expect("NuGet test data should be valid"),
            Ordering::Greater
        );
        // equal
        assert_eq!(
            compare_versions("1.0.0", "1.0.0").expect("NuGet test data should be valid"),
            Ordering::Equal
        );
        // 4-part version
        assert_eq!(
            compare_versions("1.2.3.4", "1.2.3.3").expect("NuGet test data should be valid"),
            Ordering::Greater
        );
    }
}
