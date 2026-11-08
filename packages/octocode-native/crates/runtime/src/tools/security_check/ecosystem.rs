//! The OSV ecosystem a package row names. The generated enum is the contract's
//! own (`ecosystem` of the `package` row) and its `Display` is OSV's
//! case-sensitive name. Which ecosystems are distributions and which have a
//! registry reader comes from the contract's `security` block; the registry
//! provider each reader maps to is the only fact native adds.
use crate::contracts::security::security_contract;
use crate::providers::artifact::ArtifactError;
use octocode_contract_types::{ArtifactType, ErrorCode, SecurityCheckQueryPackageEcosystem};

pub(crate) type Ecosystem = SecurityCheckQueryPackageEcosystem;

/// The registry provider behind `ecosystem`, as the provider names it.
const fn provider(ecosystem: Ecosystem) -> Option<ArtifactType> {
    match ecosystem {
        Ecosystem::Npm => Some(ArtifactType::Npm),
        Ecosystem::PyPi => Some(ArtifactType::Pypi),
        Ecosystem::CratesIo => Some(ArtifactType::Crates),
        Ecosystem::Maven => Some(ArtifactType::Maven),
        Ecosystem::NuGet => Some(ArtifactType::Nuget),
        Ecosystem::Go => Some(ArtifactType::Go),
        Ecosystem::Packagist => Some(ArtifactType::Packagist),
        Ecosystem::RubyGems => Some(ArtifactType::Rubygems),
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

/// What OSV is asked: the ecosystem, then `:release` for a distribution release.
pub(super) fn osv_name(ecosystem: Ecosystem, release: Option<&str>) -> String {
    match release {
        Some(release) => format!("{ecosystem}:{release}"),
        None => ecosystem.to_string(),
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
mod tests {
    use super::*;
    use crate::contracts::parsed_contract;
    use std::collections::BTreeSet;

    fn enum_values() -> Vec<String> {
        let schema = parsed_contract().expect("contract")["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool["name"] == "securityCheck")
            .expect("securityCheck")["querySchema"]
            .to_string();
        let values: serde_json::Value = serde_json::from_str(&schema).expect("schema");
        let mut found = Vec::new();
        collect_ecosystem_enum(&values, &mut found);
        collect_ecosystem_enum(&parsed_contract().expect("contract")["$defs"], &mut found);
        found.sort();
        found.dedup();
        found
    }

    /// Every enum in the schema that lists the distros: the `ecosystem` values.
    fn collect_ecosystem_enum(node: &serde_json::Value, found: &mut Vec<String>) {
        match node {
            serde_json::Value::Object(map) => {
                if let Some(values) = map.get("enum").and_then(|values| values.as_array())
                    && values.iter().any(|value| value == "Debian")
                {
                    found.extend(values.iter().filter_map(|v| v.as_str().map(str::to_owned)));
                }
                map.values().for_each(|v| collect_ecosystem_enum(v, found));
            }
            serde_json::Value::Array(items) => {
                items.iter().for_each(|v| collect_ecosystem_enum(v, found));
            }
            _ => {}
        }
    }

    fn every_ecosystem() -> Vec<Ecosystem> {
        enum_values()
            .iter()
            .map(|name| serde_json::from_value(serde_json::json!(name)).expect("an enum value"))
            .collect()
    }

    /// N2: the contract's registry readers are exactly the ecosystems this
    /// module maps to a provider, and every distro and reader is an enum value.
    #[test]
    fn the_contract_s_readers_are_the_native_reader_set() {
        let contract = security_contract().expect("security block");
        let enum_names: BTreeSet<String> = enum_values().into_iter().collect();
        assert!(!enum_names.is_empty());
        for name in contract
            .ecosystems
            .distros
            .iter()
            .chain(&contract.ecosystems.registry_readers)
        {
            assert!(
                enum_names.contains(name),
                "{name} is not an ecosystem value"
            );
        }
        let native: BTreeSet<String> = every_ecosystem()
            .into_iter()
            .filter(|ecosystem| provider(*ecosystem).is_some())
            .map(|ecosystem| ecosystem.to_string())
            .collect();
        let declared: BTreeSet<String> = contract
            .ecosystems
            .registry_readers
            .iter()
            .cloned()
            .collect();
        assert_eq!(native, declared);
        for ecosystem in every_ecosystem() {
            assert_eq!(
                registry(ecosystem).expect("contract").is_some(),
                declared.contains(&ecosystem.to_string())
            );
        }
    }

    #[test]
    fn display_is_the_osv_name_and_a_release_is_joined_with_a_colon() {
        for ecosystem in every_ecosystem() {
            assert!(enum_values().contains(&ecosystem.to_string()));
            assert_eq!(osv_name(ecosystem, None), ecosystem.to_string());
            assert_eq!(osv_name(ecosystem, Some("r")), format!("{ecosystem}:r"));
        }
    }

    #[test]
    fn a_release_is_accepted_on_distros_with_releases_and_refused_elsewhere() {
        for ecosystem in every_ecosystem() {
            let outcome = check_release(ecosystem, Some("12"));
            if takes_release(ecosystem).expect("contract") {
                assert!(outcome.is_ok(), "{ecosystem}");
            } else {
                assert_eq!(outcome.expect_err("refused").code, ErrorCode::InvalidInput);
            }
            assert!(check_release(ecosystem, None).is_ok());
        }
    }

    #[test]
    fn rolling_distributions_are_distributions_that_take_no_release() {
        let rolling = &security_contract().expect("contract").ecosystems.rolling;
        assert!(!rolling.is_empty());
        for name in rolling {
            let ecosystem: Ecosystem =
                serde_json::from_value(serde_json::json!(name)).expect("an ecosystem");
            assert!(
                security_contract()
                    .expect("contract")
                    .is_distro(&ecosystem.to_string()),
                "{ecosystem}"
            );
            assert!(!takes_release(ecosystem).expect("contract"), "{ecosystem}");
            let error = check_release(ecosystem, Some("v1")).expect_err("refused");
            assert_eq!(error.code, ErrorCode::InvalidInput);
            assert!(!error.message.contains("Wolfi,"), "{}", error.message);
        }
    }

    #[test]
    fn registries_round_trip_through_their_ecosystem() {
        for ecosystem in every_ecosystem() {
            if let Some(kind) = provider(ecosystem) {
                assert_eq!(
                    crate::providers::artifact::query::artifact_type::osv_ecosystem(kind),
                    ecosystem
                );
            }
        }
    }
}
