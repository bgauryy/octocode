//! The ecosystem a package row names. The generated enum is the contract's
//! own (`ecosystem` of the `package` row) and its `Display` is the input's
//! spelling: a registry artifactSearch reads as artifactSearch spells it, any
//! other as OSV does. What OSV is asked and states is OSV's case-sensitive
//! name ([`osv`], [`as_input`]), from the contract's `osvNames`. Which
//! ecosystems are distributions and which have a registry reader comes from
//! the contract's `security` block; the registry provider each reader maps to
//! is the only fact native adds.
use crate::artifact::ArtifactError;
use octocode_contract_types::{ArtifactType, ErrorCode, SecurityCheckQueryPackageEcosystem};
use octocode_runtime_base::contracts::security::security_contract;

pub(crate) type Ecosystem = SecurityCheckQueryPackageEcosystem;

/// The registry provider behind `ecosystem`, as the provider names it.
const fn provider(ecosystem: Ecosystem) -> Option<ArtifactType> {
    match ecosystem {
        Ecosystem::Npm => Some(ArtifactType::Npm),
        Ecosystem::Pypi => Some(ArtifactType::Pypi),
        Ecosystem::Crates => Some(ArtifactType::Crates),
        Ecosystem::Maven => Some(ArtifactType::Maven),
        Ecosystem::Nuget => Some(ArtifactType::Nuget),
        Ecosystem::Go => Some(ArtifactType::Go),
        Ecosystem::Packagist => Some(ArtifactType::Packagist),
        Ecosystem::Rubygems => Some(ArtifactType::Rubygems),
        Ecosystem::Pub
        | Ecosystem::Hex
        | Ecosystem::SwiftUrl
        | Ecosystem::GitHubActions
        | Ecosystem::Cran
        | Ecosystem::Bioconductor
        | Ecosystem::Debian
        | Ecosystem::Ubuntu
        | Ecosystem::Alpine
        | Ecosystem::RockyLinux
        | Ecosystem::AlmaLinux
        | Ecosystem::RedHat
        | Ecosystem::Suse
        | Ecosystem::OpenSuse
        | Ecosystem::Wolfi
        | Ecosystem::Chainguard
        | Ecosystem::Alpaquita
        | Ecosystem::Mageia
        | Ecosystem::OpenEuler
        | Ecosystem::AzureLinux => None,
    }
}

fn contract_failure(why: &str) -> ArtifactError {
    ArtifactError::new(
        ErrorCode::ProviderError,
        format!("The embedded security contract is unreadable: {why}."),
    )
}

/// How `ecosystem` orders its versions, when a registry provider reads the
/// ecosystem or its package manager's order is known (dpkg, apk): the order
/// its fixed versions compare in.
pub(super) fn version_order(ecosystem: Ecosystem) -> Option<crate::artifact::VersionOrder> {
    match ecosystem {
        Ecosystem::Debian | Ecosystem::Ubuntu => Some(super::distro_order::DPKG),
        Ecosystem::Alpine => Some(super::distro_order::APK),
        _ => provider(ecosystem).map(crate::artifact::order_of),
    }
}

/// The registry that confirms this ecosystem's names, when the contract says
/// one is read. `None`: the name goes to OSV as given.
pub(super) fn registry(ecosystem: Ecosystem) -> Result<Option<ArtifactType>, ArtifactError> {
    let contract = security_contract().map_err(contract_failure)?;
    Ok(contract
        .has_registry_reader(&ecosystem.to_string())
        .then(|| provider(ecosystem))
        .flatten())
}

/// Whether `ecosystem` matches versions per release: a Linux distribution that
/// has releases, not one the contract lists as a rolling stream (a `release`
/// names nothing there, and OSV answers `Wolfi:foo` with no rows).
pub(super) fn takes_release(ecosystem: Ecosystem) -> Result<bool, ArtifactError> {
    let contract = security_contract().map_err(contract_failure)?;
    let name = ecosystem.to_string();
    Ok(contract.is_distro(&name) && !contract.is_rolling(&name))
}

/// Whether a version asked of `ecosystem` with no release is not evaluated: it
/// takes releases, and the contract does not list its bare name as evaluating
/// a version (`bareVersions`).
pub(super) fn needs_release(ecosystem: Ecosystem) -> Result<bool, ArtifactError> {
    let contract = security_contract().map_err(contract_failure)?;
    Ok(takes_release(ecosystem)? && !contract.evaluates_bare(&ecosystem.to_string()))
}

/// Whether OSV orders versions of `ecosystem`. When it does not, a version
/// query matches none of its records, so OSV is asked by name and each record's
/// ranges are compared here.
pub(super) fn osv_orders_versions(ecosystem: Ecosystem) -> Result<bool, ArtifactError> {
    Ok(security_contract()
        .map_err(contract_failure)?
        .orders_versions(&ecosystem.to_string()))
}

/// The ecosystem the contract names `name`.
fn named(name: &str) -> Result<Ecosystem, ArtifactError> {
    serde_json::from_value(serde_json::json!(name))
        .map_err(|_| contract_failure(&format!("{name} is not an ecosystem value")))
}

/// A name as a purl namespace spells it: lower case, letters and digits only.
fn folded(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

/// The purl types the contract maps to an ecosystem.
pub(super) fn purl_types() -> Result<Vec<String>, ArtifactError> {
    Ok(security_contract()
        .map_err(contract_failure)?
        .ecosystems
        .purl_types
        .keys()
        .cloned()
        .collect())
}

/// The ecosystem and package name of a purl's `type/rest`. A type that names
/// several ecosystems is a distribution type: the first segment of `rest` is
/// the distribution (`debian`, `rocky`), the remainder the package. `None`: the
/// type is not in the contract, or the namespace names none of its distributions.
pub(super) fn from_purl(
    kind: &str,
    rest: &str,
) -> Result<Option<(Ecosystem, String)>, ArtifactError> {
    let contract = security_contract().map_err(contract_failure)?;
    let Some(candidates) = contract.ecosystems.purl_types.get(kind) else {
        return Ok(None);
    };
    if let [only] = candidates.as_slice() {
        return Ok(Some((named(only)?, rest.to_owned())));
    }
    let Some((namespace, name)) = rest.split_once('/') else {
        return Ok(None);
    };
    let namespace = folded(namespace);
    for candidate in candidates {
        let candidate_name = folded(candidate);
        if candidate_name == namespace || candidate_name == format!("{namespace}linux") {
            return Ok(Some((named(candidate)?, name.to_owned())));
        }
    }
    Ok(None)
}

/// The `release` a purl's `distro` qualifier (`debian-11`, `alpine-3.14.2`)
/// names for `ecosystem`, spelled as the contract says OSV does. `None`: the
/// ecosystem takes no release, or the qualifier does not lead with its name
/// (`bookworm`), which names no release this can state.
pub(super) fn purl_release(
    ecosystem: Ecosystem,
    distro: &str,
) -> Result<Option<String>, ArtifactError> {
    if !takes_release(ecosystem)? {
        return Ok(None);
    }
    let Some((name, version)) = distro.split_once('-') else {
        return Ok(None);
    };
    let own = folded(&ecosystem.to_string());
    let name = folded(name);
    if version.is_empty() || (own != name && own != format!("{name}linux")) {
        return Ok(None);
    }
    let contract = security_contract().map_err(contract_failure)?;
    Ok(Some(
        match contract
            .ecosystems
            .purl_releases
            .get(&ecosystem.to_string())
        {
            Some(form) => {
                let parts = version.split('.').take(form.parts).collect::<Vec<_>>();
                format!("{}{}", form.prefix, parts.join("."))
            }
            None => version.to_owned(),
        },
    ))
}

/// OSV's own name of `ecosystem`: what OSV is asked and what its records
/// state (`pypi` is `PyPI`). An ecosystem the contract gives no OSV name is
/// already spelled as OSV spells it.
pub(super) fn osv(ecosystem: Ecosystem) -> String {
    let input = ecosystem.to_string();
    match security_contract() {
        Ok(contract) => contract.osv_name_of(&input).to_owned(),
        Err(_) => input,
    }
}

/// An ecosystem a source states (OSV's name, with any `:release` suffix), as
/// the `ecosystem` input spells it: what a row shows, so it runs unchanged as
/// the next call's `ecosystem`. The release suffix stays as stated.
pub(super) fn as_input(stated: &str) -> String {
    let (name, release) = match stated.split_once(':') {
        Some((name, release)) => (name, Some(release)),
        None => (stated, None),
    };
    let name = match security_contract() {
        Ok(contract) => contract.input_of_osv(name),
        Err(_) => name,
    };
    match release {
        Some(release) => format!("{name}:{release}"),
        None => name.to_owned(),
    }
}

/// What OSV is asked: its name of the ecosystem, then `:release` for a
/// distribution release.
pub(super) fn osv_name(ecosystem: Ecosystem, release: Option<&str>) -> String {
    match release {
        Some(release) => format!("{}:{release}", osv(ecosystem)),
        None => osv(ecosystem),
    }
}

/// A `release` is refused off the distributions, before anything is sent.
pub(super) fn check_release(
    ecosystem: Ecosystem,
    release: Option<&str>,
) -> Result<(), ArtifactError> {
    let contract = security_contract().map_err(contract_failure)?;
    if release.is_none() || takes_release(ecosystem)? {
        return Ok(());
    }
    let with_releases = contract
        .ecosystems
        .distros
        .iter()
        .filter(|name| !contract.is_rolling(name))
        .cloned()
        .collect::<Vec<_>>();
    Err(ArtifactError::new(
        ErrorCode::InvalidInput,
        format!(
            "release applies to Linux distributions that have releases ({}), not {ecosystem}: nothing was sent.",
            with_releases.join(", ")
        ),
    ))
}

#[cfg(test)]
#[path = "ecosystem_tests.rs"]
mod tests;
