use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::fmt;
use url::Url;

pub use crate::contracts::tool_types::{ArtifactSearchQuery, ArtifactType};
use crate::contracts::tool_types::{RegistryDiscoveryType, RegistryExactType};

impl ArtifactType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pypi => "pypi",
            Self::Crates => "crates",
            Self::Maven => "maven",
            Self::Nuget => "nuget",
            Self::Go => "go",
            Self::Packagist => "packagist",
            Self::Rubygems => "rubygems",
        }
    }
}

/// Registry-facing views over the generated wire query.
impl ArtifactSearchQuery {
    pub fn artifact_type(&self) -> ArtifactType {
        match self {
            Self::NpmExact(_) | Self::NpmDiscovery(_) => ArtifactType::Npm,
            Self::RegistryExact(query) => match query.type_ {
                RegistryExactType::Pypi => ArtifactType::Pypi,
                RegistryExactType::Crates => ArtifactType::Crates,
                RegistryExactType::Maven => ArtifactType::Maven,
                RegistryExactType::Nuget => ArtifactType::Nuget,
                RegistryExactType::Go => ArtifactType::Go,
                RegistryExactType::Packagist => ArtifactType::Packagist,
                RegistryExactType::Rubygems => ArtifactType::Rubygems,
            },
            Self::RegistryDiscovery(query) => match query.type_ {
                RegistryDiscoveryType::Crates => ArtifactType::Crates,
                RegistryDiscoveryType::Maven => ArtifactType::Maven,
                RegistryDiscoveryType::Nuget => ArtifactType::Nuget,
                RegistryDiscoveryType::Go => ArtifactType::Go,
                RegistryDiscoveryType::Packagist => ArtifactType::Packagist,
                RegistryDiscoveryType::Rubygems => ArtifactType::Rubygems,
            },
        }
    }
    pub fn package_name(&self) -> Option<&str> {
        match self {
            Self::NpmExact(query) => Some(query.package_name.as_str()),
            Self::RegistryExact(query) => Some(query.package_name.as_str()),
            _ => None,
        }
    }
    pub fn registry(&self) -> Option<&str> {
        match self {
            Self::NpmExact(query) => query.registry.as_deref(),
            Self::NpmDiscovery(query) => query.registry.as_deref(),
            _ => None,
        }
    }
    pub fn cursor(&self) -> Option<&str> {
        match self {
            Self::NpmDiscovery(query) => query.cursor.as_ref().map(|value| value.as_str()),
            Self::RegistryDiscovery(query) => query.cursor.as_ref().map(|value| value.as_str()),
            _ => None,
        }
    }
    pub fn set_cursor(&mut self, value: &str) -> Result<(), ArtifactError> {
        let invalid =
            |_| ArtifactError::new("invalid_query", "Invalid artifact continuation cursor.");
        match self {
            Self::NpmDiscovery(query) => query.cursor = Some(value.parse().map_err(invalid)?),
            Self::RegistryDiscovery(query) => query.cursor = Some(value.parse().map_err(invalid)?),
            _ => {
                return Err(ArtifactError::new(
                    "invalid_query",
                    "Exact lookups do not accept cursors.",
                ));
            }
        }
        Ok(())
    }
    pub fn page_size(&self) -> Option<usize> {
        match self {
            Self::NpmDiscovery(query) => Some(query.page_size.get() as usize),
            Self::RegistryDiscovery(query) => Some(query.page_size.get() as usize),
            _ => None,
        }
    }
    /// Keywords as one space-joined registry search text.
    pub fn terms(&self) -> String {
        match self {
            Self::NpmDiscovery(query) => query
                .keywords
                .iter()
                .map(|word| word.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            Self::RegistryDiscovery(query) => query
                .keywords
                .iter()
                .map(|word| word.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        }
    }
}

/// Parses an artifactSearch query from JSON fields layered over `base`
/// (or a minimal npm query); a `null` field removes it.
#[cfg(test)]
pub(crate) fn artifact_query(
    fields: serde_json::Value,
    base: Option<&ArtifactSearchQuery>,
) -> ArtifactSearchQuery {
    let mut value = base.map_or_else(
        || serde_json::json!({"type": "npm", "goal": "test", "reasoning": "test"}),
        |base| serde_json::to_value(base).expect("query serializes"),
    );
    let object = value.as_object_mut().expect("query object");
    for (key, field) in fields.as_object().expect("fields object") {
        if field.is_null() {
            object.remove(key);
        } else {
            object.insert(key.clone(), field.clone());
        }
    }
    serde_json::from_value(value).expect("valid artifactSearch query")
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactItem {
    #[serde(rename = "type")]
    pub artifact_type: ArtifactType,
    pub name: String,
    pub registry_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_path: Option<String>,
    /// Commit or tag the registry says this version was published from; it
    /// pins the `viewRepo` lead and is not a public row field.
    #[serde(skip)]
    pub source_ref: Option<String>,
}

impl ArtifactItem {
    pub(crate) fn new(artifact_type: ArtifactType, name: String, registry_url: String) -> Self {
        Self {
            artifact_type,
            name,
            registry_url,
            version: None,
            description: None,
            license: None,
            homepage: None,
            repository: None,
            repository_directory: None,
            package_path: None,
            module_path: None,
            source_ref: None,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactProviderState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactProviderPage {
    pub artifacts: Vec<ArtifactItem>,
    pub next_state: Option<ArtifactProviderState>,
    pub total: Option<u64>,
    pub terminal_limit: Option<String>,
    pub registry: Option<String>,
}

impl ArtifactProviderPage {
    pub(crate) fn empty(total: Option<u64>) -> Self {
        Self {
            artifacts: vec![],
            next_state: None,
            total,
            terminal_limit: None,
            registry: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
}

impl ArtifactError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            status: None,
            hints: vec![],
        }
    }

    pub(crate) fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    pub(crate) fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hints.push(hint.into());
        self
    }
}

#[derive(Clone)]
pub struct ResolvedNpmRegistry {
    pub base: Url,
    /// Complete already-resolved Authorization header value.
    pub authorization: Option<SecretString>,
    pub cache_identity: String,
}

impl fmt::Debug for ResolvedNpmRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedNpmRegistry")
            .field("base", &self.base)
            .field(
                "authorization",
                &self.authorization.as_ref().map(|_| "[REDACTED]"),
            )
            .field("cache_identity", &self.cache_identity)
            .finish()
    }
}
