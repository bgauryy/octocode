//! The runtime dependencies a registry row pins to one exact version: what
//! installing the package installs, whatever the dependency's own advisories
//! say. Each registry spells a requirement its own way ([`ArtifactItem`]'s
//! `dependencies` keeps it as declared), so each has its grammar here; a
//! requirement that allows more than one version is no pin.
use super::ArtifactItem;
use octocode_contract_types::ArtifactType;

/// A dependency pinned to one version: its registry name and that version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pin {
    pub name: String,
    pub version: String,
}

/// The exact pins among `artifact`'s required runtime dependencies, each
/// dependency once, in declared order. A registry whose requirements are
/// minimums (Go's `go.mod`) pins nothing.
pub fn exact_pins(artifact: &ArtifactItem) -> Vec<Pin> {
    let mut pins: Vec<Pin> = Vec::new();
    for pin in artifact
        .dependencies
        .iter()
        .flatten()
        .filter_map(|entry| pin_of(artifact.artifact_type, entry))
    {
        if !pins.contains(&pin) {
            pins.push(pin);
        }
    }
    pins
}

fn pin_of(kind: ArtifactType, entry: &str) -> Option<Pin> {
    let (name, version) = match kind {
        ArtifactType::Npm => npm(entry)?,
        ArtifactType::Pypi => pypi(entry)?,
        ArtifactType::Crates => crates(entry)?,
        ArtifactType::Maven => maven(entry)?,
        ArtifactType::Nuget => nuget(entry)?,
        ArtifactType::Packagist => packagist(entry)?,
        ArtifactType::Rubygems => rubygems(entry)?,
        ArtifactType::Go => return None,
    };
    Some(Pin {
        name: name.to_owned(),
        version: version.to_owned(),
    })
}

/// A version number alone: dotted numbers with an optional prerelease or
/// build tail, no operator, wildcard or range.
fn is_version(text: &str) -> bool {
    let core = text.strip_prefix('v').unwrap_or(text);
    let number_end = core
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(core.len());
    let (number, tail) = core.split_at(number_end);
    !number.is_empty()
        && number.starts_with(|c: char| c.is_ascii_digit())
        && !number.ends_with('.')
        && !number.contains("..")
        && tail
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+' | '_'))
}

/// `name@1.2.3`, `name@=1.2.3`, `@scope/name@1.2.3`.
fn npm(entry: &str) -> Option<(&str, &str)> {
    let (name, range) = entry
        .rsplit_once('@')
        .filter(|(name, _)| !name.is_empty())?;
    let version = range.trim().trim_start_matches('=').trim();
    is_version(version).then_some((name, version))
}

/// `name==1.2.3`, `name[extra] (==1.2.3) ; marker`.
fn pypi(entry: &str) -> Option<(&str, &str)> {
    let requirement = entry
        .split_once(';')
        .map_or(entry, |(requirement, _)| requirement);
    let name_end = requirement
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')))
        .unwrap_or(requirement.len());
    let (name, rest) = requirement.split_at(name_end);
    let rest = rest.trim();
    let rest = match rest.strip_prefix('[') {
        Some(extras) => extras.split_once(']')?.1.trim(),
        None => rest,
    };
    let rest = rest
        .strip_prefix('(')
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or(rest);
    let version = rest
        .trim()
        .strip_prefix("===")
        .or_else(|| rest.trim().strip_prefix("=="))?
        .trim();
    (!name.is_empty() && is_version(version)).then_some((name, version))
}

/// `name =1.2.3` (cargo), with an optional `; target` suffix.
fn crates(entry: &str) -> Option<(&str, &str)> {
    let entry = entry.split_once(';').map_or(entry, |(entry, _)| entry);
    let (name, requirement) = entry.trim().split_once(' ')?;
    let version = requirement.trim().strip_prefix('=')?.trim();
    is_version(version).then_some((name, version))
}

/// `group:artifact:1.2.3`: the version a POM declares for the dependency.
fn maven(entry: &str) -> Option<(&str, &str)> {
    let (name, version) = entry.rsplit_once(':')?;
    (name.contains(':') && is_version(version)).then_some((name, version))
}

/// `Id [1.2.3]` or `Id [1.2.3, 1.2.3]`, with an optional ` (framework, ...)`.
fn nuget(entry: &str) -> Option<(&str, &str)> {
    let entry = entry.split_once(" (").map_or(entry, |(entry, _)| entry);
    let (name, range) = entry.trim().split_once(' ')?;
    let bounds = range.trim().strip_prefix('[')?.strip_suffix(']')?;
    let mut parts = bounds.split(',').map(str::trim);
    let low = parts.next()?;
    let exact = match parts.next() {
        None => low,
        Some(high) if high == low && parts.next().is_none() => low,
        Some(_) => return None,
    };
    is_version(exact).then_some((name, exact))
}

/// `vendor/package 1.2.3` or `vendor/package =1.2.3`; platform requirements
/// (`php`, `ext-json`) have no vendor and name no package.
fn packagist(entry: &str) -> Option<(&str, &str)> {
    let (name, constraint) = entry.trim().split_once(' ')?;
    let version = constraint.trim().trim_start_matches('=').trim();
    (name.contains('/') && is_version(version)).then_some((name, version))
}

/// `name = 1.2.3`.
fn rubygems(entry: &str) -> Option<(&str, &str)> {
    let (name, requirement) = entry.trim().split_once(' ')?;
    let version = requirement.trim().strip_prefix('=')?.trim();
    is_version(version).then_some((name, version))
}

#[cfg(test)]
mod tests;
