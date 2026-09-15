use super::{ArtifactError, ArtifactType};
use serde_json::{Map, Value};
use url::Url;

pub(crate) fn object_for(
    value: &Value,
    artifact_type: ArtifactType,
) -> Result<&Map<String, Value>, ArtifactError> {
    value.as_object().ok_or_else(|| invalid(artifact_type))
}

pub(crate) fn rows(
    value: &Value,
    artifact_type: ArtifactType,
) -> Result<&Vec<Value>, ArtifactError> {
    value.as_array().ok_or_else(|| invalid(artifact_type))
}

pub(crate) fn string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn required(
    value: Option<&Value>,
    artifact_type: ArtifactType,
) -> Result<String, ArtifactError> {
    string(value).ok_or_else(|| invalid(artifact_type))
}

pub(crate) fn total(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str()?.parse::<u64>().ok())
    })
}

pub(crate) fn safe_url(value: Option<&Value>) -> Option<String> {
    let value = string(value)?;
    let cleaned = value.strip_prefix("git+").unwrap_or(&value);
    let url = Url::parse(cleaned).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(url.into())
}

pub(crate) fn license(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Array(values) => {
            let joined = values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" OR ");
            (!joined.is_empty()).then_some(joined)
        }
        value => string(Some(value)),
    }
}

pub(crate) fn endpoint(
    base: &str,
    params: &[(&str, Option<String>)],
) -> Result<Url, ArtifactError> {
    let mut url = parse_url(base)?;
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in params {
            if let Some(value) = value {
                pairs.append_pair(key, value);
            }
        }
    }
    Ok(url)
}

pub(crate) fn parse_url(value: &str) -> Result<Url, ArtifactError> {
    Url::parse(value).map_err(|_| {
        ArtifactError::new(
            "provider_error",
            "Artifact registry endpoint could not be constructed.",
        )
    })
}

pub(crate) fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            encoded.push(char::from(*byte));
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub(crate) fn coordinate_path(value: &str) -> String {
    value
        .split('/')
        .map(encode_component)
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn invalid(artifact_type: ArtifactType) -> ArtifactError {
    ArtifactError::new(
        "provider_error",
        format!(
            "{} returned an invalid registry response.",
            artifact_type.as_str()
        ),
    )
}
