use super::http::RegistryClient;
use super::util::{endpoint, object_for, parse_url, required, rows, safe_url, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactType,
};
use serde_json::{Map, Value};
use std::cmp::Ordering;
use url::Url;

pub(crate) async fn nuget(
    query: &ArtifactSearchQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.package_name() {
        return exact(name, client).await;
    }
    let offset = state.offset.unwrap_or(0);
    if offset >= REACHABLE_END {
        return Ok(ArtifactProviderPage {
            artifacts: vec![],
            next_state: None,
            total: None,
            terminal_limit: Some(limit_reason()),
            registry: None,
        });
    }
    let base = service_endpoint("SearchQueryService", client).await?;
    let size = query
        .page_size()
        .unwrap_or(10)
        .min((REACHABLE_END - offset) as usize);
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
            ("q", Some(query.terms())),
            ("skip", Some(skip.to_string())),
            ("take", Some(take.to_string())),
            ("prerelease", Some("true".into())),
            ("semVerLevel", Some("2.0.0".into())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Nuget, url, false, None)
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
    let reason = if more && artifacts.is_empty() {
        Some("NuGet returned an empty page before its reported total.".into())
    } else if more && next_offset >= REACHABLE_END {
        Some(limit_reason())
    } else {
        None
    };
    Ok(ArtifactProviderPage {
        next_state: (more && reason.is_none()).then(|| ArtifactProviderState {
            offset: Some(next_offset),
            ..Default::default()
        }),
        artifacts,
        total: count,
        terminal_limit: reason,
        registry: None,
    })
}

/// NuGet search accepts `skip <= 3000` and `take <= 1000`, so result 4000 is
/// the last one any request can reach.
const MAX_SKIP: u64 = 3000;
const REACHABLE_END: u64 = MAX_SKIP + 1000;

async fn exact(
    package_name: &str,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let base = service_endpoint("RegistrationsBaseUrl", client).await?;
    let url = parse_url(&format!(
        "{}/{}/index.json",
        base.as_str().trim_end_matches('/'),
        super::util::encode_component(&package_name.to_ascii_lowercase())
    ))?;
    let Some(response) = client.json(ArtifactType::Nuget, url, true, None).await? else {
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
    // Newest registration pages first. The highest listed stable version wins;
    // a prerelease only when the package has no stable release (as NuGet
    // clients resolve "latest").
    let mut pages = pages
        .iter()
        .map(|page| object_for(page, ArtifactType::Nuget))
        .collect::<Result<Vec<_>, _>>()?;
    let upper = |page: &Map<String, Value>| {
        required(page.get("upper"), ArtifactType::Nuget).unwrap_or_default()
    };
    pages.sort_by(|a, b| compare_versions(&upper(b), &upper(a)).unwrap_or(Ordering::Equal));
    let mut stable: Option<Map<String, Value>> = None;
    let mut prerelease: Option<Map<String, Value>> = None;
    for page in pages.iter().take(MAX_REGISTRATION_PAGES) {
        let owned_page;
        let items = if let Some(items) = page.get("items") {
            items
        } else {
            let advertised = official_url(page.get("@id"))?;
            owned_page = client
                .json(ArtifactType::Nuget, advertised, false, None)
                .await?
                .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?;
            object_for(&owned_page, ArtifactType::Nuget)?
                .get("items")
                .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?
        };
        for leaf in rows(items, ArtifactType::Nuget)? {
            let candidate = catalog(leaf)?;
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
    let latest = stable
        .or(prerelease)
        .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?;
    let mut artifact = item(&latest)?;
    // The registration catalogEntry usually omits `repository`; the package's
    // own nuspec declares it.
    if artifact.repository.is_none()
        && let Some(version) = artifact.version.clone()
    {
        artifact.repository = nuspec_repository(package_name, &version, client).await;
    }
    Ok(ArtifactProviderPage {
        artifacts: vec![artifact],
        next_state: None,
        total: Some(1),
        terminal_limit: None,
        registry: None,
    })
}

/// Registration pages read (newest first) while looking for a stable release.
const MAX_REGISTRATION_PAGES: usize = 4;

/// `<repository url="…">` from the package's nuspec in the flat container.
async fn nuspec_repository(
    package_name: &str,
    version: &str,
    client: &RegistryClient<'_>,
) -> Option<String> {
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
    let url = tag.split("url=\"").nth(1)?.split('"').next()?;
    safe_url(Some(&Value::String(url.to_owned())))
}

async fn service_endpoint(kind: &str, client: &RegistryClient<'_>) -> Result<Url, ArtifactError> {
    let index = client
        .json(
            ArtifactType::Nuget,
            parse_url("https://api.nuget.org/v3/index.json")?,
            false,
            None,
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
            "provider_error",
            "NuGet did not advertise a supported official metadata endpoint.",
        )
    })?;
    let url = Url::parse(value).map_err(|_| {
        ArtifactError::new(
            "provider_error",
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
            "provider_error",
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
        Value::Object(row) => safe_url(row.get("url")),
        value => safe_url(Some(value)),
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

fn limit_reason() -> String {
    "NuGet search reaches only the first 4000 results (skip <= 3000, take <= 1000). Narrow keywords to reach additional packages.".into()
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

    /// Serves fixtures by URL-path fragment. Routing (rather than call order)
    /// keeps tests independent of the process-global artifact JSON cache,
    /// which is keyed by URL and can absorb the shared index.json fetch.
    struct RouteMock(Vec<(&'static str, Vec<u8>)>);

    impl RouteMock {
        fn json_routes(routes: Vec<(&'static str, serde_json::Value)>) -> Self {
            Self(
                routes
                    .into_iter()
                    .map(|(fragment, value)| {
                        (
                            fragment,
                            serde_json::to_vec(&value).expect("NuGet test data should be valid"),
                        )
                    })
                    .collect(),
            )
        }
    }

    impl ArtifactHttp for RouteMock {
        fn get<'a>(
            &'a self,
            req: ArtifactHttpRequest,
            _budget: &'a RequestBudget,
        ) -> ArtifactHttpFuture<'a> {
            let path = req.url.path().to_owned();
            let body = self
                .0
                .iter()
                .find(|(fragment, _)| path.contains(fragment))
                .map(|(_, body)| body.clone());
            Box::pin(async move {
                match body {
                    Some(body) => Ok(ArtifactHttpResponse { status: 200, body }),
                    None => Ok(ArtifactHttpResponse {
                        status: 404,
                        body: b"not found".to_vec(),
                    }),
                }
            })
        }
    }

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

    fn budget() -> RequestBudget {
        RequestBudget::with_timeout(Duration::from_secs(10), 10_000_000)
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
        let mut http = RouteMock::json_routes(vec![
            ("registration5/newtonsoft.json", registration),
            ("/v3/index.json", index),
        ]);
        http.0.push((
            "newtonsoft.json.nuspec",
            br#"<package><metadata><repository type="git" url="https://github.com/JamesNK/Newtonsoft.Json" /></metadata></package>"#.to_vec(),
        ));
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let q = artifact_query(
            serde_json::json!({"type": ArtifactType::Nuget, "packageName": "Newtonsoft.Json".to_string()}),
            None,
        );
        let page = nuget(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("nuget exact");
        let item = &page.artifacts[0];
        // 13.0.5 is unlisted; 14.0.1-beta2 is a prerelease.
        assert_eq!(item.version.as_deref(), Some("13.0.4"));
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
        let http = RouteMock::json_routes(vec![
            ("newtonsoft.json", registration),
            ("/v3/index.json", service_index()),
        ]);
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let q = artifact_query(
            serde_json::json!({"type": ArtifactType::Nuget, "packageName": "Newtonsoft.Json".to_string()}),
            None,
        );
        let page = nuget(&q, &ArtifactProviderState::default(), &client)
            .await
            .expect("nuget exact");
        assert_eq!(page.artifacts.len(), 1);
        let item = &page.artifacts[0];
        assert_eq!(item.name, "Newtonsoft.Json");
        assert_eq!(item.version.as_deref(), Some("13.0.3"));
        assert!(
            item.registry_url.contains("nuget.org"),
            "{}",
            item.registry_url
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
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let q = artifact_query(
            json!({"type": ArtifactType::Nuget, "keywords": ["json"], "pageSize": 2}),
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
        // The final reachable page ends with an honest terminal limit.
        let page = nuget(&q, &at(3998), &client).await.unwrap();
        assert_eq!(names(&page), ["pkg3998", "pkg3999"]);
        assert!(page.next_state.is_none());
        assert!(page.terminal_limit.is_some());
        let page = nuget(&q, &at(4000), &client).await.unwrap();
        assert!(page.artifacts.is_empty() && page.terminal_limit.is_some());
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
        let http = RouteMock::json_routes(vec![
            ("serilog", registration),
            ("/v3/index.json", service_index()),
        ]);
        let b = budget();
        let client = RegistryClient {
            http: &http,
            budget: &b,
            cache_revision: 0,
            cache_enabled: false,
        };
        let q = artifact_query(
            serde_json::json!({"type": ArtifactType::Nuget, "packageName": "Serilog".to_string()}),
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
