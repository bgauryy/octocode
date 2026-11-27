//! PEP 440 version specifiers (`>=2.8,<3`, `~=2.31`, `==2.*`, `===1.0`)
//! and the release pip selects for one, as `packaging` 26 matches them.
use super::Version;

/// One clause, as the versions it admits.
#[derive(Debug)]
enum Clause {
    /// `>=V`; also `>V.devN` (from `V.dev(N+1)`) and `>V.postN` (from
    /// `V.post(N+1).dev0`).
    AtLeast(Version),
    /// `<=V`: up to `V` and its local builds.
    AtMost(Version),
    /// `<V`: below `V.dev0` for a final `V`, so no pre-release of `V`.
    Below(Version),
    /// `>V` for a final or pre-release `V`: above it, but none of its
    /// post releases or local builds.
    AbovePosts(Version),
    /// `==V` (`negate`: `!=V`); a `V` without a local label also admits
    /// its local builds.
    Equal { version: Version, negate: bool },
    /// `~=V`, `==V.*` (`negate`: `!=V.*`): `lower <= version < upper`.
    Within {
        lower: Version,
        upper: Version,
        negate: bool,
    },
    /// `===text`: the release string itself, ignoring case.
    Arbitrary(String),
}

/// Comma-joined clauses, all of which a release must satisfy.
#[derive(Debug)]
pub(crate) struct Specifiers {
    clauses: Vec<Clause>,
    /// A clause names a pre-release (`>=5.0rc1`), so pre-releases match
    /// like any release.
    prereleases: bool,
}

impl Specifiers {
    /// The specifier `text` spells; `Err` names the first clause that is
    /// not PEP 440 (or the whole text when it holds no clause).
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let mut clauses = Vec::new();
        let mut prereleases = false;
        for written in text.split(',').map(str::trim).filter(|c| !c.is_empty()) {
            let (clause, names_prerelease) =
                Clause::parse(written).ok_or_else(|| written.to_owned())?;
            clauses.push(clause);
            prereleases |= names_prerelease;
        }
        if clauses.is_empty() {
            return Err(text.trim().to_owned());
        }
        Ok(Self {
            clauses,
            prereleases,
        })
    }

    /// Whether `version` (spelled `raw`) satisfies every clause, before
    /// the pre-release policy.
    pub(super) fn admits(&self, version: &Version, raw: &str) -> bool {
        self.clauses
            .iter()
            .all(|clause| clause.admits(version, raw))
    }

    /// The highest of `releases` pip would install: pre-releases only when
    /// a clause names one or no final release matches. Releases that are
    /// not PEP 440 never match.
    pub(crate) fn resolve<'a>(
        &self,
        releases: impl IntoIterator<Item = &'a str>,
    ) -> Option<&'a str> {
        let matching = releases
            .into_iter()
            .filter_map(|raw| Version::parse(raw).map(|version| (version, raw)))
            .filter(|(version, raw)| self.admits(version, raw))
            .collect::<Vec<_>>();
        let finals_only =
            !self.prereleases && matching.iter().any(|(version, _)| !version.is_prerelease());
        matching
            .into_iter()
            .filter(|(version, _)| !finals_only || !version.is_prerelease())
            .max_by(|(left, _), (right, _)| left.cmp(right))
            .map(|(_, raw)| raw)
    }
}

impl Clause {
    /// One clause and whether it names a pre-release; `None` when it is
    /// not PEP 440: an unknown operator, a wildcard other than a bare
    /// release after `==`/`!=`, a local label after an ordering operator,
    /// or `~=` with a single release number.
    fn parse(written: &str) -> Option<(Self, bool)> {
        const OPERATORS: [&str; 8] = ["===", "~=", "==", "!=", "<=", ">=", "<", ">"];
        let operator = *OPERATORS.iter().find(|op| written.starts_with(**op))?;
        let value = written[operator.len()..].trim();
        if value.is_empty() || value.contains(char::is_whitespace) {
            return None;
        }
        if operator == "===" {
            if value.contains([';', ')']) {
                return None;
            }
            let names_prerelease = Version::parse(value).is_some_and(|v| v.is_prerelease());
            return Some((
                Self::Arbitrary(value.to_ascii_lowercase()),
                names_prerelease,
            ));
        }
        let (value, wildcard) = value
            .strip_suffix(".*")
            .map_or((value, false), |prefix| (prefix, true));
        let version = Version::parse(value)?;
        let equality = matches!(operator, "==" | "!=");
        if (wildcard && !(equality && version.is_bare_release()))
            || (version.has_local() && !equality)
            || (operator == "~=" && version.release_len() < 2)
        {
            return None;
        }
        let names_prerelease = operator != "!=" && !wildcard && version.is_prerelease();
        let negate = operator == "!=";
        let clause = match operator {
            "==" | "!=" if wildcard => {
                let (lower, upper) = version.prefix_bounds(version.release_len());
                Self::Within {
                    lower,
                    upper,
                    negate,
                }
            }
            "==" | "!=" => Self::Equal { version, negate },
            "~=" => {
                let (_, upper) = version.prefix_bounds(version.release_len() - 1);
                Self::Within {
                    lower: version,
                    upper,
                    negate: false,
                }
            }
            ">=" => Self::AtLeast(version),
            "<=" => Self::AtMost(version),
            "<" if version.is_prerelease() => Self::Below(version),
            "<" => Self::Below(version.with_dev(0)),
            _ => match (version.dev(), version.post()) {
                (Some(dev), _) => Self::AtLeast(version.with_dev(dev.saturating_add(1))),
                (None, Some(post)) => Self::AtLeast(version.post_dev0(post.saturating_add(1))),
                (None, None) => Self::AbovePosts(version),
            },
        };
        Some((clause, names_prerelease))
    }

    fn admits(&self, version: &Version, raw: &str) -> bool {
        match self {
            Self::AtLeast(bound) => version >= bound,
            Self::AtMost(bound) => version.cmp_public(bound).is_le(),
            Self::Below(bound) => version < bound,
            Self::AbovePosts(bound) => version > bound && !version.is_post_of(bound),
            Self::Equal {
                version: pin,
                negate,
            } => {
                let equal = if pin.has_local() {
                    version == pin
                } else {
                    version.cmp_public(pin).is_eq()
                };
                equal != *negate
            }
            Self::Within {
                lower,
                upper,
                negate,
            } => (lower <= version && version < upper) != *negate,
            Self::Arbitrary(text) => raw.to_ascii_lowercase() == *text,
        }
    }
}
