//! 1-based inclusive line spans: merging, run compression, the `a-b` range
//! text, and the bounded `moreLines` list localSearch writes.

/// A line number type the span helpers accept.
pub(crate) trait LineNumber: Copy + Ord + std::fmt::Display {
    /// The next line, saturating at the type's maximum.
    fn succ(self) -> Self;
}

macro_rules! line_number {
    ($($ty:ty),*) => {$(
        impl LineNumber for $ty {
            fn succ(self) -> Self {
                self.saturating_add(1)
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

/// The first `max_lines` of sorted, distinct `lines`, and how many more
/// there are: a file with thousands of scattered hits costs a bounded
/// hint, and the match pages still hold the rest.
pub(crate) fn more_lines(lines: &[u32], max_lines: usize) -> (Vec<u32>, Option<u32>) {
    let mut distinct = lines.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    let unlisted = distinct.len().saturating_sub(max_lines);
    distinct.truncate(max_lines);
    (
        distinct,
        (unlisted > 0).then(|| u32::try_from(unlisted).unwrap_or(u32::MAX)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_lines_list_the_first_lines_and_count_the_rest() {
        assert_eq!(more_lines(&[2], 24), (vec![2], None));
        assert_eq!(
            more_lines(&[717, 711, 712, 711, 802], 24),
            (vec![711, 712, 717, 802], None)
        );
        let scattered = (1..=100).map(|n| n * 10).collect::<Vec<_>>();
        assert_eq!(more_lines(&scattered, 3), (vec![10, 20, 30], Some(97)));
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
