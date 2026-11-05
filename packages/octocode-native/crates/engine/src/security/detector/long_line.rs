//! A hit on one very long line: what a row shows of it. The line is judged whole, so
//! no value sits across an edge; the row keeps a window around the hit, with only
//! a secret's own bytes hidden.
use super::long_text::secret_spans;
use super::{Scope, replacement_for};
use std::ops::Range;

/// A line past this shows a window around its hit in a row: below it a row holds the
/// line whole. Equal to what a read holds of a source at once.
pub const LONG_LINE_BYTES: usize = super::MAX_CONTENT_SIZE;

/// Bytes of a line a row shows on each side of its hit.
const WINDOW_SIDE_BYTES: usize = 8 * 1024;

/// One long line's secrets, judged once for all the rows it gives.
pub struct LongLine {
    spans: Vec<(Range<usize>, usize)>,
}

impl LongLine {
    /// The secrets of `text`, the line of the file at `path` (`None`: no known file).
    #[must_use]
    pub fn judged(text: &str, path: Option<&str>) -> Self {
        let scope = path.map_or(Scope::Pathless, Scope::File);
        Self {
            spans: secret_spans(text, scope),
        }
    }

    /// `text[hit]` as a row shows it, each secret's part of it replaced by its
    /// placeholder.
    #[must_use]
    pub fn cut(&self, text: &str, hit: Range<usize>) -> String {
        self.with_placeholders(text, hit)
    }

    /// A row's value for a hit at `hit` of `text`: the bytes around it, marked as a
    /// window of the line.
    #[must_use]
    pub fn window(&self, text: &str, hit: Range<usize>) -> String {
        let start = text.floor_char_boundary(hit.start.saturating_sub(WINDOW_SIDE_BYTES));
        let end =
            text.ceil_char_boundary(hit.end.saturating_add(WINDOW_SIDE_BYTES).min(text.len()));
        let shown = self.with_placeholders(text, start..end);
        let mut value = String::with_capacity(shown.len() + 6);
        if start > 0 {
            value.push('…');
        }
        value.push_str(&shown);
        if end < text.len() {
            value.push('…');
        }
        value
    }

    fn with_placeholders(&self, text: &str, shown: Range<usize>) -> String {
        let first = self
            .spans
            .partition_point(|(range, _)| range.end <= shown.start);
        let mut out = String::with_capacity(shown.len());
        let mut from = shown.start;
        for (range, idx) in self.spans[first..]
            .iter()
            .take_while(|(range, _)| range.start < shown.end)
        {
            let start = range.start.max(from);
            out.push_str(&text[from..start]);
            out.push_str(replacement_for(*idx));
            from = range.end.min(shown.end);
        }
        out.push_str(&text[from..shown.end]);
        out
    }
}
