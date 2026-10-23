//! The order a refs page lists tags in: newest version first.
//!
//! A tag's version is the first dotted number run in its name that starts
//! the name, follows a `v`, or follows a separator (`v1.2.3`, `15.2.0`,
//! `ignore-0.4.33`, `pkg@2.0.0`, `rmcp-v0.8.5`). Versions compare by their
//! numbers (a missing component is 0), then a release ranks above its
//! pre-releases (`1.0.0` above `1.0.0-rc.2` above `1.0.0-rc.1`), as semver
//! orders them. Tags without a version follow every versioned tag. Ties
//! (one version under several package prefixes, or no version) keep
//! reverse-lexicographic name order, GitHub's own, so the order is total
//! and stable.
use std::cmp::Ordering;

/// Sort `names` newest version first ([module docs](self)).
pub(super) fn newest_first(names: &mut [String]) {
    names.sort_by(|left, right| {
        let (a, b) = (TagVersion::of(left), TagVersion::of(right));
        b.cmp(&a).then_with(|| right.cmp(left))
    });
}

/// A tag name's version: `None` sorts below every version.
#[derive(Debug, PartialEq, Eq)]
struct TagVersion<'a>(Option<(Vec<u64>, Option<&'a str>)>);

impl<'a> TagVersion<'a> {
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
        Self(Some((numbers, pre)))
    }
}

impl Ord for TagVersion<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (&self.0, &other.0) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some((left, left_pre)), Some((right, right_pre))) => {
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

/// Semver pre-release order: dot-separated identifiers left to right,
/// numbers numerically and below words, a shorter list below a longer one
/// it prefixes. Identifiers that glue a word to a number (`rc10`) compare
/// the word, then the number.
fn pre_release(left: &str, right: &str) -> Ordering {
    let ids = |text: &str| {
        text.split(['.', '-'])
            .map(|id| {
                let digits = id.trim_start_matches(|c: char| !c.is_ascii_digit());
                let word = &id[..id.len() - digits.len()];
                (word.to_owned(), digits.parse::<u64>().ok())
            })
            .collect::<Vec<_>>()
    };
    ids(left).cmp(&ids(right))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ripgrep's tags: its own releases, its crates' releases, a
    /// pre-release and an unversioned tag, newest version first.
    #[test]
    fn tags_list_newest_version_first() {
        let mut names = [
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
            "v14.1.1",
        ]
        .map(str::to_owned);
        newest_first(&mut names);
        assert_eq!(
            names,
            [
                "15.2.0",
                "15.1.0",
                "15.0.0",
                "15.0.0-rc.10",
                "15.0.0-rc.1",
                "v14.1.1",
                "13.0.0",
                "termcolor-1.4.1",
                "0.10.0",
                "rmcp-v0.8.5",
                "ignore-0.4.33",
                "grep-0.3.2",
                "wincolor-0.1.6",
                "nightly",
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
}
