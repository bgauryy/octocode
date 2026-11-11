//! The order a refs page lists tags in: grouped by tag prefix, newest
//! version first within each group.
//!
//! A tag's version is the first dotted number run in its name that starts
//! the name, follows a `v`, or follows a separator (`v1.2.3`, `15.2.0`,
//! `ignore-0.4.33`, `pkg@2.0.0`, `rmcp-v0.8.5`); its prefix is the name
//! before that run, without the `v` (`""`, `ignore-`, `pkg@`, `rmcp-`).
//! Tags of one prefix are listed together, so a package's releases never
//! interleave by number with another's (`termcolor-1.4.1` stays with its
//! own, apart from `13.0.0`). The repository's own releases lead: the group
//! with no prefix (`v8.3.8`, `15.2.0`), since a version alone names the
//! repository, and a package's number says nothing against it
//! (`create-vite@9.3.4` is not newer than `v8.3.8`). The other groups go by
//! their newest version, then reverse-lexicographic prefix; tags without a
//! version are the last group.
//!
//! Within a group, versions compare by their numbers (a missing component
//! is 0), then a release ranks above its pre-releases (`1.0.0` above
//! `1.0.0-rc.2` above `1.0.0-rc.1`), as semver orders them. A dotted version
//! (`1.145.1`) is a release: it leads every lone number (`v14`) and every
//! date stamp (`translation/20172701.01`). Ties keep reverse-lexicographic
//! name order, GitHub's own, so the order is total and stable.
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Sort `names` by prefix group, newest version first within it
/// ([module docs](self)).
pub(super) fn newest_first(names: &mut [String]) {
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
            let own = |version: &TagVersion<'_>| version.prefix() == Some("");
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
mod tests {
    use super::*;

    /// ripgrep's tags: the invariant is that each prefix's tags are one
    /// contiguous run, in descending version order, and that unversioned
    /// tags come last: `termcolor-1.4.1` never lands between `13.0.0` and
    /// `0.10.0` by its number.
    #[test]
    fn each_prefix_is_one_run_in_version_order() {
        let tags = [
            "wincolor-0.1.6",
            "termcolor-1.4.1",
            "nightly",
            "ignore-0.4.33",
            "grep-0.3.2",
            "15.1.0",
            "15.2.0",
            "15.0.0-rc.1",
            "15.0.0-rc.10",
            "15.0.0",
            "13.0.0",
            "0.10.0",
            "rmcp-v0.8.5",
            "rmcp-v0.9.0",
            "termcolor-1.4.0",
            "v14.1.1",
        ];
        let listed = sorted(&tags);
        assert_eq!(listed.len(), tags.len());
        let mut runs: Vec<(Option<String>, Vec<&String>)> = Vec::new();
        for name in &listed {
            let prefix = TagVersion::of(name).prefix().map(str::to_owned);
            match runs.last_mut() {
                Some((last, run)) if *last == prefix => run.push(name),
                _ => runs.push((prefix, vec![name])),
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        for (prefix, run) in &runs {
            assert!(seen.insert(prefix), "{prefix:?} is split: {listed:?}");
            for pair in run.windows(2) {
                assert!(
                    TagVersion::of(pair[0]) >= TagVersion::of(pair[1]),
                    "{pair:?} out of order in {listed:?}"
                );
            }
        }
        assert_eq!(runs.last().and_then(|(prefix, _)| prefix.as_deref()), None);
        assert_eq!(runs.len(), 6 + 1);
    }

    /// A monorepo's package tags never lead its own releases, whatever
    /// their numbers: the first page answers "the repository's newest
    /// release" (vite: `v8.3.8` before `create-vite@9.3.4`).
    #[test]
    fn the_repositorys_own_releases_lead_package_groups() {
        let listed = sorted(&[
            "create-vite@9.3.4",
            "v8.3.7",
            "plugin-legacy@7.0.0",
            "create-vite@9.3.3",
            "v8.3.8",
            "v8.3.8-beta.1",
            "nightly",
        ]);
        assert_eq!(
            listed,
            [
                "v8.3.8",
                "v8.3.8-beta.1",
                "v8.3.7",
                "create-vite@9.3.4",
                "create-vite@9.3.3",
                "plugin-legacy@7.0.0",
                "nightly"
            ]
        );
    }

    /// One version under several prefixes keeps GitHub's
    /// reverse-lexicographic order; so do unversioned tags.
    #[test]
    fn ties_keep_reverse_lexicographic_order() {
        let mut names = ["a-1.0.0", "c-1.0.0", "b-1.0.0", "beta", "alpha"].map(str::to_owned);
        newest_first(&mut names);
        assert_eq!(names, ["c-1.0.0", "b-1.0.0", "a-1.0.0", "beta", "alpha"]);
    }

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut names = names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        newest_first(&mut names);
        names
    }

    /// A dated or lone-number tag (`translation/20190712`, `v14`) is not a
    /// release: within a group dotted versions lead, descending, whatever
    /// number the others start with.
    #[test]
    fn dotted_versions_lead_over_dates_and_lone_numbers() {
        let listed = sorted(&["v14", "1.145.0", "1.145.1", "v2", "latest"]);
        assert_eq!(listed, ["1.145.1", "1.145.0", "v14", "v2", "latest"]);
        // A date stamp in its own prefix group stays one run.
        let listed = sorted(&[
            "translation/20190712",
            "1.145.0",
            "translation/20172701.01",
            "v14",
        ]);
        assert_eq!(
            listed,
            [
                "1.145.0",
                "v14",
                "translation/20190712",
                "translation/20172701.01"
            ]
        );
    }

    /// Without any dotted version, lone numbers still order newest first.
    #[test]
    fn lone_number_tags_order_among_themselves() {
        assert_eq!(
            sorted(&["v1", "v10", "v2", "beta"]),
            ["v10", "v2", "v1", "beta"]
        );
    }
}
