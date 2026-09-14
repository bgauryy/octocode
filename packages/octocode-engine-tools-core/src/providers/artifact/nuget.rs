use super::http::RegistryClient;
use super::util::{endpoint, object_for, parse_url, required, rows, safe_url, string, total};
use super::{
    ArtifactError, ArtifactItem, ArtifactProviderPage, ArtifactProviderState, ArtifactQuery,
    ArtifactType,
};
use serde_json::{Map, Value};
use std::cmp::Ordering;
use url::Url;

pub(crate) async fn nuget(
    query: &ArtifactQuery,
    state: &ArtifactProviderState,
    client: &RegistryClient<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    if let Some(name) = query.package_name.as_deref() {
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
        query.page_size.unwrap_or(10).min((3000 - offset) as usize)
    };
    let url = endpoint(
        base.as_str(),
        &[
            (
                "q",
                Some(
                    query
                        .keywords
                        .as_ref()
                        .map(|v| v.join(" "))
                        .unwrap_or_default(),
                ),
            ),
            ("skip", Some(offset.to_string())),
            ("take", Some(size.to_string())),
            ("prerelease", Some("true".into())),
            ("semVerLevel", Some("2.0.0".into())),
        ],
    )?;
    let response = client
        .json(ArtifactType::Nuget, url, false, None)
        .await?
        .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?;
    let data = object_for(&response, ArtifactType::Nuget)?;
    let artifacts = rows(
        data.get("data")
            .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?,
        ArtifactType::Nuget,
    )?
    .iter()
    .map(|value| item(object_for(value, ArtifactType::Nuget)?))
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
    let mut highest = object_for(&pages[0], ArtifactType::Nuget)?;
    for page in &pages[1..] {
        let page = object_for(page, ArtifactType::Nuget)?;
        if compare_versions(
            &required(page.get("upper"), ArtifactType::Nuget)?,
            &required(highest.get("upper"), ArtifactType::Nuget)?,
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
            .json(ArtifactType::Nuget, advertised, false, None)
            .await?
            .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?;
        object_for(&owned_page, ArtifactType::Nuget)?
            .get("items")
            .ok_or_else(|| super::util::invalid(ArtifactType::Nuget))?
    };
    let leaves = rows(page, ArtifactType::Nuget)?;
    if leaves.is_empty() {
        return Err(super::util::invalid(ArtifactType::Nuget));
    }
    let mut latest = catalog(&leaves[0])?;
    for leaf in &leaves[1..] {
        let candidate = catalog(leaf)?;
        if compare_versions(
            &required(candidate.get("version"), ArtifactType::Nuget)?,
            &required(latest.get("version"), ArtifactType::Nuget)?,
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
    artifact.repository = row
        .get("repository")
        .and_then(Value::as_object)
        .and_then(|value| safe_url(value.get("url")));
    Ok(artifact)
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
    "NuGet search supports skip up to 3000. Narrow keywords to reach additional packages.".into()
}
