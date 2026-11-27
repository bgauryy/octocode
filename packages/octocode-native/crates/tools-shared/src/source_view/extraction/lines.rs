//! Contiguous byte boundaries for source lines. The source remains with the
//! request; this table contains no strings or per-line allocations.

pub(crate) struct LineOffsets(Vec<usize>);

impl LineOffsets {
    pub(crate) fn new(content: &str) -> Self {
        let mut offsets = octocode_engine::text::line_starts(content);
        if offsets.last() != Some(&content.len()) {
            offsets.push(content.len());
        }
        Self(offsets)
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len() - 1
    }

    /// Bytes kept by this index, including spare capacity in its table.
    pub(in crate::source_view) fn weight(&self) -> usize {
        std::mem::size_of::<Self>() + self.0.capacity() * std::mem::size_of::<usize>()
    }

    pub(in crate::source_view) fn start(&self, line: usize) -> usize {
        self.0[line]
    }

    /// The 0-based line holding byte `at` (the last line past the end).
    pub(in crate::source_view) fn line_at(&self, at: usize) -> usize {
        self.0
            .partition_point(|start| *start <= at)
            .clamp(1, self.len().max(1))
            - 1
    }

    pub(super) fn spans(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.0.windows(2).map(|pair| (pair[0], pair[1]))
    }

    pub(super) fn range<'a>(&self, content: &'a str, first: usize, last: usize) -> &'a str {
        &content[self.0[first - 1]..self.0[last]]
    }
}

#[cfg(test)]
mod tests;
