//! Version orders of Linux distributions OSV covers with no registry reader:
//! dpkg's (Debian, Ubuntu) and apk's (Alpine). OSV's fixed versions in these
//! ecosystems compare only in the distribution's own order.
use crate::artifact::VersionOrder;
use std::cmp::Ordering;

/// dpkg: `[epoch:]upstream[-revision]`; `~` sorts before anything, even the
/// end of the string, so `1.0~rc1` is a prerelease of `1.0`.
pub(super) const DPKG: VersionOrder = VersionOrder {
    cmp: |left, right| Some(dpkg_cmp(&dpkg_parts(left)?, &dpkg_parts(right)?)),
    prerelease: |value| dpkg_parts(value).is_some_and(|(_, upstream, _)| upstream.contains('~')),
};

/// apk: `numbers[letter][_suffix[n]]...[-rN]`; `_alpha`, `_beta`, `_pre`
/// and `_rc` sort before the release, `_cvs`, `_svn`, `_git`, `_hg` and
/// `_p` after it.
pub(super) const APK: VersionOrder = VersionOrder {
    cmp: |left, right| Some(apk_of(left)?.cmp(&apk_of(right)?)),
    prerelease: |value| apk_of(value).is_some_and(|version| version.suffixes.iter().any(|(rank, _)| *rank < 0)),
};

fn dpkg_parts(value: &str) -> Option<(u64, &str, &str)> {
    let value = value.trim();
    let (epoch, rest) = match value.split_once(':') {
        Some((epoch, rest)) => (epoch.parse().ok()?, rest),
        None => (0, value),
    };
    let (upstream, revision) = rest.rsplit_once('-').unwrap_or((rest, ""));
    let allowed = |c: char| c.is_ascii_alphanumeric() || ".+~-:".contains(c);
    (upstream.chars().next().is_some_and(|c| c.is_ascii_digit())
        && upstream.chars().all(allowed)
        && revision.chars().all(allowed))
    .then_some((epoch, upstream, revision))
}

fn dpkg_cmp(left: &(u64, &str, &str), right: &(u64, &str, &str)) -> Ordering {
    left.0
        .cmp(&right.0)
        .then_with(|| verrevcmp(left.1, right.1))
        .then_with(|| verrevcmp(left.2, right.2))
}

/// The weight of one character in a non-digit run: `~` below the end, the
/// end below letters, letters below other characters.
fn weight(c: Option<char>) -> i32 {
    match c {
        Some('~') => -1,
        None => 0,
        Some(c) if c.is_ascii_alphabetic() => c as i32,
        Some(c) => c as i32 + 256,
    }
}

/// dpkg's `verrevcmp`: alternating non-digit runs, compared by [`weight`],
/// and digit runs, compared as numbers.
fn verrevcmp(left: &str, right: &str) -> Ordering {
    let (mut left, mut right) = (left.chars().peekable(), right.chars().peekable());
    loop {
        if left.peek().is_none() && right.peek().is_none() {
            return Ordering::Equal;
        }
        loop {
            let l = left.peek().copied().filter(|c| !c.is_ascii_digit());
            let r = right.peek().copied().filter(|c| !c.is_ascii_digit());
            if l.is_none() && r.is_none() {
                break;
            }
            let order = weight(l).cmp(&weight(r));
            if order != Ordering::Equal {
                return order;
            }
            left.next();
            right.next();
        }
        let number = |chars: &mut std::iter::Peekable<std::str::Chars<'_>>| {
            let mut digits = String::new();
            while let Some(c) = chars.next_if(char::is_ascii_digit) {
                digits.push(c);
            }
            let digits = digits.trim_start_matches('0');
            (digits.len(), digits.to_owned())
        };
        let order = number(&mut left).cmp(&number(&mut right));
        if order != Ordering::Equal {
            return order;
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Apk {
    numbers: Vec<u64>,
    letter: Option<char>,
    /// Each suffix's rank (negative before the release) and number; the
    /// release itself ends the list with rank 0.
    suffixes: Vec<(i8, u64)>,
    revision: u64,
}

fn apk_of(value: &str) -> Option<Apk> {
    let value = value.trim();
    let (body, revision) = match value.rsplit_once("-r") {
        Some((body, revision)) => (body, revision.parse().ok()?),
        None => (value, 0),
    };
    let mut parts = body.split('_');
    let mut head = parts.next()?;
    let letter = head
        .chars()
        .last()
        .filter(char::is_ascii_lowercase)
        .inspect(|_| head = &head[..head.len() - 1]);
    let numbers = head
        .split('.')
        .map(|number| number.parse().ok())
        .collect::<Option<Vec<u64>>>()?;
    let mut suffixes = parts
        .map(|suffix| {
            let name = suffix.trim_end_matches(|c: char| c.is_ascii_digit());
            let rank = match name {
                "alpha" => -4,
                "beta" => -3,
                "pre" => -2,
                "rc" => -1,
                "cvs" => 1,
                "svn" => 2,
                "git" => 3,
                "hg" => 4,
                "p" => 5,
                _ => return None,
            };
            let number = &suffix[name.len()..];
            Some((rank, if number.is_empty() { 0 } else { number.parse().ok()? }))
        })
        .collect::<Option<Vec<_>>>()?;
    suffixes.push((0, 0));
    Some(Apk {
        numbers,
        letter,
        suffixes,
        revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ascending(order: VersionOrder, versions: &[&str]) {
        for pair in versions.windows(2) {
            assert_eq!((order.cmp)(pair[0], pair[1]), Some(Ordering::Less), "{pair:?}");
            assert_eq!((order.cmp)(pair[1], pair[0]), Some(Ordering::Greater), "{pair:?}");
        }
        for version in versions {
            assert_eq!((order.cmp)(version, version), Some(Ordering::Equal), "{version}");
        }
    }

    #[test]
    fn dpkg_orders_debian_and_ubuntu_versions() {
        ascending(
            DPKG,
            &[
                "1.0~rc1",
                "1.0",
                "1.0-1",
                "1.1.1n-0+deb11u3",
                "1.1.1n-0+deb11u4",
                "1.1.1n-0+deb11u10",
                "1.1.1w-0+deb11u1",
                "3.0.2-0ubuntu1",
                "3.0.2-0ubuntu1.10",
                "3.0.11-1~deb12u1",
                "3.0.11-1",
                "1:0.9",
            ],
        );
        assert!((DPKG.prerelease)("1.0~rc1"));
        assert!(!(DPKG.prerelease)("1.0-1~deb12u1"));
        assert_eq!((DPKG.cmp)("not a version", "1.0"), None);
    }

    #[test]
    fn apk_orders_alpine_versions() {
        ascending(
            APK,
            &[
                "3.1.0_alpha",
                "3.1.0_rc2",
                "3.1.0-r0",
                "3.1.0-r4",
                "3.1.0_p1-r0",
                "3.1.0a-r0",
                "3.1.1-r0",
                "3.10.0-r0",
            ],
        );
        assert!((APK.prerelease)("3.1.0_rc2"));
        assert!(!(APK.prerelease)("3.1.0-r4"));
        assert_eq!((APK.cmp)("3.1.0_bogus", "3.1.0"), None);
    }
}
