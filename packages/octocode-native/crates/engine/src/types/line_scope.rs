//! A text search kept to the lines a scope names in each file.

/// Keep only hit lines inside each file's kept spans (for example the lines
/// of the value at a key path in a JSON or TOML document). A file the scope
/// cannot hold any kept line of is left out unread; any other has its spans
/// read once, on its first hit, so a file without hits is never parsed.
#[derive(Clone)]
pub struct LineScope {
    /// Whether a file can hold kept lines, decided without reading it.
    pub holds: std::sync::Arc<super::test_code::PathTest>,
    /// A file's kept 1-based line spans, inclusive, in line order.
    pub spans: std::sync::Arc<super::test_code::LineSpans>,
}

impl LineScope {
    /// Whether 1-based `line` lies in one of `spans`.
    #[must_use]
    pub fn keeps(spans: &[[u32; 2]], line: u32) -> bool {
        let at = spans.partition_point(|[_, end]| *end < line);
        spans.get(at).is_some_and(|[start, _]| *start <= line)
    }
}

impl std::fmt::Debug for LineScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LineScope").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
