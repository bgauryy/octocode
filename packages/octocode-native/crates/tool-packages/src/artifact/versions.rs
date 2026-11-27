//! `artifactSearch.version`: classify a requested version and resolve ranges
//! against a registry's published versions.
use super::ArtifactError;
use octocode_contract_types::ArtifactType;
use semver::{Version, VersionReq};

/// What a `version` string asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VersionSpec {
    /// One published version (`1.2.3`, `v1.2.3`, `=1.2.3`).
    Exact(String),
    /// A registry tag (`latest`, `next`, `beta`).
    Tag(String),
    /// A range; resolved to the highest matching release.
    Range(String),
}

impl VersionSpec {
    pub(crate) fn parse(spec: &str) -> Self {
        let spec = spec.trim();
        let bare = spec.trim_start_matches('=').trim_start_matches('v').trim();
        if Version::parse(bare).is_ok() {
            return Self::Exact(bare.to_owned());
        }
        // `v18`, `v1.2`, `v1.x` name versions, never a dist-tag.
        if let Some(rest) = spec
            .strip_prefix('v')
            .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        {
            return Self::Range(rest.to_owned());
        }
        let tag = spec
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
            && spec
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            && !matches!(spec, "x" | "X");
        if tag {
            Self::Tag(spec.to_owned())
        } else {
            Self::Range(spec.to_owned())
        }
    }

    /// [`Self::parse`] as npm reads a spec (npm-package-arg): a spec that
    /// parses as no npm range is a dist-tag, even one spelled like a version
    /// (`v12-lts`).
    pub(crate) fn parse_npm(spec: &str) -> Self {
        match Self::parse(spec) {
            Self::Range(range) if npm_requirements(&range).is_none() => {
                Self::Tag(spec.trim().to_owned())
            }
            parsed => parsed,
        }
    }
}

/// Whether `spec` is written as a version range (npm, Cargo, PEP 440,
/// Maven or NuGet syntax): an operator, a bracketed interval, a list, a
/// wildcard component or spaces. Exact versions of every ecosystem
/// (`31.1-jre`, `2.0.0.Final`, `v0.0.0-2021…-abc`) hold none of these.
pub(crate) fn is_range(spec: &str) -> bool {
    let spec = spec.trim();
    spec.contains(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '[' | ']' | '(' | ')' | ',' | '<' | '>' | '^' | '~' | '*' | '|'
            )
    }) || spec.split(['.', '-']).any(|part| matches!(part, "x" | "X"))
}

/// npm range syntax mapped onto `semver` requirements, one per `||`
/// alternative. Bare versions are exact or partial (`1.2` = `1.2.x`), as in
/// npm, not Cargo's caret default; `a - b` is inclusive.
fn npm_requirements(range: &str) -> Option<Vec<VersionReq>> {
    range
        .split("||")
        .map(|alternative| {
            let alternative = alternative.trim();
            if alternative.is_empty() || matches!(alternative, "*" | "x" | "X" | "latest") {
                return VersionReq::parse("*").ok();
            }
            if let Some((low, high)) = alternative.split_once(" - ") {
                return VersionReq::parse(&format!(
                    ">={}, <={}",
                    low.trim().trim_start_matches('v'),
                    high.trim().trim_start_matches('v')
                ))
                .ok();
            }
            // Join an operator separated from its version (`>= 1.2`).
            let mut comparators = Vec::<String>::new();
            let mut pending = String::new();
            for token in alternative.split_whitespace() {
                if token
                    .chars()
                    .all(|c| matches!(c, '<' | '>' | '=' | '~' | '^'))
                {
                    pending.push_str(token);
                    continue;
                }
                let token = format!("{pending}{token}");
                pending.clear();
                let starts_with_digit = token
                    .trim_start_matches('v')
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit());
                let token = token.trim_start_matches('v').to_owned();
                let wildcard = token.split('.').any(|part| matches!(part, "x" | "X" | "*"));
                comparators.push(if starts_with_digit && !wildcard {
                    format!("={token}")
                } else {
                    token
                });
            }
            VersionReq::parse(&comparators.join(", ")).ok()
        })
        .collect()
}

/// The npm-install choice for `range`: the `latest` tag when it satisfies the
/// range and is not deprecated, else the highest matching version that is
/// not deprecated, else the highest matching one (prereleases only when a
/// comparator names one, per `semver`).
pub(crate) fn npm_resolve<'a>(
    range: &str,
    versions: impl IntoIterator<Item = &'a str>,
    latest: Option<&str>,
    deprecated: impl Fn(&str) -> bool,
) -> Option<String> {
    let requirements = npm_requirements(range)?;
    let matches = |version: &Version| requirements.iter().any(|req| req.matches(version));
    if let Some(latest) = latest
        .filter(|value| !deprecated(value))
        .and_then(|value| Version::parse(value).ok())
        && matches(&latest)
    {
        return Some(latest.to_string());
    }
    let versions = versions.into_iter().collect::<Vec<_>>();
    highest(
        versions
            .iter()
            .copied()
            .filter(|version| !deprecated(version)),
        matches,
    )
    .or_else(|| highest(versions, matches))
}

/// Cargo requirement syntax (crates.io): `semver` parses it natively.
pub(crate) fn cargo_resolve<'a>(
    range: &str,
    versions: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let requirement = VersionReq::parse(range).ok()?;
    highest(versions, |version| requirement.matches(version))
}

fn highest<'a>(
    versions: impl IntoIterator<Item = &'a str>,
    matches: impl Fn(&Version) -> bool,
) -> Option<String> {
    versions
        .into_iter()
        .filter_map(|value| Version::parse(value).ok())
        .filter(|version| matches(version))
        .max()
        .map(|version| version.to_string())
}

/// How one ecosystem orders its published versions: the precedence of two
/// versions (`None` when either is not a version it reads) and which are
/// prereleases. Nearest-version suggestions read only this, never a
/// cross-ecosystem guess.
#[derive(Clone, Copy)]
pub(crate) struct VersionOrder {
    pub(crate) cmp: fn(&str, &str) -> Option<std::cmp::Ordering>,
    pub(crate) prerelease: fn(&str) -> bool,
}

impl VersionOrder {
    fn readable(&self, version: &str) -> bool {
        (self.cmp)(version, version).is_some()
    }
}

fn semver_of(value: &str) -> Option<Version> {
    Version::parse(value.trim().trim_start_matches('v')).ok()
}

/// Semantic Versioning precedence (npm, crates.io, Go modules): build
/// metadata takes no part.
pub(crate) const SEMVER: VersionOrder = VersionOrder {
    cmp: |left, right| Some(semver_of(left)?.cmp_precedence(&semver_of(right)?)),
    prerelease: |value| semver_of(value).is_some_and(|version| !version.pre.is_empty()),
};

/// A version read as its leading release numbers (`2.31.0`) and the label
/// after them (`rc1`, `-beta.2`, `.Final`); `None` with no leading number.
fn segments(value: &str) -> Option<(Vec<u64>, &str)> {
    let value = value.trim().trim_start_matches(['v', 'V']);
    let mut numbers = Vec::new();
    let mut rest = value;
    loop {
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let (number, after) = rest.split_at(digits);
        // A part such as `1a` is a number followed by its label.
        let Ok(number) = number.parse::<u64>() else {
            break;
        };
        numbers.push(number);
        rest = after;
        match rest.strip_prefix('.') {
            Some(next) if next.starts_with(|c: char| c.is_ascii_digit()) => rest = next,
            _ => break,
        }
    }
    (!numbers.is_empty()).then_some((numbers, rest))
}

/// Labels compared part by part, numbers as numbers (`rc10` after `rc9`).
fn cmp_labels(left: &str, right: &str) -> std::cmp::Ordering {
    let parts = |label: &str| {
        let mut parts = Vec::new();
        let mut current = String::new();
        for c in label.chars() {
            let boundary = current
                .chars()
                .last()
                .is_some_and(|last| last.is_ascii_digit() != c.is_ascii_digit());
            if !c.is_ascii_alphanumeric() || boundary {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
                if !c.is_ascii_alphanumeric() {
                    continue;
                }
            }
            current.push(c.to_ascii_lowercase());
        }
        if !current.is_empty() {
            parts.push(current);
        }
        parts
    };
    let (left, right) = (parts(left), parts(right));
    for (l, r) in left.iter().zip(&right) {
        let ordering = match (l.parse::<u64>(), r.parse::<u64>()) {
            (Ok(l), Ok(r)) => l.cmp(&r),
            (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
            (Err(_), Ok(_)) => std::cmp::Ordering::Less,
            (Err(_), Err(_)) => l.cmp(r),
        };
        if ordering.is_ne() {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

/// The order of registries whose versions are release numbers and a label
/// (Maven, RubyGems, Packagist): release numbers first (missing trailing
/// parts read as 0), then a prerelease label before the bare release before
/// any other label (`sp1`, `patch1`), labels part by part.
pub(crate) fn cmp_segmented(
    left: &str,
    right: &str,
    prerelease: fn(&str) -> bool,
) -> Option<std::cmp::Ordering> {
    let (left_numbers, left_label) = segments(left)?;
    let (right_numbers, right_label) = segments(right)?;
    let length = left_numbers.len().max(right_numbers.len());
    let at = |values: &[u64], index: usize| values.get(index).copied().unwrap_or(0);
    let rank = |version: &str, label: &str| match (label.is_empty(), prerelease(version)) {
        (_, true) => 0,
        (true, false) => 1,
        (false, false) => 2,
    };
    Some(
        (0..length)
            .map(|index| at(&left_numbers, index).cmp(&at(&right_numbers, index)))
            .find(|ordering| ordering.is_ne())
            .unwrap_or_else(|| {
                rank(left, left_label)
                    .cmp(&rank(right, right_label))
                    .then_with(|| cmp_labels(left_label, right_label))
            }),
    )
}

/// The order of `ecosystem`'s versions.
pub(crate) fn order_of(ecosystem: ArtifactType) -> VersionOrder {
    match ecosystem {
        ArtifactType::Npm | ArtifactType::Crates | ArtifactType::Go => SEMVER,
        ArtifactType::Pypi => super::pypi::ORDER,
        ArtifactType::Nuget => super::nuget::ORDER,
        ArtifactType::Maven => super::maven::ORDER,
        ArtifactType::Packagist => super::packagist::ORDER,
        ArtifactType::Rubygems => RUBYGEMS,
    }
}

/// RubyGems: a version with a letter in it is a prerelease (`Gem::Version`).
const RUBYGEMS: VersionOrder = VersionOrder {
    cmp: |left, right| {
        cmp_segmented(left, right, |value| {
            value.chars().any(|c| c.is_ascii_alphabetic())
        })
    },
    prerelease: |value| value.chars().any(|c| c.is_ascii_alphabetic()),
};

/// A known package without the requested version: typed, with every
/// published version `ecosystem` orders ranked nearest first as the
/// recovery ([`nearest`]); the tool's repair lead reads as many as one call
/// carries ([`suggested`]). The message quotes `requested` as the caller
/// typed it; the ranking reads it without a leading `=` or `v` (`=0.1.99`,
/// `v2.99.0`), as the registries' versions are written.
pub(crate) fn version_not_found(
    ecosystem: ArtifactType,
    name: &str,
    requested: &str,
    versions: &[String],
) -> ArtifactError {
    ArtifactError {
        nearest: nearest(
            ranked_as(requested),
            versions.iter().map(String::as_str),
            &order_of(ecosystem),
        ),
        ..ArtifactError::version_not_found(format!(
            "{name} has no published version matching \"{requested}\""
        ))
    }
}

/// `error` with the nearest versions `discouraged` names (yanked or
/// deprecated: published, but no release to move to) after every other one,
/// each group in its order, so a repair call reads one only when no other
/// fills it.
pub(crate) fn discouraged_last(
    mut error: ArtifactError,
    discouraged: impl Fn(&str) -> bool,
) -> ArtifactError {
    let (live, rest): (Vec<String>, Vec<String>) = std::mem::take(&mut error.nearest)
        .into_iter()
        .partition(|version| !discouraged(version));
    error.nearest = live.into_iter().chain(rest).collect();
    error
}

/// A requested version as the registries write theirs: no surrounding space,
/// no leading `=` (an exact pin), and no `v` before a digit.
fn ranked_as(requested: &str) -> &str {
    let bare = requested.trim().trim_start_matches('=').trim_start();
    bare.strip_prefix('v')
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(bare)
}

/// The versions a repair call reads of `error`'s nearest ones: the first
/// `limit` (one call's batch), newest first on `ecosystem`'s order. Fewer
/// than `limit` are all the registry has to suggest; the call shows them.
pub fn suggested(ecosystem: ArtifactType, error: &ArtifactError, limit: usize) -> Vec<String> {
    let order = order_of(ecosystem);
    let mut picked = error
        .nearest
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    picked.sort_by(|left, right| (order.cmp)(right, left).unwrap_or(std::cmp::Ordering::Equal));
    picked
}

/// The published versions `order` reads, nearest a missing `requested` one
/// first: final releases before prereleases (unless the request names a
/// prerelease), then those sharing a longer leading part of the request
/// (`5.4` of `5.4.99`), then the closest on `order` either side of it, a
/// newer one first on a tie. Empty when the request shares no leading part
/// with any version and `order` cannot place it.
pub(crate) fn nearest<'a>(
    requested: &str,
    versions: impl IntoIterator<Item = &'a str>,
    order: &VersionOrder,
) -> Vec<String> {
    use std::cmp::{Ordering, Reverse};
    let mut sorted = versions
        .into_iter()
        .filter(|version| order.readable(version))
        .collect::<Vec<_>>();
    sorted.sort_by(|left, right| (order.cmp)(left, right).unwrap_or(Ordering::Equal));
    sorted.dedup();
    let wanted: Vec<&str> = requested
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split(['.', '-', '+'])
        .collect();
    let shared = |version: &str| {
        version
            .trim_start_matches(|c: char| !c.is_ascii_digit())
            .split(['.', '-', '+'])
            .zip(&wanted)
            .take_while(|(have, want)| have == *want)
            .count()
    };
    // Where the missing version would sit; unknown when `order` cannot read it.
    let placed = order.readable(requested).then(|| {
        sorted
            .partition_point(|version| (order.cmp)(version, requested).is_some_and(Ordering::is_lt))
    });
    if placed.is_none() && sorted.iter().all(|version| shared(version) == 0) {
        return Vec::new();
    }
    let wants_prerelease = (order.prerelease)(requested);
    let mut ranked = sorted
        .iter()
        .enumerate()
        .map(|(index, version)| {
            let distance = match placed {
                Some(at) if index < at => at - 1 - index,
                Some(at) => index - at,
                None => sorted.len() - 1 - index,
            };
            let prerelease = !wants_prerelease && (order.prerelease)(version);
            (
                (
                    prerelease,
                    Reverse(shared(version)),
                    distance,
                    Reverse(index),
                ),
                *version,
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .into_iter()
        .map(|(_, version)| version.to_owned())
        .collect()
}

#[cfg(test)]
mod tests;
