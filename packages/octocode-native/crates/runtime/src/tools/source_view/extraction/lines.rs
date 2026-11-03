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

    pub(super) fn start(&self, line: usize) -> usize {
        self.0[line]
    }

    pub(super) fn spans(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.0.windows(2).map(|pair| (pair[0], pair[1]))
    }

    pub(super) fn range<'a>(&self, content: &'a str, first: usize, last: usize) -> &'a str {
        &content[self.0[first - 1]..self.0[last]]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_select_exact_source_bytes_for_every_line_shape() {
        for source in [
            "",
            "\n",
            "a",
            "a\n",
            "a\n\n",
            "café\r\n😀\nλ\rfinal",
            "\r\n\r\n",
        ] {
            let offsets = LineOffsets::new(source);
            let expected = source.split_inclusive('\n').collect::<Vec<_>>();
            assert_eq!(offsets.len(), expected.len(), "{source:?}");
            for first in 1..=expected.len() {
                for last in first..=expected.len() {
                    assert_eq!(
                        offsets.range(source, first, last),
                        expected[first - 1..last].concat(),
                        "{source:?}, {first}..={last}"
                    );
                }
            }
        }
    }
}
