//! The order a refs page lists tags in: grouped by tag prefix, the
//! repository's own groups first, newest version first within each group.
//!
//! A tag's version is the first dotted number run in its name that starts
//! the name, follows a `v`, or follows a separator (`v1.2.3`, `15.2.0`,
//! `ignore-0.4.33`, `pkg@2.0.0`, `rmcp-v0.8.5`); its prefix is the name
//! before that run, without the `v` (`""`, `ignore-`, `pkg@`, `rmcp-`).
//! Tags of one prefix are listed together, so a package's releases never
//! interleave by number with another's (`termcolor-1.4.1` stays with its
//! own, apart from `13.0.0`).
//!
//! The repository's own releases lead: the groups whose prefix names the
//! repository, either none (`v8.3.8`, `15.2.0`: a version alone names the
//! repository) or a package named like the repository, scoped or not
//! (`kit` lists `@sveltejs/kit@3.2.0`, `astro` lists `astro@5.0.0`, `bun`
//! lists `bun-v1.1.0`). A package's number says nothing against them
//! (`create-vite@9.3.4` is not newer than `v8.3.8`, nor
//! `@sveltejs/adapter-cloudflare@8.0.2` than `@sveltejs/kit@3.2.0`). Every
//! group, own or not, goes by its newest version, then reverse-lexicographic
//! prefix, so a current package line leads an abandoned unprefixed one
//! (`astro@5.0.0` before `v0.0.12`); tags without a version are the last
//! group. GitHub's latest release does not decide it: in a monorepo that
//! releases each package it names whichever package published last.
//!
//! Within a group, versions compare by their numbers (a missing component
//! is 0), then a release ranks above its pre-releases (`1.0.0` above
//! `1.0.0-rc.2` above `1.0.0-rc.1`), as semver orders them. A dotted version
//! (`1.145.1`) is a release: it leads every lone number (`v14`) and every
//! date stamp (`translation/20172701.01`). Ties keep reverse-lexicographic
//! name order, GitHub's own, so the order is total and stable.
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Sort the tag `names` of repository `repo` by prefix group, its own
/// groups first, newest version first within each ([module docs](self)).
pub(super) fn newest_first(names: &mut [String], repo: &str) {
    // Parse each name once, not at every comparison of the sort.
    let listed = names.to_vec();
    let parsed = listed
        .iter()
        .map(|name| (name, TagVersion::of(name)))
        .collect::<Vec<_>>();
    let mut newest: BTreeMap<Option<&str>, &TagVersion<'_>> = BTreeMap::new();
    for (_, version) in &parsed {
        let best = newest.entry(version.prefix()).or_insert(version);
        if version > *best {
            *best = version;
        }
    }
    let mut keyed = parsed
        .iter()
        .map(|(name, version)| (newest[&version.prefix()], version, *name))
        .collect::<Vec<_>>();
    keyed.sort_by(
        |(left_best, left_version, left), (right_best, right_version, right)| {
            let own = |version: &TagVersion<'_>| {
                version
                    .prefix()
                    .is_some_and(|prefix| names_repository(prefix, repo))
            };
            own(right_version)
                .cmp(&own(left_version))
                .then_with(|| right_best.cmp(left_best))
                .then_with(|| right_version.prefix().cmp(&left_version.prefix()))
                .then_with(|| right_version.cmp(left_version))
                .then_with(|| right.cmp(left))
        },
    );
    for (slot, (_, _, name)) in names.iter_mut().zip(keyed) {
        slot.clone_from(name);
    }
}

/// Whether a tag prefix names the repository `repo` itself: none, or a
/// package name (its last `/` segment, without the separator before the
/// version) equal to the repository's name, ignoring case.
fn names_repository(prefix: &str, repo: &str) -> bool {
    let package = prefix.trim_end_matches(|c: char| !c.is_alphanumeric());
    let package = package.rsplit('/').next().unwrap_or(package);
    package.is_empty() || package.eq_ignore_ascii_case(repo)
}

/// A tag name's version: `None` sorts below every version, and a lone
/// number below every dotted one.
#[derive(Debug, PartialEq, Eq)]
struct TagVersion<'a>(Option<Versioned<'a>>);

/// A version found in a name: its prefix, numbers and pre-release suffix.
#[derive(Debug, PartialEq, Eq)]
struct Versioned<'a> {
    prefix: &'a str,
    numbers: Vec<u64>,
    pre: Option<&'a str>,
}

impl<'a> TagVersion<'a> {
    /// The name before the version (without its `v`); `None` for no version.
    fn prefix(&self) -> Option<&'a str> {
        self.0.as_ref().map(|version| version.prefix)
    }

    fn of(name: &'a str) -> Self {
        let bytes = name.as_bytes();
        let start = (0..bytes.len()).find(|&at| {
            bytes[at].is_ascii_digit()
                && (at == 0
                    || matches!(bytes[at - 1], b'v' | b'V')
                        && (at == 1 || !bytes[at - 2].is_ascii_alphanumeric())
                    || !bytes[at - 1].is_ascii_alphanumeric())
        });
        let Some(start) = start else {
            return Self(None);
        };
        let rest = &name[start..];
        let end = rest
            .char_indices()
            .find(|&(at, c)| {
                !(c.is_ascii_digit()
                    || c == '.' && rest[at + 1..].starts_with(|next: char| next.is_ascii_digit()))
            })
            .map_or(rest.len(), |(at, _)| at);
        let numbers = rest[..end]
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(u64::MAX))
            .collect();
        // A suffix starting with a letter or `-` + letter/digit is a
        // pre-release (`-rc.1`, `-beta`, `a1`); build metadata (`+…`) and a
        // trailing separator are not.
        let suffix = rest[end..].trim_start_matches(['-', '.', '_']);
        let pre = (!suffix.is_empty() && !rest[end..].starts_with('+')).then_some(suffix);
        let before = &name[..start];
        let prefix = before
            .strip_suffix(['v', 'V'])
            .filter(|rest| !rest.ends_with(|c: char| c.is_ascii_alphanumeric()))
            .unwrap_or(before);
        Self(Some(Versioned {
            prefix,
            numbers,
            pre,
        }))
    }
}

impl Ord for TagVersion<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (&self.0, &other.0) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(left), Some(right)) => {
                let (left, left_pre) = (&left.numbers, &left.pre);
                let (right, right_pre) = (&right.numbers, &right.pre);
                if is_release(left) != is_release(right) {
                    return is_release(left).cmp(&is_release(right));
                }
                let width = left.len().max(right.len());
                let at = |numbers: &[u64], index: usize| numbers.get(index).copied().unwrap_or(0);
                (0..width)
                    .map(|index| at(left, index).cmp(&at(right, index)))
                    .find(|order| order.is_ne())
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| match (left_pre, right_pre) {
                        (None, None) => Ordering::Equal,
                        (None, Some(_)) => Ordering::Greater,
                        (Some(_), None) => Ordering::Less,
                        (Some(a), Some(b)) => pre_release(a, b),
                    })
            }
        }
    }
}

impl PartialOrd for TagVersion<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Whether a version's numbers name a release: dotted, and led by a number
/// that is not a date stamp (`20172701.01`, six digits or more).
fn is_release(numbers: &[u64]) -> bool {
    numbers.len() > 1 && numbers[0] < 100_000
}

/// Semver pre-release order: dot-separated identifiers left to right,
/// numbers numerically and below words, a shorter list below a longer one
/// it prefixes. Identifiers that glue a word to a number (`rc10`) compare
/// the word, then the number.
fn pre_release(left: &str, right: &str) -> Ordering {
    let ids = |text: &str| {
        text.split(['.', '-'])
            .map(|id| {
                let digits = id.trim_start_matches(|c: char| !c.is_ascii_digit());
                let word = id.strip_suffix(digits).unwrap_or(id);
                (word.to_owned(), digits.parse::<u64>().ok())
            })
            .collect::<Vec<_>>()
    };
    ids(left).cmp(&ids(right))
}

#[cfg(test)]
mod tests;
