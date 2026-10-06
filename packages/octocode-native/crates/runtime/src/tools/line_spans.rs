//! 1-based inclusive line spans: merging, run compression, the `a-b` range
//! text, and the `moreLines` run list (`711-717,802,+40 more`) that
//! localSearch writes and clasify reads back.

/// A line number type the span helpers accept.
pub(crate) trait LineNumber: Copy + Ord + std::fmt::Display {
    /// The next line, saturating at the type's maximum.
    fn succ(self) -> Self;
    /// Lines in `start..=end`, for counts.
    fn span_len(start: Self, end: Self) -> u64;
}

macro_rules! line_number {
    ($($ty:ty),*) => {$(
        impl LineNumber for $ty {
            fn succ(self) -> Self {
                self.saturating_add(1)
            }
            fn span_len(start: Self, end: Self) -> u64 {
                u64::try_from(end - start).unwrap_or(u64::MAX) + 1
            }
        }
    )*};
}
line_number!(u32, u64, usize);

/// `spans` sorted, with overlapping or adjacent ones merged.
pub(crate) fn merge_spans<T: LineNumber>(spans: impl IntoIterator<Item = (T, T)>) -> Vec<(T, T)> {
    let mut spans = spans.into_iter().collect::<Vec<_>>();
    spans.sort_unstable();
    let mut merged: Vec<(T, T)> = Vec::new();
    for (start, end) in spans {
        match merged.last_mut() {
            Some(last) if start <= last.1.succ() => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Consecutive runs of `lines` (in the given order): `[3,4,5,9]` →
/// `[(3,5),(9,9)]`. A repeated line starts a new run.
pub(crate) fn runs<T: LineNumber>(lines: impl IntoIterator<Item = T>) -> Vec<(T, T)> {
    let mut out: Vec<(T, T)> = Vec::new();
    for line in lines {
        match out.last_mut() {
            Some((_, end)) if line == end.succ() => *end = line,
            _ => out.push((line, line)),
        }
    }
    out
}

/// One `start-end` range text as numbers.
pub(crate) fn parse_span<T: std::str::FromStr>(range: &str) -> Option<(T, T)> {
    let (start, end) = range.split_once('-')?;
    Some((start.parse().ok()?, end.parse().ok()?))
}

/// Sorted, distinct line numbers as runs: `711-717,802`. Past `max_ranges`
/// runs the rest is a count (`,+40 more`), so a file with thousands of
/// scattered hits costs a bounded hint, and the match pages still hold them.
pub(crate) fn more_lines(lines: &[u32], max_ranges: usize) -> String {
    let runs = runs(lines.iter().copied());
    let mut out = runs
        .iter()
        .take(max_ranges)
        .map(|&(start, end)| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}-{end}")
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let omitted: u64 = runs
        .iter()
        .skip(max_ranges)
        .map(|&(start, end)| u32::span_len(start, end))
        .sum();
    if omitted > 0 {
        out.push_str(&format!(",+{omitted} more"));
    }
    out
}

/// Every line a [`more_lines`] text names; the trailing `+N more` count names
/// no line and is skipped.
pub(crate) fn more_lines_named(text: &str) -> impl Iterator<Item = u64> + '_ {
    text.split(',').flat_map(|run| {
        let run = run.trim();
        let (start, end) = run.split_once('-').unwrap_or((run, run));
        match (start.parse::<u64>(), end.parse::<u64>()) {
            (Ok(start), Ok(end)) => Some(start..=end),
            _ => None,
        }
        .into_iter()
        .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_lines_compress_runs_and_cap_scattered_hits_with_a_count() {
        assert_eq!(more_lines(&[2], 24), "2");
        assert_eq!(
            more_lines(&[711, 712, 713, 714, 715, 716, 717, 802], 24),
            "711-717,802"
        );
        let scattered = (1..=100).map(|n| n * 10).collect::<Vec<u32>>();
        assert_eq!(more_lines(&scattered, 3), "10,20,30,+97 more");
    }

    #[test]
    fn more_lines_round_trip_names_every_shown_line() {
        let text = more_lines(&[711, 712, 713, 802, 900], 2);
        assert_eq!(text, "711-713,802,+1 more");
        assert_eq!(
            more_lines_named(&text).collect::<Vec<_>>(),
            vec![711, 712, 713, 802]
        );
    }

    #[test]
    fn spans_merge_when_overlapping_or_adjacent() {
        assert_eq!(
            merge_spans([(10_u32, 12), (1, 3), (4, 5), (11, 20)]),
            vec![(1, 5), (10, 20)]
        );
        assert_eq!(runs([3_usize, 4, 5, 9, 9]), vec![(3, 5), (9, 9), (9, 9)]);
        assert_eq!(parse_span::<u64>("95-105"), Some((95, 105)));
        assert_eq!(parse_span::<u64>("95"), None);
    }
}
