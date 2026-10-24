//! The NuGet service index: the official endpoint each resource kind is served from.
use super::super::ArtifactError;
use super::super::http::{JSON, RegistryClient};
use super::super::util::{object_for, parse_url, rows};
use octocode_contract_types::{ArtifactType, ErrorCode};
use serde_json::Value;
use url::Url;

pub(super) async fn service_endpoint(
    kind: &str,
    client: &RegistryClient<'_>,
) -> Result<Url, ArtifactError> {
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
        .ok_or_else(|| super::super::util::invalid(ArtifactType::Nuget))?;
    let resources = rows(
        object_for(&index, ArtifactType::Nuget)?
            .get("resources")
            .ok_or_else(|| super::super::util::invalid(ArtifactType::Nuget))?,
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

pub(super) fn official_url(value: Option<&Value>) -> Result<Url, ArtifactError> {
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
