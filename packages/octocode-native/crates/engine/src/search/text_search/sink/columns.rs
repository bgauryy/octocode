//! How a matched block's bytes map to the text and UTF-16 columns a match
//! reports: one decoding for every reader, and a forward cursor over spans.
use crate::num::saturating_u32;
use crate::text::{Decoded, interior_offset};

/// A matched or context block's text as every reader decodes it
/// ([`Decoded::interior`]: UTF-8, else Latin-1 or U+FFFD for invalid bytes),
/// and whether it was not UTF-8. Raw byte offsets map into it with
/// [`interior_offset`].
pub(super) fn block_text(bytes: &[u8]) -> (String, bool) {
    let decoded = Decoded::interior(bytes);
    (decoded.text.into_owned(), decoded.warning.is_some())
}

/// Maps raw submatch offsets in a matched block to decoded-text offsets and UTF-16
/// columns. `find_iter` yields non-overlapping matches left to right, so a
/// forward cursor keeps a line with k spans at O(line + k) instead of
/// rescanning the prefix per span (O(line · k)). The line's UTF-8 validity is
/// checked once; an invalid line keeps the exact per-span lossy mapping.
pub(super) struct SpanColumns<'a> {
    bytes: &'a [u8],
    text: &'a str,
    valid_utf8: bool,
    cursor_byte: usize,
    cursor_utf16: usize,
    line: u32,
    line_start: usize,
}

impl<'a> SpanColumns<'a> {
    pub(super) fn new(bytes: &'a [u8], text: &'a str) -> Self {
        Self {
            bytes,
            text,
            valid_utf8: std::str::from_utf8(bytes).is_ok(),
            cursor_byte: 0,
            cursor_utf16: 0,
            line: 0,
            line_start: 0,
        }
    }

    /// The decoded-text offset of raw byte offset `raw`.
    pub(super) fn lossy(&self, raw: usize) -> usize {
        if self.valid_utf8 {
            raw.min(self.bytes.len())
        } else {
            interior_offset(self.bytes, raw)
        }
    }

    /// Line delta, UTF-16 column and line-local byte offset of a span start.
    pub(super) fn position(&mut self, start: usize) -> (u32, u32, u32) {
        let target = self.text.floor_char_boundary(start);
        if target < self.cursor_byte {
            self.cursor_byte = 0;
            self.cursor_utf16 = 0;
            self.line = 0;
            self.line_start = 0;
        }
        for (offset, ch) in self.text[self.cursor_byte..target].char_indices() {
            if ch == '\n' {
                self.line = self.line.saturating_add(1);
                self.line_start = self.cursor_byte + offset + 1;
                self.cursor_utf16 = 0;
            } else {
                self.cursor_utf16 += ch.len_utf16();
            }
        }
        self.cursor_byte = target;
        (
            self.line,
            saturating_u32(self.cursor_utf16),
            saturating_u32(target - self.line_start),
        )
    }
}

/// Slice the matched span `[start, end)` (byte offsets) out of `line` as a
/// valid UTF-8 substring.
pub(super) fn span_value(line: &str, start: usize, end: usize) -> String {
    let start = line.floor_char_boundary(start);
    let end = line.ceil_char_boundary(end).max(start);
    line[start..end].to_owned()
}
