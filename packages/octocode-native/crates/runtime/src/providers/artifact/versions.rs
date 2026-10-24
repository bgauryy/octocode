//! `artifactSearch.version`: classify a requested version and resolve ranges
//! against a registry's published versions.
use super::ArtifactError;
use octocode_contract_types::ErrorCode;
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
/// range, else the highest matching version (prereleases only when a
/// comparator names one, per `semver`).
pub(crate) fn npm_resolve<'a>(
    range: &str,
    versions: impl IntoIterator<Item = &'a str>,
    latest: Option<&str>,
) -> Option<String> {
    let requirements = npm_requirements(range)?;
    let matches = |version: &Version| requirements.iter().any(|req| req.matches(version));
    if let Some(latest) = latest.and_then(|value| Version::parse(value).ok())
        && matches(&latest)
    {
        return Some(latest.to_string());
    }
    highest(versions, matches)
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

/// The numeric release of a version (`2.31.0`, `2.31`), for ordering
/// nearest-version hints across ecosystems; versions with any label
/// (`1.0-rc.1`, `2.9.0.post0`) have none. Never a resolver: each
/// ecosystem resolves ranges with its own semantics.
fn release_numbers(value: &str) -> Option<Vec<u64>> {
    let value = value.trim().trim_start_matches('v');
    let parts = value
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then_some(parts)
}

/// Release numbers in order, missing trailing parts read as 0.
fn cmp_release_numbers(left: &[u64], right: &[u64]) -> std::cmp::Ordering {
    let length = left.len().max(right.len());
    let at = |values: &[u64], index: usize| values.get(index).copied().unwrap_or(0);
    (0..length)
        .map(|index| at(left, index).cmp(&at(right, index)))
        .find(|ordering| ordering.is_ne())
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// A known package without the requested version: typed, with the nearest
/// published versions as the recovery.
pub(crate) fn version_not_found(name: &str, requested: &str, versions: &[String]) -> ArtifactError {
    let close = nearest(requested, versions.iter().map(String::as_str));
    let error = ArtifactError::new(
        ErrorCode::VersionNotFound,
        format!("{name} has no published version matching \"{requested}\""),
    )
    .with_status(404);
    ArtifactError {
        nearest: close,
        ..error
    }
}

/// Up to five published versions closest to a missing `requested` one:
/// those sharing the longest leading numeric prefix, newest first; with no
/// shared prefix, the final releases just below and above it.
pub(crate) fn nearest<'a>(
    requested: &str,
    versions: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let versions = versions.into_iter().collect::<Vec<_>>();
    let wanted: Vec<&str> = requested
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split(['.', '-', '+'])
        .collect();
    let mut scored = versions
        .iter()
        .map(|version| {
            let shared = version
                .split(['.', '-', '+'])
                .zip(&wanted)
                .take_while(|(have, want)| have == *want)
                .count();
            (shared, *version)
        })
        .filter(|(shared, _)| *shared > 0)
        .collect::<Vec<_>>();
    let Some(best) = scored.iter().map(|(shared, _)| *shared).max() else {
        return bracketing(requested, &versions);
    };
    scored.retain(|(shared, _)| *shared == best);
    let mut picked = scored
        .into_iter()
        .map(|(_, version)| version.to_owned())
        .collect::<Vec<_>>();
    // Final releases first; a prerelease is a suggestion only when the
    // request names one, or no final release is near.
    let prerelease = |version: &str| version.contains('-');
    if !prerelease(requested) && picked.iter().any(|version| !prerelease(version)) {
        picked.retain(|version| !prerelease(version));
    }
    picked.sort_by(
        |left, right| match (release_numbers(left), release_numbers(right)) {
            (Some(l), Some(r)) => cmp_release_numbers(&r, &l),
            _ => right.cmp(left),
        },
    );
    picked.truncate(5);
    picked
}

/// The final releases nearest a requested release number: up to three
/// below it and two above it, newest first. Empty when the request names
/// no release number.
fn bracketing(requested: &str, versions: &[&str]) -> Vec<String> {
    let release = requested
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split(['-', '+'])
        .next()
        .and_then(release_numbers);
    let Some(release) = release else {
        return Vec::new();
    };
    let mut finals = versions
        .iter()
        .filter_map(|version| release_numbers(version).map(|parsed| (parsed, *version)))
        .collect::<Vec<_>>();
    finals.sort_by(|(left, _), (right, _)| cmp_release_numbers(left, right));
    let split = finals.partition_point(|(parsed, _)| cmp_release_numbers(parsed, &release).is_lt());
    let below = &finals[split.saturating_sub(3)..split];
    let above = finals[split..]
        .iter()
        .filter(|(parsed, _)| cmp_release_numbers(parsed, &release).is_gt())
        .take(2);
    let mut picked = below
        .iter()
        .chain(above)
        .map(|(_, version)| (*version).to_owned())
        .collect::<Vec<_>>();
    picked.reverse();
    picked
}

#[cfg(test)]
mod tests;
