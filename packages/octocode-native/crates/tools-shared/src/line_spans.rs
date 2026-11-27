//! 1-based inclusive line spans: merging, run compression, the `a-b` range
//! text.

/// A line number type the span helpers accept.
pub trait LineNumber: Copy + Ord + std::fmt::Display {
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
pub fn merge_spans<T: LineNumber>(spans: impl IntoIterator<Item = (T, T)>) -> Vec<(T, T)> {
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

#[cfg(test)]
mod tests;
