use octocode_contract_types::ErrorCode;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use url::Url;

pub use octocode_contract_types::{ArtifactSearchQuery, ArtifactType};
use octocode_contract_types::{RegistryDiscoveryEcosystem, RegistryExactEcosystem};

pub(crate) mod artifact_type {
    use super::*;
    pub(crate) fn as_str(kind: ArtifactType) -> &'static str {
        match kind {
            ArtifactType::Npm => "npm",
            ArtifactType::Pypi => "pypi",
            ArtifactType::Crates => "crates",
            ArtifactType::Maven => "maven",
            ArtifactType::Nuget => "nuget",
            ArtifactType::Go => "go",
            ArtifactType::Packagist => "packagist",
            ArtifactType::Rubygems => "rubygems",
        }
    }
}

/// Registry-facing views over the generated wire query.
pub(crate) mod artifact_search_query {
    use super::*;
    pub(crate) fn artifact_type(query: &ArtifactSearchQuery) -> ArtifactType {
        match query {
            ArtifactSearchQuery::NpmExact(_) | ArtifactSearchQuery::NpmDiscovery(_) => {
                ArtifactType::Npm
            }
            ArtifactSearchQuery::RegistryExact(query) => match query.ecosystem {
                RegistryExactEcosystem::Pypi => ArtifactType::Pypi,
                RegistryExactEcosystem::Crates => ArtifactType::Crates,
                RegistryExactEcosystem::Maven => ArtifactType::Maven,
                RegistryExactEcosystem::Nuget => ArtifactType::Nuget,
                RegistryExactEcosystem::Go => ArtifactType::Go,
                RegistryExactEcosystem::Packagist => ArtifactType::Packagist,
                RegistryExactEcosystem::Rubygems => ArtifactType::Rubygems,
            },
            ArtifactSearchQuery::RegistryDiscovery(query) => match query.ecosystem {
                RegistryDiscoveryEcosystem::Crates => ArtifactType::Crates,
                RegistryDiscoveryEcosystem::Maven => ArtifactType::Maven,
                RegistryDiscoveryEcosystem::Nuget => ArtifactType::Nuget,
                RegistryDiscoveryEcosystem::Go => ArtifactType::Go,
                RegistryDiscoveryEcosystem::Packagist => ArtifactType::Packagist,
                RegistryDiscoveryEcosystem::Rubygems => ArtifactType::Rubygems,
            },
        }
    }
    pub(crate) fn package_name(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::NpmExact(query) => Some(query.package_name.as_str()),
            ArtifactSearchQuery::RegistryExact(query) => Some(query.package_name.as_str()),
            _ => None,
        }
    }
    /// The requested version, range, or tag: the `version` field, else the
    /// registry's own coordinate suffix (`name@x` on npm/crates, `name==x`
    /// on PyPI), which stays valid input. `None` means the latest release.
    pub(crate) fn version(query: &ArtifactSearchQuery) -> Option<&str> {
        let explicit = match query {
            ArtifactSearchQuery::NpmExact(query) => query.version.as_deref().map(String::as_str),
            ArtifactSearchQuery::RegistryExact(query) => {
                query.version.as_deref().map(String::as_str)
            }
            _ => None,
        };
        explicit
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| split_coordinate(query).1)
    }

    /// The package name without a coordinate version suffix.
    pub(crate) fn bare_package_name(query: &ArtifactSearchQuery) -> Option<&str> {
        package_name(query).map(|_| split_coordinate(query).0)
    }

    fn split_coordinate(query: &ArtifactSearchQuery) -> (&str, Option<&str>) {
        let Some(name) = package_name(query) else {
            return ("", None);
        };
        match artifact_type(query) {
            ArtifactType::Npm => crate::providers::artifact::npm::split_npm_coordinate(name),
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

    /// Whether the query's `extras` names `kind`.
    pub(crate) fn wants(
        query: &ArtifactSearchQuery,
        kind: octocode_contract_types::ExtraKind,
    ) -> bool {
        match query {
            ArtifactSearchQuery::NpmExact(query) => query.extras.contains(&kind),
            ArtifactSearchQuery::NpmDiscovery(query) => query.extras.contains(&kind),
            ArtifactSearchQuery::RegistryExact(query) => query.extras.contains(&kind),
            ArtifactSearchQuery::RegistryDiscovery(query) => query.extras.contains(&kind),
        }
    }

    pub fn registry_url(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::NpmExact(query) => query.registry_url.as_deref(),
            ArtifactSearchQuery::NpmDiscovery(query) => query.registry_url.as_deref(),
            _ => None,
        }
    }
    /// The 1-based discovery page (`page`); exact lookups have one.
    pub(crate) fn page(query: &ArtifactSearchQuery) -> u64 {
        match query {
            ArtifactSearchQuery::NpmDiscovery(query) => query.page.map_or(1, |page| page.get()),
            ArtifactSearchQuery::RegistryDiscovery(query) => {
                query.page.map_or(1, |page| page.get())
            }
            _ => 1,
        }
    }
    /// Points a discovery query at `page`. A cursor names the start of one
    /// page, so it is cleared with the page it named.
    pub(crate) fn set_page(query: &mut ArtifactSearchQuery, page: u64) {
        let page = std::num::NonZeroU64::new(page);
        match query {
            ArtifactSearchQuery::NpmDiscovery(query) => (query.page, query.cursor) = (page, None),
            ArtifactSearchQuery::RegistryDiscovery(query) => {
                (query.page, query.cursor) = (page, None)
            }
            _ => {}
        }
    }
    /// The opaque registry cursor that starts this discovery page
    /// (`cursor`, copied from `next.nextPage`).
    pub(crate) fn cursor(query: &ArtifactSearchQuery) -> Option<&str> {
        match query {
            ArtifactSearchQuery::NpmDiscovery(query) => query.cursor.as_deref().map(String::as_str),
            ArtifactSearchQuery::RegistryDiscovery(query) => {
                query.cursor.as_deref().map(String::as_str)
            }
            _ => None,
        }
    }
    /// Names the cursor that starts this discovery page. A cursor the
    /// contract's bound rejects is left off: the page then walks to its
    /// start instead of carrying an input the tool would refuse.
    pub(crate) fn set_cursor(query: &mut ArtifactSearchQuery, cursor: Option<&str>) {
        match query {
            ArtifactSearchQuery::NpmDiscovery(query) => {
                query.cursor = cursor.and_then(|text| text.parse().ok())
            }
            ArtifactSearchQuery::RegistryDiscovery(query) => {
                query.cursor = cursor.and_then(|text| text.parse().ok());
            }
            _ => {}
        }
    }
    /// Rows per page: a discovery query's `pageSize` (absent, the contract
    /// default); an exact lookup, one page, at the contract default.
    pub(crate) fn page_size(query: &ArtifactSearchQuery) -> usize {
        match query {
            ArtifactSearchQuery::NpmDiscovery(query) => query.page_size.get() as usize,
            ArtifactSearchQuery::RegistryDiscovery(query) => query.page_size.get() as usize,
            _ => crate::contracts::query_limits::artifact_search::PAGE_SIZE_DEFAULT,
        }
    }
    /// A discovery query's keywords as written; an exact lookup has none.
    pub(crate) fn keyword_list(query: &ArtifactSearchQuery) -> Vec<String> {
        match query {
            ArtifactSearchQuery::NpmDiscovery(query) => {
                query.keywords.iter().map(ToString::to_string).collect()
            }
            ArtifactSearchQuery::RegistryDiscovery(query) => {
                query.keywords.iter().map(ToString::to_string).collect()
            }
            _ => Vec::new(),
        }
    }
    /// Keywords as one space-joined registry search text.
    pub(crate) fn terms(query: &ArtifactSearchQuery) -> String {
        keyword_list(query).join(" ")
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
        || serde_json::json!({"ecosystem": "npm", "mainGoal": "test", "reasoning": "test"}),
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

#[test]
fn page_size_defaults_from_the_contract() {
    use crate::contracts::query_limits::artifact_search::PAGE_SIZE_DEFAULT;
    let discovery = artifact_query(serde_json::json!({"keywords": ["http"]}), None);
    let exact = artifact_query(serde_json::json!({"packageName": "ajv"}), None);
    let sized = artifact_query(serde_json::json!({"pageSize": 3}), Some(&discovery));
    assert_eq!(
        artifact_search_query::page_size(&discovery),
        PAGE_SIZE_DEFAULT
    );
    assert_eq!(artifact_search_query::page_size(&exact), PAGE_SIZE_DEFAULT);
    assert_eq!(artifact_search_query::page_size(&sized), 3);
}

/// A cursor names one page's start: moving the page clears it, and a
/// cursor past the contract's bound is left off rather than carried.
#[cfg(test)]
#[test]
fn a_cursor_belongs_to_its_page_and_the_contract_bounds_it() {
    use crate::contracts::query_limits::artifact_search::CURSOR_MAX_LENGTH;
    let mut query = artifact_query(
        serde_json::json!({"ecosystem": "go", "keywords": ["http"], "page": 2, "cursor": "c2"}),
        None,
    );
    assert_eq!(artifact_search_query::cursor(&query), Some("c2"));
    artifact_search_query::set_page(&mut query, 3);
    assert_eq!(
        (
            artifact_search_query::page(&query),
            artifact_search_query::cursor(&query)
        ),
        (3, None)
    );
    artifact_search_query::set_cursor(&mut query, Some("c3"));
    assert_eq!(artifact_search_query::cursor(&query), Some("c3"));
    artifact_search_query::set_cursor(&mut query, Some(&"x".repeat(CURSOR_MAX_LENGTH + 1)));
    assert_eq!(artifact_search_query::cursor(&query), None);
    let exact = artifact_query(serde_json::json!({"packageName": "ajv"}), None);
    assert_eq!(artifact_search_query::cursor(&exact), None);
}

/// An exact lookup of `name` at `version`.
#[cfg(test)]
pub(crate) fn versioned(
    artifact_type: ArtifactType,
    name: &str,
    version: &str,
) -> ArtifactSearchQuery {
    artifact_query(
        serde_json::json!({"ecosystem": artifact_type, "packageName": name, "version": version}),
        None,
    )
}

/// An exact lookup of `name` (the latest release).
#[cfg(test)]
pub(crate) fn exact_query(artifact_type: ArtifactType, name: &str) -> ArtifactSearchQuery {
    artifact_query(
        serde_json::json!({"ecosystem": artifact_type, "packageName": name.to_string()}),
        None,
    )
}

/// A `json` keyword discovery query with `fields` set.
#[cfg(test)]
pub(crate) fn discovery(
    artifact_type: ArtifactType,
    fields: serde_json::Value,
) -> ArtifactSearchQuery {
    let mut query = serde_json::json!({"ecosystem": artifact_type, "keywords": ["json"]});
    for (key, value) in fields.as_object().into_iter().flatten() {
        query[key] = value.clone();
    }
    artifact_query(query, None)
}

/// A request budget no artifact test exhausts.
#[cfg(test)]
pub(crate) fn test_budget() -> octocode_io::budget::RequestBudget {
    octocode_io::budget::RequestBudget::with_timeout(std::time::Duration::from_secs(10), 10_000_000)
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
    /// This version's runtime dependencies as the registry declares them
    /// (`name@range`, a PEP 508 requirement, `name req`): empty when it
    /// declares none, absent when the registry read did not say.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,
    /// Optional dependencies as declared, in the `dependencies` format (npm
    /// `optionalDependencies`, PyPI extras, optional crate and Maven
    /// dependencies); absent when the version declares none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional_dependencies: Option<Vec<String>>,
    /// npm peer dependencies (`name@range`) as declared; absent when the
    /// version declares none (its `dependencies` list shows the manifest
    /// was read) and for a registry with no such field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_dependencies: Option<Vec<String>>,
    /// npm `engines`: every runtime or tool the package declares, with its
    /// range (`node`, `npm`, `pnpm`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engines: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_python: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rust_version: Option<String>,
    /// npm discovery: downloads in the last month.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads_monthly: Option<u64>,
    /// Commit or tag this version was published from; it pins the
    /// `viewReleaseSource` lead. `verification` says how it was checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref: Option<String>,
    /// How the release source is known: `provenance` (attested), `tag`
    /// (upstream tag checked to exist), `registryRef` (the registry said so,
    /// unchecked), or `defaultBranch` (no release ref; default-branch code).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<&'static str>,
    /// `source_ref` came from a provenance attestation bound to this
    /// tarball and repository, not from an unchecked registry field.
    #[serde(skip)]
    pub source_attested: bool,
    /// `source_ref` is an upstream release tag checked to exist.
    #[serde(skip)]
    pub source_tag: bool,
    /// The registry's release ref that GitHub does not have (an unpushed
    /// or rewritten commit): no lead pins it; the response warns with it.
    #[serde(skip)]
    pub missing_ref: Option<String>,
    /// `${…}` properties the declared dependencies name that the release's
    /// own metadata does not define (a Maven parent POM's): the response
    /// states them.
    #[serde(skip)]
    pub unresolved_properties: Vec<String>,
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
            optional_dependencies: None,
            peer_dependencies: None,
            engines: None,
            requires_python: None,
            rust_version: None,
            downloads_monthly: None,
            source_ref: None,
            verification: None,
            source_attested: false,
            source_tag: false,
            missing_ref: None,
            unresolved_properties: Vec::new(),
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

/// Where a registry stops serving a discovery: the machine-readable
/// `partialReasons` code and the warning stating this call's numbers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalLimit {
    pub reason: &'static str,
    pub warning: String,
}

impl TerminalLimit {
    /// The registry serves no result past a fixed depth.
    pub(crate) fn result_cap(warning: String) -> Self {
        Self {
            reason: "providerResultCap",
            warning,
        }
    }

    /// The registry answered an empty page before its own reported total.
    pub(crate) fn short_page(registry: &str) -> Self {
        Self {
            reason: "providerIncompleteResults",
            warning: format!("{registry} returned an empty page before its reported total."),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactProviderPage {
    pub artifacts: Vec<ArtifactItem>,
    pub next_state: Option<ArtifactProviderState>,
    pub total: Option<u64>,
    pub terminal_limit: Option<TerminalLimit>,
    pub(crate) registry: Option<String>,
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

    /// The one artifact an exact lookup resolved.
    pub(crate) fn single(artifact: ArtifactItem) -> Self {
        Self {
            artifacts: vec![artifact],
            next_state: None,
            total: Some(1),
            terminal_limit: None,
            registry: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// The repair lead (`next`) of the failure, when one call fixes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<serde_json::Value>>,
    /// Published versions nearest a requested one that does not exist: the
    /// tool's repair lead reads them.
    #[serde(skip)]
    pub nearest: Vec<String>,
    /// The package name is no exact coordinate of the registry but a search
    /// term (a bare Maven artifact name): the tool's repair lead searches it
    /// as keywords.
    #[serde(skip)]
    pub searchable_name: bool,
}

impl ArtifactError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            status: None,
            hints: vec![],
            next: None,
            nearest: vec![],
            searchable_name: false,
        }
    }

    pub(crate) fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
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
    /// The public npm registry (`npmjs` in cache keys), without credentials.
    pub(crate) fn npmjs() -> Self {
        static BASE: std::sync::LazyLock<Url> = std::sync::LazyLock::new(|| {
            #[expect(clippy::expect_used, reason = "static URL, valid by construction")]
            Url::parse("https://registry.npmjs.org/").expect("npm registry URL")
        });
        Self {
            base: BASE.clone(),
            authorization: None,
            cache_identity: "npmjs".into(),
        }
    }

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
