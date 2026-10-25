//! The rows, pages, state and errors every registry provider returns, and
//! the resolved npm registry a lookup runs against.
use octocode_contract_types::{ArtifactType, ErrorCode};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use url::Url;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactItem {
    /// The queried registry; the request states it, so rows omit it.
    #[serde(skip)]
    pub artifact_type: ArtifactType,
    pub package_name: String,
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
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
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
            package_name: name,
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
    pub calls: Option<Box<serde_json::Value>>,
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
            calls: None,
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
