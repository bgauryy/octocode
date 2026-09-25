use super::http::RegistryClient;
use super::util::{endpoint, object_for, parse_url, required, rows, safe_url, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactSearchQuery,
    ArtifactSearchQueryType,
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
    if offset > 3000 {
        return Ok(ArtifactProviderPage {
            artifacts: vec![],
            next_state: None,
            total: None,
            terminal_limit: Some(limit_reason()),
            registry: None,
        });
    }
    let base = service_endpoint("SearchQueryService", client).await?;
    let size = if offset == 3000 {
        1000
    } else {
        query
            .page_size()
            .unwrap_or(10)
            .min((3000 - offset) as usize)
    };
    let url = endpoint(
        base.as_str(),
        &[
            ("q", Some(query.terms())),
            ("skip", Some(offset.to_string())),
            ("take", Some(size.to_string())),
            ("prerelease", Some("true".into())),
            ("semVerLevel", Some("2.0.0".into())),
        ],
    )?;
    let response = client
        .json(ArtifactSearchQueryType::Nuget, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactSearchQueryType::Nuget))?;
    let data = object_for(&response, ArtifactSearchQueryType::Nuget)?;
    let artifacts = rows(
        data.get("data")
            .ok_or_else(|| super::util::invalid(ArtifactSearchQueryType::Nuget))?,
        ArtifactSearchQueryType::Nuget,
    )?
    .iter()
    .map(|value| item(object_for(value, ArtifactSearchQueryType::Nuget)?))
    .collect::<Result<Vec<_>, _>>()?;
    let count = total(data.get("totalHits"));
    let next_offset = offset + artifacts.len() as u64;
    let more = count
        .map(|count| next_offset < count)
        .unwrap_or(artifacts.len() == size);
    let reason = if more && artifacts.is_empty() {
        Some("NuGet returned an empty page before its reported total.".into())
    } else if more && next_offset > 3000 {
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
    let Some(response) = client.json(ArtifactSearchQueryType::Nuget, url, true, None).await? else {
        return Ok(ArtifactProviderPage::empty(Some(0)));
    };
    let pages = rows(
        object_for(&response, ArtifactSearchQueryType::Nuget)?
            .get("items")
            .ok_or_else(|| super::util::invalid(ArtifactSearchQueryType::Nuget))?,
        ArtifactSearchQueryType::Nuget,
    )?;
    if pages.is_empty() {
        return Ok(ArtifactProviderPage::empty(Some(0)));
    }
    let mut highest = object_for(&pages[0], ArtifactSearchQueryType::Nuget)?;
    for page in &pages[1..] {
        let page = object_for(page, ArtifactSearchQueryType::Nuget)?;
        if compare_versions(
            &required(page.get("upper"), ArtifactSearchQueryType::Nuget)?,
            &required(highest.get("upper"), ArtifactSearchQueryType::Nuget)?,
        )? == Ordering::Greater
        {
            highest = page;
        }
    }
    let owned_page;
    let page = if let Some(items) = highest.get("items") {
        items
    } else {
        let advertised = official_url(highest.get("@id"))?;
        owned_page = client
            .json(ArtifactSearchQueryType::Nuget, advertised, false, None)
            .await?
            .ok_or_else(|| super::util::invalid(ArtifactSearchQueryType::Nuget))?;
        object_for(&owned_page, ArtifactSearchQueryType::Nuget)?
            .get("items")
            .ok_or_else(|| super::util::invalid(ArtifactSearchQueryType::Nuget))?
    };
    let leaves = rows(page, ArtifactSearchQueryType::Nuget)?;
    if leaves.is_empty() {
        return Err(super::util::invalid(ArtifactSearchQueryType::Nuget));
    }
    let mut latest = catalog(&leaves[0])?;
    for leaf in &leaves[1..] {
        let candidate = catalog(leaf)?;
        if compare_versions(
            &required(candidate.get("version"), ArtifactSearchQueryType::Nuget)?,
            &required(latest.get("version"), ArtifactSearchQueryType::Nuget)?,
        )? == Ordering::Greater
        {
            latest = candidate;
        }
    }
    Ok(ArtifactProviderPage {
        artifacts: vec![item(latest)?],
        next_state: None,
        total: Some(1),
        terminal_limit: None,
        registry: None,
    })
}

async fn service_endpoint(kind: &str, client: &RegistryClient<'_>) -> Result<Url, ArtifactError> {
    let index = client
        .json(
            ArtifactSearchQueryType::Nuget,
            parse_url("https://api.nuget.org/v3/index.json")?,
            false,
            None,
        )
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactSearchQueryType::Nuget))?;
    let resources = rows(
        object_for(&index, ArtifactSearchQueryType::Nuget)?
            .get("resources")
            .ok_or_else(|| super::util::invalid(ArtifactSearchQueryType::Nuget))?,
        ArtifactSearchQueryType::Nuget,
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
    object_for(value, ArtifactSearchQueryType::Nuget)?
        .get("catalogEntry")
        .ok_or_else(|| super::util::invalid(ArtifactSearchQueryType::Nuget))
        .and_then(|value| object_for(value, ArtifactSearchQueryType::Nuget))
}

fn item(row: &Map<String, Value>) -> Result<ArtifactItem, ArtifactError> {
    let name = required(row.get("id"), ArtifactSearchQueryType::Nuget)?;
    let mut artifact = ArtifactItem::new(
        ArtifactSearchQueryType::Nuget,
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
    // usually omits it entirely even when the nuspec carries one. Fall back to
    // projectUrl so exact lookups still surface an upstream link, mirroring the
    // packagist/rubygems source-vs-homepage fallback chains.
    artifact.repository =
        repository_url(row.get("repository")).or_else(|| safe_url(row.get("projectUrl")));
    Ok(artifact)
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
            .map_err(|_| super::util::invalid(ArtifactSearchQueryType::Nuget))?;
        if values.is_empty() || values.len() > 4 {
            return Err(super::util::invalid(ArtifactSearchQueryType::Nuget));
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
    "NuGet search supports skip up to 3000. Narrow keywords to reach additional packages.".into()
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
            serde_json::json!({"type": ArtifactSearchQueryType::Nuget, "packageName": "Newtonsoft.Json".to_string()}),
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
        // registration5-gz-semver2 for Newtonsoft.Json); projectUrl is the
        // fallback upstream link.
        assert_eq!(
            item.repository.as_deref(),
            Some("https://www.newtonsoft.com/json")
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
            serde_json::json!({"type": ArtifactSearchQueryType::Nuget, "packageName": "Serilog".to_string()}),
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
