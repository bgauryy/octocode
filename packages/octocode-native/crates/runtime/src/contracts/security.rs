//! What `securityCheck` reads from the embedded contract's `security` block:
//! the trust thresholds, each caution rule's level and basis, and which OSV
//! ecosystems are Linux distributions, have a registry reader or cannot be ordered
//! by OSV, and which ecosystems a purl type names. Core owns the
//! values; nothing here restates them.

use serde::Deserialize;
use std::collections::BTreeMap;

/// A caution rule's level and basis, as the contract states them.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CautionLevel {
    Review,
    Note,
}

/// Whether a source reported a caution or a threshold inferred it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CautionBasis {
    Reported,
    Heuristic,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CautionRule {
    pub level: CautionLevel,
    pub basis: CautionBasis,
}

/// The thresholds every trust rule applies; each row that used them discloses them.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecurityPolicy {
    pub low_traction_stars: u64,
    pub unmaintained_after_days: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecurityEcosystems {
    pub distros: Vec<String>,
    /// Distributions with one rolling stream and no releases: a `release` names nothing.
    pub rolling: Vec<String>,
    /// Distributions whose bare name, with no release, still evaluates a version.
    pub bare_versions: Vec<String>,
    pub registry_readers: Vec<String>,
    /// Ecosystems whose versions OSV cannot order: its version query matches none of their records.
    pub unordered: Vec<String>,
    /// The ecosystems each purl type names; several: the purl's namespace picks the distribution.
    pub purl_types: BTreeMap<String, Vec<String>>,
    /// The OSV ecosystem of each ecosystem name GitHub's repository advisories use.
    pub github_ecosystems: BTreeMap<String, String>,
    /// How a purl's `distro` version becomes an ecosystem's `release`.
    pub purl_releases: BTreeMap<String, PurlRelease>,
}

/// A release spelled from a purl `distro` version: its first `parts` dotted
/// components after `prefix`.
#[derive(Debug, Deserialize)]
pub(crate) struct PurlRelease {
    #[serde(default)]
    pub prefix: String,
    pub parts: usize,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SecurityContract {
    pub policy: SecurityPolicy,
    pub rules: BTreeMap<String, CautionRule>,
    pub ecosystems: SecurityEcosystems,
}

impl SecurityContract {
    /// Whether `ecosystem` (an OSV name) is a Linux distribution: the only kind that takes a `release`.
    pub(crate) fn is_distro(&self, ecosystem: &str) -> bool {
        self.ecosystems.distros.iter().any(|name| name == ecosystem)
    }

    /// Whether `ecosystem` (an OSV name) is a distribution with one rolling stream and no releases.
    pub(crate) fn is_rolling(&self, ecosystem: &str) -> bool {
        self.ecosystems.rolling.iter().any(|name| name == ecosystem)
    }

    /// Whether OSV evaluates a version asked of `ecosystem` (an OSV name) by its bare name, with no release.
    pub(crate) fn evaluates_bare(&self, ecosystem: &str) -> bool {
        self.ecosystems
            .bare_versions
            .iter()
            .any(|name| name == ecosystem)
    }

    /// Whether a registry provider reads `ecosystem` (an OSV name).
    pub(crate) fn has_registry_reader(&self, ecosystem: &str) -> bool {
        self.ecosystems
            .registry_readers
            .iter()
            .any(|name| name == ecosystem)
    }

    /// Whether OSV can order versions of `ecosystem` (an OSV name): when it cannot, a version
    /// query matches nothing and the records' ranges must be compared by the caller.
    pub(crate) fn orders_versions(&self, ecosystem: &str) -> bool {
        !self
            .ecosystems
            .unordered
            .iter()
            .any(|name| name == ecosystem)
    }

    /// The OSV ecosystem GitHub's advisories name `github` (`pip` is `PyPI`); `None` when it names none.
    pub(crate) fn osv_ecosystem_of_github(&self, github: &str) -> Option<&str> {
        self.ecosystems
            .github_ecosystems
            .get(&github.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// GitHub's advisory ecosystem for the OSV ecosystem `osv` (`PyPI` is `pip`); `None` when
    /// GitHub keeps none for it.
    pub(crate) fn github_ecosystem_of_osv(&self, osv: &str) -> Option<&str> {
        self.ecosystems
            .github_ecosystems
            .iter()
            .find(|(_, name)| name.as_str() == osv)
            .map(|(github, _)| github.as_str())
    }

    /// The level and basis of `rule`; `None` when the contract has no such rule.
    pub(crate) fn rule(&self, rule: &str) -> Option<&CautionRule> {
        self.rules.get(rule)
    }
}

/// The embedded contract's `security` block, parsed once.
pub(crate) fn security_contract() -> Result<&'static SecurityContract, &'static str> {
    static SECURITY: std::sync::OnceLock<Result<SecurityContract, String>> =
        std::sync::OnceLock::new();
    SECURITY
        .get_or_init(|| {
            let block = super::parsed_contract()?
                .get("security")
                .ok_or("the embedded contract has no security block")?;
            SecurityContract::deserialize(block).map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(String::as_str)
}
