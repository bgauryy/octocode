//! How a hit fits a read: the covering window, goal terms, hit scores and the match anchors.
use super::blob::Hit;

/// Widest span (in lines) of a window that anchors a multi-keyword read.
const MAX_WINDOW_LINES: usize = 60;

/// The hits (`start..=end` indexes into `hits`) of the smallest window of at
/// most [`MAX_WINDOW_LINES`] lines holding every keyword, preferring one
/// opened by a declaration (`declares`), then the narrowest, then the
/// earliest. `None` when no such window exists.
pub(super) fn covering_window(
    hits: &[Hit],
    lowered: &[String],
    needles: &[String],
    declares: impl Fn(usize) -> bool,
) -> Option<(usize, usize)> {
    if needles.is_empty() {
        return (0..hits.len())
            .min_by_key(|&index| !declares(index))
            .map(|index| (index, index));
    }
    let coverage = KeywordCoverage::new(hits, lowered, needles);
    let mut counts = vec![0usize; needles.len()];
    let mut missing = needles.len();
    let mut end = 0;
    // ((opened by no declaration, span, start), (start, end)).
    type Ranked = ((bool, usize, usize), (usize, usize));
    let mut chosen: Option<Ranked> = None;
    for start in 0..hits.len() {
        while missing != 0
            && end < hits.len()
            && hits[end].index - hits[start].index <= MAX_WINDOW_LINES
        {
            for keyword in coverage.at(end) {
                if counts[keyword] == 0 {
                    missing -= 1;
                }
                counts[keyword] += 1;
            }
            end += 1;
        }
        if missing == 0 {
            let last = end - 1;
            let span = hits[last].index - hits[start].index;
            // A declaration cannot improve a narrower or earlier declaration
            // window. Prove that before its full-file uniqueness check.
            if chosen
                .as_ref()
                .is_none_or(|(best, _)| (false, span, start) < *best)
            {
                let rank = (!declares(start), span, start);
                if chosen.as_ref().is_none_or(|(best, _)| rank < *best) {
                    chosen = Some((rank, (start, last)));
                }
            }
        }
        for keyword in coverage.at(start) {
            counts[keyword] -= 1;
            if counts[keyword] == 0 {
                missing += 1;
            }
        }
    }
    chosen.map(|(_, window)| window)
}

/// Each hit occupies consecutive membership words. Keyword ids are set bits,
/// so dense matches use one bit per keyword rather than one machine index.
/// Window movement updates dense counters without repeating substring searches.
struct KeywordCoverage {
    words: Vec<u64>,
    width: usize,
}

impl KeywordCoverage {
    fn new(hits: &[Hit], lowered: &[String], needles: &[String]) -> Self {
        let width = needles.len().div_ceil(u64::BITS as usize);
        let mut words = vec![0; hits.len() * width];
        for (hit, row) in hits.iter().zip(words.chunks_exact_mut(width)) {
            for (index, needle) in needles.iter().enumerate() {
                if lowered[hit.index].contains(needle.as_str()) {
                    row[index / u64::BITS as usize] |= 1 << (index % u64::BITS as usize);
                }
            }
        }
        Self { words, width }
    }

    fn at(&self, hit: usize) -> impl Iterator<Item = usize> + '_ {
        self.words[hit * self.width..(hit + 1) * self.width]
            .iter()
            .enumerate()
            .flat_map(|(word, &bits)| {
                let mut bits = bits;
                std::iter::from_fn(move || {
                    if bits == 0 {
                        return None;
                    }
                    let bit = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    Some(word * u64::BITS as usize + bit)
                })
            })
    }
}

/// Lines on each side of a hit searched for the goal's other terms.
const GOAL_CONTEXT_LINES: usize = 8;

/// Goal words (three or more characters, lowercase) that are not part of a
/// searched keyword and are rare in this file: on at most one line in
/// [`COMMON_TERM_LINES`] (and at least one line allowed). A word on more
/// lines (`the`, `use`, a type every line names) cannot tell one hit's
/// neighborhood from another's.
pub(super) fn goal_terms(goal: &str, needles: &[String], lowered: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = goal
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3)
        .filter(|word| !needles.iter().any(|needle| needle.contains(word)))
        .map(str::to_owned)
        .collect();
    terms.sort_unstable();
    terms.dedup();
    let common = (lowered.len() / COMMON_TERM_LINES).max(1);
    terms.retain(|term| {
        lowered
            .iter()
            .filter(|line| line.contains(term.as_str()))
            .count()
            <= common
    });
    terms
}

/// A goal term on more than one line in this many is too common to locate.
const COMMON_TERM_LINES: usize = 20;

/// How well hit line `index` (keyword ending at byte `after`; `declares`:
/// the keyword is a declared name) fits the goal, ordered by the hit's own
/// shape first (a literal value assigned to the keyword, `x = 512` or
/// `x: 512`, 3; any other assignment or declaration 2; other code 1; a
/// comment 0), then by the goal terms named within [`GOAL_CONTEXT_LINES`].
/// Shape leads because neighboring prose names goal terms near uses and
/// definitions alike.
pub(super) fn hit_score(
    lowered: &[String],
    index: usize,
    after: usize,
    declares: bool,
    terms: &[String],
) -> (usize, usize) {
    let window = &lowered[index.saturating_sub(GOAL_CONTEXT_LINES)
        ..(index + GOAL_CONTEXT_LINES + 1).min(lowered.len())];
    let named = terms
        .iter()
        .filter(|term| window.iter().any(|line| line.contains(term.as_str())))
        .count();
    (hit_shape(&lowered[index], after, declares), named)
}

fn hit_shape(lower: &str, after: usize, declares: bool) -> usize {
    let code = lower.trim_start();
    if ["//", "#", "/*", "*", "--", "<!--", "\"\"\""]
        .iter()
        .any(|marker| code.starts_with(marker))
    {
        return 0;
    }
    let rest = lower.get(after..).unwrap_or_default().trim_start();
    let value = rest
        .strip_prefix(":=")
        .or_else(|| {
            rest.strip_prefix('=')
                .filter(|value| !value.starts_with(['=', '>']))
        })
        .or_else(|| {
            rest.strip_prefix(':')
                .filter(|value| !value.starts_with(':'))
        })
        .map(str::trim_start);
    if let Some(value) = value {
        let literal = value
            .starts_with(|c: char| c.is_ascii_digit() || matches!(c, '"' | '\'' | '`'))
            || (value.starts_with('-') && value[1..].starts_with(|c: char| c.is_ascii_digit()))
            || ["true", "false", "none", "null", "nil"]
                .iter()
                .any(|word| value.starts_with(word));
        return if literal { 3 } else { 2 };
    }
    if declares { 2 } else { 1 }
}

#[cfg(test)]
mod tests;
