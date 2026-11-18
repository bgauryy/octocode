//! The lowest fixed version that clears every listed advisory, when OSV's data
//! says so. Fixed versions are compared in the ecosystem registry's own version
//! order ([`Order`]): a prerelease fix (`13.4.20-canary.13`) is a fix like any
//! other. An ecosystem no registry orders is compared as dotted numbers only,
//! and any other spelling says why no answer exists, rather than guess at its
//! ordering. OSV then answers at that version, so the claim is its own.
use super::record::SecurityAdvisory;
use crate::artifact::VersionOrder;
use std::cmp::Ordering;

/// `4.17.21` as numbers, a leading `v` allowed; `None` for any other spelling.
fn numeric(version: &str) -> Option<Vec<u64>> {
    version
        .strip_prefix('v')
        .unwrap_or(version)
        .split('.')
        .map(|part| part.parse().ok())
        .collect()
}

fn compare(left: &[u64], right: &[u64]) -> Ordering {
    let at = |list: &[u64], index: usize| list.get(index).copied().unwrap_or(0);
    (0..left.len().max(right.len()))
        .map(|index| at(left, index).cmp(&at(right, index)))
        .find(|order| order.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// How an ecosystem's versions compare: its registry's order when a registry
/// provider reads the ecosystem, else dotted numbers.
#[derive(Clone, Copy)]
pub(super) struct Order(Option<VersionOrder>);

impl Order {
    /// The registry's order, or dotted numbers when there is none.
    pub(super) const fn of(order: Option<VersionOrder>) -> Self {
        Self(order)
    }

    /// The precedence of two versions; `None` when either is not a version
    /// this order reads.
    fn cmp(self, left: &str, right: &str) -> Option<Ordering> {
        match self.0 {
            Some(order) => (order.cmp)(left, right),
            None => Some(compare(&numeric(left)?, &numeric(right)?)),
        }
    }

    fn readable(self, version: &str) -> bool {
        self.cmp(version, version).is_some()
    }

    /// Whether two spellings name one version (`v1.2.3` and `1.2.3`).
    pub(super) fn same(self, left: &str, right: &str) -> bool {
        left == right || self.cmp(left, right) == Some(Ordering::Equal)
    }

    fn above(self, version: &str, floor: &str) -> bool {
        self.cmp(version, floor) == Some(Ordering::Greater)
    }

    /// Whether `version` is a prerelease in this order; never, in dotted
    /// numbers, which read no prerelease.
    pub(super) fn prerelease(self, version: &str) -> bool {
        self.0.is_some_and(|order| (order.prerelease)(version))
    }

    /// The lowest of `versions` above `floor`, each read in this order.
    fn lowest_above<'a>(
        self,
        floor: &str,
        versions: impl IntoIterator<Item = &'a str>,
    ) -> Option<&'a str> {
        versions
            .into_iter()
            .filter(|version| self.above(version, floor))
            .min_by(|left, right| self.cmp(left, right).unwrap_or(Ordering::Equal))
    }
}

/// The leading number of a version (`13` of `v13.4.20-canary.13`): its major
/// line.
fn major(version: &str) -> Option<u64> {
    let version = version.trim().trim_start_matches(['v', 'V']);
    version
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

/// What moving from one version to another changes, compared as dotted numbers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Move {
    /// The target is the same version (or older): nothing to move to.
    None,
    Patch,
    Minor,
    Major,
    /// Either side is not a plain dotted version: never guessed.
    Unknown,
}

impl Move {
    /// The contract's `upgradeKind` value; `None` when there is no upgrade.
    pub(super) fn kind(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Patch => Some("patch"),
            Self::Minor => Some("minor"),
            Self::Major => Some("major"),
            Self::Unknown => Some("unknown"),
        }
    }
}

/// What moving from `current` to `latest` is: the first dotted component that
/// differs names the kind, a third or later one being a patch.
pub(super) fn movement(current: &str, latest: &str) -> Move {
    let (Some(from), Some(to)) = (numeric(current), numeric(latest)) else {
        return Move::Unknown;
    };
    if !compare(&to, &from).is_gt() {
        return Move::None;
    }
    let at = |list: &[u64], index: usize| list.get(index).copied().unwrap_or(0);
    match (0..2).find(|&index| at(&from, index) != at(&to, index)) {
        Some(0) => Move::Major,
        Some(_) => Move::Minor,
        None => Move::Patch,
    }
}

/// `version` written as `current` writes its own, with or without the `v`.
fn written_as(current: &str, version: &str) -> String {
    if current.starts_with('v') && !version.starts_with('v') {
        format!("v{version}")
    } else {
        version.to_owned()
    }
}

/// The version to try, or why there is none: for each advisory its lowest stated
/// fix above `current` (a prerelease fix included), and the highest of those.
/// Written as `current` writes its own version, with or without the `v`.
pub(super) fn candidate(
    current: &str,
    advisories: &[SecurityAdvisory],
    order: Order,
) -> Result<String, String> {
    if !order.readable(current) {
        return Err(format!(
            "{current} is not a version this ecosystem's order reads, so fixed versions were not compared"
        ));
    }
    let mut needed: Option<&str> = None;
    for advisory in advisories {
        let lowest = order
            .lowest_above(current, advisory.fixed_in.iter().map(String::as_str))
            .ok_or_else(|| format!("{} states no fixed version above {current}", advisory.id))?;
        if needed.is_none_or(|known| order.above(lowest, known)) {
            needed = Some(lowest);
        }
    }
    let version = needed.ok_or_else(|| "no advisory is listed".to_owned())?;
    Ok(written_as(current, version))
}

/// The lowest of `published` above `version`: the release to step to when
/// `version` is not published, was yanked, or is a prerelease. A prerelease
/// is a step only when `prereleases` is true (the version asked is one).
/// Written as `published` writes it.
pub(super) fn next_published(
    version: &str,
    published: &[String],
    order: Order,
    prereleases: bool,
) -> Option<String> {
    order
        .lowest_above(
            version,
            published
                .iter()
                .map(String::as_str)
                .filter(|release| prereleases || !order.prerelease(release)),
        )
        .map(str::to_owned)
}

/// The fix that decides for `current`: the lowest listed fix above it in its
/// own major line, else the lowest above it in a later one. The other lines'
/// fixes stay on the record's `advisory` row. A list is kept whole when
/// `current` or every fix is not a version the order reads, or none is above
/// it: nothing is chosen by guess.
pub(super) fn deciding_fix(current: &str, fixed_in: &mut Vec<String>, order: Order) {
    if !order.readable(current) {
        return;
    }
    let line = major(current);
    let lowest = |own_line: bool| {
        order
            .lowest_above(
                current,
                fixed_in
                    .iter()
                    .map(String::as_str)
                    .filter(|fixed| !own_line || major(fixed) == line),
            )
            .map(str::to_owned)
    };
    if let Some(fix) = lowest(true).or_else(|| lowest(false)) {
        *fixed_in = vec![fix];
    }
}

/// The advisories no release of `current`'s major is listed as fixing, when
/// `named` (the upgrade) is in another major, and the highest fix that major
/// does list for the others. `None` when the named version stays in the
/// current major or a version is not one the order reads.
fn outside_major<'a>(
    current: &str,
    named: &str,
    advisories: &'a [SecurityAdvisory],
    order: Order,
) -> Option<(u64, Vec<&'a str>, Option<&'a str>)> {
    if !order.readable(current) || !order.readable(named) {
        return None;
    }
    let line = major(current)?;
    if major(named)? == line {
        return None;
    }
    let mut unfixed: Vec<&str> = Vec::new();
    let mut within: Option<&str> = None;
    for advisory in advisories {
        let lowest = order.lowest_above(
            current,
            advisory
                .fixed_in
                .iter()
                .map(String::as_str)
                .filter(|fixed| major(fixed) == Some(line)),
        );
        match lowest {
            // A twin record of the same id is the same advisory: named once.
            None if unfixed.contains(&advisory.id.as_str()) => {}
            None => unfixed.push(advisory.id.as_str()),
            Some(lowest) => {
                if within.is_none_or(|known| order.above(lowest, known)) {
                    within = Some(lowest);
                }
            }
        }
    }
    Some((line, unfixed, within))
}

/// Ids of the advisories only a later major than `current`'s clears, when
/// the upgrade `named` crosses a major: what [`major_gap`] names in words.
pub(super) fn unfixed_in_major<'a>(
    current: &str,
    named: &str,
    advisories: &'a [SecurityAdvisory],
    order: Order,
) -> Vec<&'a str> {
    outside_major(current, named, advisories, order)
        .map_or_else(Vec::new, |(_, unfixed, _)| unfixed)
}

/// What is said beside an upgrade that crosses a major version: the advisories
/// no release of the current major is listed as fixing (so only a later
/// major clears them), and the highest fix the current major does list for the
/// others. `None` when the named version stays in the current major, or the
/// current major clears every advisory.
pub(super) fn major_gap(
    current: &str,
    named: &str,
    advisories: &[SecurityAdvisory],
    order: Order,
) -> Option<String> {
    let (major, unfixed, within) = outside_major(current, named, advisories, order)?;
    if unfixed.is_empty() {
        return None;
    }
    let mut note = format!(
        "No {major}.x release is listed as fixing {}; {named} is the lowest listed fix in a later major.",
        unfixed.join(", ")
    );
    if let Some(version) = within {
        note.push_str(&format!(
            " {version} is the highest fix listed within {major}.x, for the other advisories."
        ));
    }
    Some(note)
}

#[cfg(test)]
#[path = "upgrade_tests.rs"]
mod tests;
