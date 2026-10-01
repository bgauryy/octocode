//! `artifactSearch.version`: classify a requested version and resolve ranges
//! against a registry's published versions.
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

/// PEP 440 release numbers (`2.31.0`, `2.31`); pre/post/dev releases and
/// local labels do not parse, so ranges resolve to final releases only.
fn pep440(value: &str) -> Option<Vec<u64>> {
    let value = value.trim().trim_start_matches('v');
    let parts = value
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then_some(parts)
}

fn pep440_cmp(left: &[u64], right: &[u64]) -> std::cmp::Ordering {
    let length = left.len().max(right.len());
    let at = |values: &[u64], index: usize| values.get(index).copied().unwrap_or(0);
    (0..length)
        .map(|index| at(left, index).cmp(&at(right, index)))
        .find(|ordering| ordering.is_ne())
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// PyPI specifiers (`>=2,<3`, `~=2.31`, `==2.*`, `!=2.30.0`): the highest
/// final release satisfying every clause.
pub(crate) fn pypi_resolve<'a>(
    specifier: &str,
    versions: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    type Clause = Box<dyn Fn(&[u64]) -> bool>;
    let mut clauses = Vec::<Clause>::new();
    for clause in specifier
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        let (operator, value) = ["~=", "==", "!=", ">=", "<=", ">", "<"]
            .iter()
            .find_map(|op| clause.strip_prefix(op).map(|rest| (*op, rest.trim())))?;
        if let Some(prefix) = value.strip_suffix(".*") {
            let prefix = pep440(prefix)?;
            let equal = operator == "==";
            if !matches!(operator, "==" | "!=") {
                return None;
            }
            clauses.push(Box::new(move |version: &[u64]| {
                let within = prefix
                    .iter()
                    .enumerate()
                    .all(|(index, part)| version.get(index).copied().unwrap_or(0) == *part);
                within == equal
            }));
            continue;
        }
        let bound = pep440(value)?;
        let compatible_upper = (operator == "~=").then(|| {
            let mut upper = bound[..bound.len().saturating_sub(1).max(1)].to_vec();
            if let Some(last) = upper.last_mut() {
                *last += 1;
            }
            upper
        });
        clauses.push(Box::new(move |version: &[u64]| {
            let ordering = pep440_cmp(version, &bound);
            match operator {
                "==" => ordering.is_eq(),
                "!=" => ordering.is_ne(),
                ">=" => ordering.is_ge(),
                "<=" => ordering.is_le(),
                ">" => ordering.is_gt(),
                "<" => ordering.is_lt(),
                _ => {
                    ordering.is_ge()
                        && compatible_upper
                            .as_deref()
                            .is_some_and(|upper| pep440_cmp(version, upper).is_lt())
                }
            }
        }));
    }
    if clauses.is_empty() {
        return None;
    }
    versions
        .into_iter()
        .filter_map(|value| pep440(value).map(|parts| (parts, value)))
        .filter(|(parts, _)| clauses.iter().all(|clause| clause(parts)))
        .max_by(|(left, _), (right, _)| pep440_cmp(left, right))
        .map(|(_, value)| value.to_owned())
}

/// Up to five published versions closest to a missing `requested` one:
/// those sharing the longest leading numeric prefix, newest first.
pub(crate) fn nearest<'a>(
    requested: &str,
    versions: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let wanted: Vec<&str> = requested
        .trim_start_matches(|c: char| !c.is_ascii_digit())
        .split(['.', '-', '+'])
        .collect();
    let mut scored = versions
        .into_iter()
        .map(|version| {
            let shared = version
                .split(['.', '-', '+'])
                .zip(&wanted)
                .take_while(|(have, want)| have == *want)
                .count();
            (shared, version)
        })
        .filter(|(shared, _)| *shared > 0)
        .collect::<Vec<_>>();
    let best = scored.iter().map(|(shared, _)| *shared).max().unwrap_or(0);
    scored.retain(|(shared, _)| *shared == best);
    let mut picked = scored
        .into_iter()
        .map(|(_, version)| version.to_owned())
        .collect::<Vec<_>>();
    picked.sort_by(|left, right| match (pep440(left), pep440(right)) {
        (Some(l), Some(r)) => pep440_cmp(&r, &l),
        _ => right.cmp(left),
    });
    picked.truncate(5);
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_exact_tag_and_range() {
        assert_eq!(
            VersionSpec::parse("3.22.0"),
            VersionSpec::Exact("3.22.0".into())
        );
        assert_eq!(
            VersionSpec::parse("v1.0.100"),
            VersionSpec::Exact("1.0.100".into())
        );
        assert_eq!(
            VersionSpec::parse("latest"),
            VersionSpec::Tag("latest".into())
        );
        assert_eq!(VersionSpec::parse("next"), VersionSpec::Tag("next".into()));
        for range in ["^3", "~3.22", ">=1.2 <2", "3.x", "1 || 2", "2.31"] {
            assert_eq!(
                VersionSpec::parse(range),
                VersionSpec::Range(range.into()),
                "{range}"
            );
        }
    }

    #[test]
    fn npm_ranges_follow_npm_semantics() {
        let versions = ["3.22.0", "3.25.76", "3.26.0-beta.1", "4.0.0", "4.6.5"];
        let resolve = |range| npm_resolve(range, versions, Some("4.6.5"));
        assert_eq!(resolve("^3").as_deref(), Some("3.25.76"));
        assert_eq!(resolve("~3.22").as_deref(), Some("3.22.0"));
        assert_eq!(resolve(">=3.22.0 <4").as_deref(), Some("3.25.76"));
        assert_eq!(resolve(">= 3.22.0 < 4").as_deref(), Some("3.25.76"));
        assert_eq!(resolve("3.x").as_deref(), Some("3.25.76"));
        assert_eq!(
            resolve("3.22").as_deref(),
            Some("3.22.0"),
            "npm: partial = x-range"
        );
        assert_eq!(resolve("3.22.0 - 3.30.0").as_deref(), Some("3.25.76"));
        assert_eq!(
            resolve("^2 || ^4").as_deref(),
            Some("4.6.5"),
            "latest satisfies"
        );
        assert_eq!(resolve("^5"), None);
    }

    #[test]
    fn cargo_and_pypi_ranges() {
        assert_eq!(
            cargo_resolve("^1.0.100", ["1.0.99", "1.0.100", "1.0.228", "2.0.0"]).as_deref(),
            Some("1.0.228")
        );
        let pypi = ["2.30.0", "2.31.0", "2.32.3", "3.0.0rc1", "1.2"];
        assert_eq!(pypi_resolve(">=2.31,<3", pypi).as_deref(), Some("2.32.3"));
        assert_eq!(pypi_resolve("~=2.31", pypi).as_deref(), Some("2.32.3"));
        assert_eq!(pypi_resolve("~=2.31.0", pypi).as_deref(), Some("2.31.0"));
        assert_eq!(pypi_resolve("==2.30.*", pypi).as_deref(), Some("2.30.0"));
        assert_eq!(
            pypi_resolve("!=2.32.3,>=2", pypi).as_deref(),
            Some("2.31.0")
        );
        assert_eq!(pypi_resolve("nonsense", pypi), None);
    }

    #[test]
    fn nearest_versions_share_the_longest_prefix() {
        let versions = ["1.0.99", "1.0.100", "1.0.101", "1.1.0", "2.0.0"];
        assert_eq!(
            nearest("1.0.105", versions),
            ["1.0.101", "1.0.100", "1.0.99"]
        );
        assert_eq!(nearest("3.0.0", versions), Vec::<String>::new());
    }
}
