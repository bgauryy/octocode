use octocode_engine::search::walk::IgnoreRules;
use std::path::Path;

use super::reasons::{below, skip_of};
use super::{Cause, Diagnosis, Hits, SAMPLE_FILES, Skip, Verdict, Walk};

/// Finds which of the causes `walk` skips by held matches. `probe` runs the
/// tool's own walk (files only) with the given relaxed [`Walk`] and says what
/// it found; it is called with everything lifted first (one call when
/// nothing is found), then with each smaller set of lifted causes by size
/// until one finds matches, so the answer is the smallest sets that do (the
/// union of those that reach different files).
pub(crate) fn diagnose<E>(
    root: &Path,
    walk: Walk,
    probe: impl FnMut(Walk) -> Result<Hits, E>,
) -> Result<Verdict, E> {
    diagnose_shown(root, walk, &|path| below(root, path), probe)
}

/// [`diagnose`] whose reasons spell paths with `shown` (a tool whose rows
/// name paths relative to the workspace names its rules the same way).
pub(crate) fn diagnose_shown<E>(
    root: &Path,
    walk: Walk,
    shown: &dyn Fn(&Path) -> String,
    mut probe: impl FnMut(Walk) -> Result<Hits, E>,
) -> Result<Verdict, E> {
    let off = walk.skipping();
    if off.is_empty() {
        return Ok(Verdict::Absent);
    }
    let subset = |mask: usize| -> Vec<Cause> {
        off.iter()
            .enumerate()
            .filter(|(index, _)| mask >> index & 1 == 1)
            .map(|(_, cause)| *cause)
            .collect()
    };
    let full = (1_usize << off.len()) - 1;
    let everything = probe(walk.lifting(&off))?;
    if everything.count == 0 {
        return Ok(if everything.cut {
            Verdict::Unproven
        } else {
            Verdict::Absent
        });
    }
    // Smallest sets first; a larger set is probed only while the sets found
    // so far reach fewer files than lifting everything does, and kept only
    // when it reaches more than the found sets inside it (a file behind two
    // causes, beside files behind one of them).
    let mut revealed: Vec<(usize, Hits)> = Vec::new();
    let within = |revealed: &[(usize, Hits)], mask: usize| -> usize {
        revealed
            .iter()
            .filter(|(known, _)| known & mask == *known)
            .map(|(_, found)| found.count)
            .sum()
    };
    for size in 1..off.len() {
        if within(&revealed, full) >= everything.count {
            break;
        }
        let mut level = Vec::new();
        for mask in (1..full).filter(|mask| mask.count_ones() as usize == size) {
            let found = probe(walk.lifting(&subset(mask)))?;
            if found.count > within(&revealed, mask) {
                level.push((mask, found));
            }
        }
        revealed.extend(level);
    }
    if revealed.is_empty() {
        revealed.push((full, everything));
    }
    // Causes that reach the same files are alternatives (a walk that couples
    // two flags): the first of them in cause order is enough.
    // A larger set kept above reaches files its subsets do not, whatever its
    // sample shows.
    let mut covered = std::collections::BTreeSet::new();
    let mut kept: Vec<(usize, Hits)> = Vec::new();
    for (mask, found) in revealed {
        let new = found.sample.iter().any(|file| !covered.contains(file))
            || (kept.iter().any(|(known, _)| known & mask == *known)
                && found.count > within(&kept, mask));
        covered.extend(found.sample.iter().cloned());
        if new {
            kept.push((mask, found));
        }
    }
    let revealed = kept;
    let union = revealed.iter().fold(0, |union, (mask, _)| union | mask);
    let found = match revealed.iter().find(|(mask, _)| *mask == union) {
        Some((_, found)) => found.clone(),
        None => {
            let found = probe(walk.lifting(&subset(union)))?;
            if found.count > 0 {
                found
            } else {
                revealed[0].1.clone()
            }
        }
    };
    let mut skips: Vec<Skip> = Vec::new();
    let rules = std::cell::OnceCell::new();
    for (mask, found) in &revealed {
        let causes = subset(*mask);
        for file in found.sample.iter().take(SAMPLE_FILES) {
            let rules = rules.get_or_init(|| IgnoreRules::new(root));
            if let Some(skip) = skip_of(root, rules, file, &causes, shown)
                && !skips.iter().any(|known| known.reason == skip.reason)
            {
                skips.push(skip);
            }
        }
    }
    Ok(Verdict::Skipped(Diagnosis {
        causes: subset(union),
        found,
        skips,
    }))
}
