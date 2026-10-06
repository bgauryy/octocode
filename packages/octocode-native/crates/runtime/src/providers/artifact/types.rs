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
    /// The requested version, range, or tag: the `version` field, else the
    /// registry's own coordinate suffix (`name@x` on npm/crates, `name==x`
    /// on PyPI), which stays valid input. `None` means the latest release.
    pub fn version(&self) -> Option<&str> {
        let explicit = match self {
            Self::NpmExact(query) => query.version.as_deref().map(String::as_str),
            Self::RegistryExact(query) => query.version.as_deref().map(String::as_str),
            _ => None,
        };
        explicit
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| self.split_coordinate().1)
    }

    /// The package name without a coordinate version suffix.
    pub fn bare_package_name(&self) -> Option<&str> {
        self.package_name().map(|_| self.split_coordinate().0)
    }

    fn split_coordinate(&self) -> (&str, Option<&str>) {
        let Some(name) = self.package_name() else {
            return ("", None);
        };
        match self.artifact_type() {
            ArtifactType::Npm => super::npm::split_npm_coordinate(name),
            ArtifactType::Pypi => match name.split_once("==") {
                Some((bare, version)) if !bare.trim().is_empty() && !version.trim().is_empty() => {
                    (bare.trim(), Some(version.trim()))
                }
                _ => (name, None),
            },
            ArtifactType::Crates => match name.split_once('@') {
                Some((bare, version)) if !bare.is_empty() && !version.is_empty() => {
                    (bare, Some(version))
                }
                _ => (name, None),
            },
            _ => (name, None),
        }
    }

    /// `debug:true`: keep diagnostic row fields (registry URLs).
    pub fn debug(&self) -> bool {
        match self {
            Self::NpmExact(query) => query.debug,
            Self::NpmDiscovery(query) => query.debug,
            Self::RegistryExact(query) => query.debug,
            Self::RegistryDiscovery(query) => query.debug,
        }
    }

    pub fn registry(&self) -> Option<&str> {
        match self {
            Self::NpmExact(query) => query.registry.as_deref(),
            Self::NpmDiscovery(query) => query.registry.as_deref(),
            _ => None,
        }
    }
    /// The 1-based discovery page (`page`); exact lookups have one.
    pub fn page(&self) -> u64 {
        match self {
            Self::NpmDiscovery(query) => query.page.map_or(1, |page| page.get()),
            Self::RegistryDiscovery(query) => query.page.map_or(1, |page| page.get()),
            _ => 1,
        }
    }
    /// Points a discovery query at `page`.
    pub fn set_page(&mut self, page: u64) {
        let page = std::num::NonZeroU64::new(page);
        match self {
            Self::NpmDiscovery(query) => query.page = page,
            Self::RegistryDiscovery(query) => query.page = page,
            _ => {}
        }
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
        || serde_json::json!({"type": "npm", "mainGoal": "test", "reasoning": "test"}),
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

/// A request budget no artifact test exhausts.
#[cfg(test)]
pub(crate) fn test_budget() -> crate::providers::RequestBudget {
    crate::providers::RequestBudget::with_timeout(std::time::Duration::from_secs(10), 10_000_000)
}

/// Answers every request with status 200 and one fixed body.
#[cfg(test)]
pub(crate) struct StaticHttp(pub(crate) Vec<u8>);

#[cfg(test)]
impl StaticHttp {
    pub(crate) fn json(body: serde_json::Value) -> Self {
        Self(serde_json::to_vec(&body).expect("test body serializes"))
    }
}

#[cfg(test)]
impl super::http::ArtifactHttp for StaticHttp {
    fn get<'a>(
        &'a self,
        _req: super::http::ArtifactHttpRequest,
        _budget: &'a crate::providers::RequestBudget,
    ) -> super::http::ArtifactHttpFuture<'a> {
        let body = self.0.clone();
        Box::pin(async move { Ok(super::http::ArtifactHttpResponse { status: 200, body }) })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactItem {
    /// The queried registry; the request states it, so rows omit it.
    #[serde(skip)]
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
    /// Release date (YYYY-MM-DD) of this version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// Registry deprecation message for this version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<String>,
    /// Set (true) only when the registry yanked this version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yanked: Option<bool>,
    /// Runtime dependency count of this version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<usize>,
    /// Those runtime dependencies as the registry declares them
    /// (`name@range`, a PEP 508 requirement, `name req`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependency_list: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_dependencies: Option<usize>,
    /// npm `engines.node` range.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engines: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_python: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_version: Option<String>,
    /// npm discovery: downloads in the last month.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads_monthly: Option<u64>,
    /// Commit or tag the registry says this version was published from; it
    /// pins the `viewReleaseSource` lead and is not a public row field.
    #[serde(skip)]
    pub source_ref: Option<String>,
    /// `source_ref` came from a provenance attestation bound to this
    /// tarball and repository, not from an unchecked registry field.
    #[serde(skip)]
    pub source_attested: bool,
    /// `source_ref` is an upstream release tag checked to exist.
    #[serde(skip)]
    pub source_tag: bool,
    /// Directory of the published entry point inside the package directory,
    /// when it is shipped source (npm `main` without a build step); it
    /// points the source lead and is not a public row field.
    #[serde(skip)]
    pub entry_directory: Option<String>,
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
            published_at: None,
            deprecated: None,
            yanked: None,
            dependencies: None,
            dependency_list: Vec::new(),
            peer_dependencies: None,
            engines: None,
            requires_python: None,
            rust_version: None,
            downloads_monthly: None,
            source_ref: None,
            source_attested: false,
            source_tag: false,
            entry_directory: None,
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

/// Resolved credential together with its normalized npmrc path scope.
#[derive(Clone)]
pub struct NpmAuthorization {
    pub(super) header: SecretString,
    pub(super) scope: String,
}

#[derive(Clone)]
pub struct ResolvedNpmRegistry {
    pub base: Url,
    pub authorization: Option<NpmAuthorization>,
    pub cache_identity: String,
}

impl ResolvedNpmRegistry {
    /// Attach credentials only inside their original origin and npmrc path scope.
    pub(crate) fn authorization_for(&self, target: &Url) -> Option<SecretString> {
        if target.origin() != self.base.origin()
            || !target.username().is_empty()
            || target.password().is_some()
        {
            return None;
        }
        let authorization = self.authorization.as_ref()?;
        let target = super::npmrc::nerf_dart(target)?;
        target
            .starts_with(&authorization.scope)
            .then(|| authorization.header.clone())
    }
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
