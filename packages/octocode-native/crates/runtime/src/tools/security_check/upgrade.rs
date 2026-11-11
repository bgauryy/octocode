//! The lowest fixed version that clears every listed advisory, when OSV's data
//! says so. Each advisory's versions are compared as dotted numbers only; any
//! other spelling says why no answer exists, rather than guess at an ecosystem's
//! ordering. OSV then answers at that version, so the claim is its own.
use super::record::SecurityAdvisory;
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

/// The version to try, or why there is none: for each advisory its lowest stated
/// fix above `current`, and the highest of those. Written as `current` writes
/// its own version, with or without the `v`.
pub(super) fn candidate(current: &str, advisories: &[SecurityAdvisory]) -> Result<String, String> {
    let at = numeric(current).ok_or_else(|| {
        format!("{current} is not a plain dotted version, so fixed versions were not compared")
    })?;
    let mut needed: Option<(Vec<u64>, &str)> = None;
    for advisory in advisories {
        let lowest = advisory
            .fixed_in
            .iter()
            .filter_map(|fixed| Some((numeric(fixed)?, fixed.as_str())))
            .filter(|(numbers, _)| compare(numbers, &at).is_gt())
            .min_by(|(left, _), (right, _)| compare(left, right))
            .ok_or_else(|| format!("{} states no fixed version above {current}", advisory.id))?;
        if needed
            .as_ref()
            .is_none_or(|(numbers, _)| compare(&lowest.0, numbers).is_gt())
        {
            needed = Some(lowest);
        }
    }
    let (_, version) = needed.ok_or_else(|| "no advisory is listed".to_owned())?;
    Ok(if current.starts_with('v') && !version.starts_with('v') {
        format!("v{version}")
    } else {
        version.to_owned()
    })
}

/// The fix that decides for `current`: the lowest listed fix above it in its
/// own major line, else the lowest above it in a later one. The other lines'
/// fixes stay on the record's `advisory` row. A list is kept whole when
/// `current` or every fix is not plain dotted numbers, or none is above it:
/// nothing is chosen by guess.
pub(super) fn deciding_fix(current: &str, fixed_in: &mut Vec<String>) {
    let Some(at) = numeric(current) else {
        return;
    };
    let above = fixed_in
        .iter()
        .filter_map(|fixed| Some((numeric(fixed)?, fixed)))
        .filter(|(numbers, _)| compare(numbers, &at).is_gt());
    let lowest = |own_line: bool| {
        above
            .clone()
            .filter(|(numbers, _)| !own_line || numbers.first() == at.first())
            .min_by(|(left, _), (right, _)| compare(left, right))
            .map(|(_, fixed)| fixed.clone())
    };
    if let Some(fix) = lowest(true).or_else(|| lowest(false)) {
        *fixed_in = vec![fix];
    }
}

/// The advisories no release of `current`'s major is listed as fixing, when
/// `named` (the upgrade) is in another major, and the highest fix that major
/// does list for the others. `None` when the named version stays in the
/// current major or a version is not plain dotted numbers.
fn outside_major<'a>(
    current: &str,
    named: &str,
    advisories: &'a [SecurityAdvisory],
) -> Option<(u64, Vec<&'a str>, Option<&'a str>)> {
    let at = numeric(current)?;
    let major = *at.first()?;
    if *numeric(named)?.first()? == major {
        return None;
    }
    let mut unfixed: Vec<&str> = Vec::new();
    let mut within: Option<(Vec<u64>, &str)> = None;
    for advisory in advisories {
        let lowest = advisory
            .fixed_in
            .iter()
            .filter_map(|fixed| Some((numeric(fixed)?, fixed.as_str())))
            .filter(|(numbers, _)| numbers.first() == Some(&major) && compare(numbers, &at).is_gt())
            .min_by(|(left, _), (right, _)| compare(left, right));
        match lowest {
            // A twin record of the same id is the same advisory: named once.
            None if unfixed.contains(&advisory.id.as_str()) => {}
            None => unfixed.push(advisory.id.as_str()),
            Some(lowest) => {
                if within
                    .as_ref()
                    .is_none_or(|(numbers, _)| compare(&lowest.0, numbers).is_gt())
                {
                    within = Some(lowest);
                }
            }
        }
    }
    Some((major, unfixed, within.map(|(_, version)| version)))
}

/// Ids of the advisories only a later major than `current`'s clears, when
/// the upgrade `named` crosses a major: what [`major_gap`] names in words.
pub(super) fn unfixed_in_major<'a>(
    current: &str,
    named: &str,
    advisories: &'a [SecurityAdvisory],
) -> Vec<&'a str> {
    outside_major(current, named, advisories).map_or_else(Vec::new, |(_, unfixed, _)| unfixed)
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
) -> Option<String> {
    let (major, unfixed, within) = outside_major(current, named, advisories)?;
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
