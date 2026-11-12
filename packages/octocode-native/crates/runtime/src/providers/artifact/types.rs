//! The rows, pages, state and errors every registry provider returns, and
//! the resolved npm registry a lookup runs against.
use octocode_contract_types::{ArtifactSearchRowEnginesItem, ArtifactType, ErrorCode};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use std::fmt;
use url::Url;

mod advisory;

pub use advisory::Advisory;
pub(crate) use advisory::alias_groups;

#[derive(Clone, Debug, PartialEq, Serialize)]
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
    /// Why the registry says this version was yanked, when it says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yanked_reason: Option<String>,
    /// Advisories the registry lists for this version: empty when it lists
    /// none, absent when the registry gives no advisory data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisories: Option<Vec<Advisory>>,
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
    /// range (`node`, `npm`, `pnpm`, …), as rows: a name is data, never a
    /// JSON key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engines: Option<Vec<ArtifactSearchRowEnginesItem>>,
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
    /// `source_ref` is the release tag the package's metadata declares (a
    /// Maven `<scm><tag>`), which exists upstream but names no tag of this
    /// version: pinned as the registry's claim, and the response says so.
    #[serde(skip)]
    pub declared_ref: bool,
    /// The registry's release ref that GitHub does not have (an unpushed
    /// or rewritten commit): no lead pins it; the response warns with it.
    #[serde(skip)]
    pub missing_ref: Option<String>,
    /// The upstream release tag of this version, when it names a commit other
    /// than the registry's `ref`: the response warns with it.
    #[serde(skip)]
    pub tag_differs: Option<super::ReleaseTag>,
    /// GitHub answered for the repository the registry names: it is gone, or
    /// reached through a redirect (renamed). The response warns with it.
    #[serde(skip)]
    pub repository_status: Option<RepositoryStatus>,
    /// Where the project moved, as `owner/repo`: the repository GitHub
    /// redirects the registry's link to (`Moved`), or, when GitHub has no
    /// repository for this row (`Missing`), the one the registry's latest
    /// release names. The response warns with it.
    #[serde(skip)]
    pub successor_repository: Option<String>,
    /// What limits the response's "no tag names this version" claim when
    /// no tag of this package named the release.
    #[serde(skip)]
    pub tag_absence: Option<TagAbsence>,
    /// `${…}` properties the declared dependencies name that the release's
    /// own metadata does not define (a Maven parent POM's): the response
    /// states them.
    #[serde(skip)]
    pub unresolved_properties: Vec<String>,
    /// The registry lists no stable release, so an unversioned lookup named a
    /// prerelease: the response warns with it.
    #[serde(skip)]
    pub only_prerelease: bool,
    /// The module's latest `go.mod` retracts this Go version: the response
    /// warns with the reason it gives.
    #[serde(skip)]
    pub retraction: Option<Retraction>,
    /// What a provider could not read or settle while building the row, each
    /// stated whole: the response adds every one as a warning, so a missing
    /// fact reads as unknown, never as absent.
    #[serde(skip)]
    pub disclosures: Vec<String>,
    /// The page path of a source link into a GitHub `tree`/`blob`, whose
    /// directory is not placed until the ref's extent is known.
    #[serde(skip)]
    pub(crate) link_below_ref: Option<LinkBelowRef>,
}

/// A source link's path below `tree`/`blob`: the ref and the directory
/// below it, as one run of segments a ref's own `/` may span.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LinkBelowRef {
    /// The link as the registry gave it.
    pub link: String,
    /// At least two segments: the ref's first, then the rest.
    pub segments: Vec<String>,
}

/// A retracted Go version's retraction, with the rationale its module
/// gives (a comment on the `retract` directive), when it gives one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Retraction {
    pub reason: Option<String>,
}

/// What limits a row's "no tag names this version" claim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TagAbsence {
    /// The whole tag list was read: only these tags, another package's,
    /// name the version.
    OtherPackagesOnly(Vec<String>),
    /// The candidate shapes were checked, and the tag list could not be read
    /// or resolved, for this reason.
    ListUnread(String),
    /// No tag could be checked, for this reason.
    Unchecked(String),
}

/// What GitHub says of the repository a registry row names, when it says
/// anything but "there".
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryStatus {
    /// GitHub has no such repository (deleted, private, or never there).
    Missing,
    /// GitHub redirects it to another name: renamed or transferred.
    Moved,
}

impl ArtifactItem {
    /// Records what a commit check of the row's repository showed about the
    /// repository itself. A repository that is gone names no ref a lead can
    /// pin, so the row keeps none.
    pub(crate) fn note_repository(&mut self, check: super::release_ref::CommitCheck) {
        use super::release_ref::CommitCheck;
        match check {
            CommitCheck::RepositoryMissing => {
                self.repository_status = Some(RepositoryStatus::Missing);
                self.source_ref = None;
                self.source_tag = false;
            }
            CommitCheck::FoundAfterRedirect { to } => {
                self.repository_status = Some(RepositoryStatus::Moved);
                self.successor_repository = to;
            }
            CommitCheck::Found | CommitCheck::Missing => {}
        }
    }

    /// States a GitHub check of `what` that gave no answer, so the row's
    /// state is unknown, not fine. A link to no GitHub repository and a call
    /// without GitHub access are the lookup's own scope, not a failure to
    /// report on every row.
    pub(crate) fn note_unchecked(&mut self, what: &str, failure: super::release_ref::CheckFailure) {
        use super::release_ref::CheckFailure;
        if matches!(failure, CheckFailure::NotGitHub | CheckFailure::Unavailable) {
            return;
        }
        self.disclosures.push(format!(
            "{}: {what} could not be checked on GitHub ({}), so what it shows is unknown, not confirmed.",
            self.package_name,
            failure.describe()
        ));
    }

    /// Takes the source link a registry gives (a repository, or a page of
    /// one) as the row's repository. A link into a GitHub page below a ref
    /// cannot place its directory yet, since a ref may contain `/`: the page
    /// path waits in `link_below_ref` for [`super::link_directory::settle`].
    pub(crate) fn set_source_link(&mut self, link: &str) {
        let Some(found) = super::util::source_link(link) else {
            return;
        };
        self.repository = Some(found.repository);
        self.link_below_ref = found.ref_path.map(|segments| LinkBelowRef {
            link: link.to_owned(),
            segments,
        });
    }

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
            yanked_reason: None,
            advisories: None,
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
            declared_ref: false,
            tag_differs: None,
            missing_ref: None,
            repository_status: None,
            successor_repository: None,
            tag_absence: None,
            unresolved_properties: Vec::new(),
            only_prerelease: false,
            retraction: None,
            disclosures: Vec::new(),
            link_below_ref: None,
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
    pub reason: crate::response::paging::PartialReason,
    pub warning: String,
}

impl TerminalLimit {
    /// The registry serves no result past a fixed depth.
    pub(crate) fn result_cap(warning: String) -> Self {
        Self {
            reason: crate::response::paging::PartialReason::ProviderResultCap,
            warning,
        }
    }

    /// The registry answered an empty page before its own reported total.
    pub(crate) fn short_page(registry: &str) -> Self {
        Self {
            reason: crate::response::paging::PartialReason::ProviderIncompleteResults,
            warning: format!("{registry} returned an empty page before its reported total."),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
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
    /// `nearest` ranks every version the registry publishes, so fewer than
    /// one call reads means there are no more to suggest.
    #[serde(skip)]
    pub nearest_ranked: bool,
    /// The package name is no exact coordinate of the registry but a search
    /// term (a bare Maven artifact name): the tool's repair lead searches it
    /// as keywords.
    #[serde(skip)]
    pub searchable_name: bool,
    /// The package exists and the requested version does not: a `notFound`
    /// whose repair reads another version, never a search for the name.
    #[serde(skip)]
    pub missing_version: bool,
}

/// The recovery of a version a known package does not publish.
const VERSION_NOT_FOUND_HINT: &str = "Version not found: check the registry and manifest/lockfile version; discover published versions before choosing another version.";

impl ArtifactError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            status: None,
            hints: vec![],
            calls: None,
            nearest: vec![],
            nearest_ranked: false,
            searchable_name: false,
            missing_version: false,
        }
    }

    /// A known package without the requested version (`notFound`), with the
    /// recovery the message alone does not state.
    pub(crate) fn version_not_found(message: impl Into<String>) -> Self {
        Self {
            hints: vec![VERSION_NOT_FOUND_HINT.to_owned()],
            missing_version: true,
            ..Self::new(ErrorCode::NotFound, message).with_status(404)
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
}

impl ResolvedNpmRegistry {
    /// The public npm registry, without credentials.
    pub(crate) fn npmjs() -> Self {
        static BASE: std::sync::LazyLock<Url> = std::sync::LazyLock::new(|| {
            #[expect(clippy::expect_used, reason = "static URL, valid by construction")]
            Url::parse("https://registry.npmjs.org/").expect("npm registry URL")
        });
        Self {
            base: BASE.clone(),
            authorization: None,
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
            .finish()
    }
}
