/// Zero-allocation UTF-8 offset helpers and content slicer.
///
/// Replaces `utils/file/byteOffset.ts` in octocode-tools-core, which used
/// `Buffer.from(content, 'utf8')` — a full copy of the content — for every
/// char↔byte conversion, and called it 4–6 times per `applyPagination` invocation.
///
/// All functions here walk the UTF-8 byte sequence in-place via `str::char_indices()`
/// with no heap allocation proportional to content length.
use crate::types::{SliceContentOptions, SliceContentResult};

// ── core offset helpers ───────────────────────────────────────────────────────

/// Number of UTF-8 bytes up to (not including) the `char_index`-th JavaScript
/// UTF-16 code unit in `s`. Clamps to `s.len()` if `char_index` exceeds the string.
pub(crate) fn char_to_byte_offset_inner(s: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }

    let mut utf16_units = 0usize;
    for (byte_idx, ch) in s.char_indices() {
        if utf16_units >= char_index || utf16_units + ch.len_utf16() > char_index {
            return byte_idx;
        }
        utf16_units += ch.len_utf16();
    }
    s.len() // char_index beyond string length - clamp
}

/// JavaScript UTF-16 code-unit offset corresponding to `byte_offset` bytes into `s`.
/// Clamps to the JS string length if `byte_offset` exceeds `s.len()`.
pub(crate) fn byte_to_char_offset_inner(s: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(s.len());
    // Safe: we snap to the nearest valid boundary
    let valid_offset = floor_char_boundary(s, clamped);
    utf16_len(&s[..valid_offset])
}

/// Extract a byte-range substring from `s`. Returns `""` for an out-of-range or
/// invalid range.
pub(crate) fn byte_slice_content_inner(s: &str, byte_start: usize, byte_end: usize) -> String {
    if byte_start >= byte_end || byte_start >= s.len() {
        return String::new();
    }
    let start = floor_char_boundary(s, byte_start.min(s.len()));
    let end = floor_char_boundary(s, byte_end.min(s.len()));
    if start > end {
        return String::new();
    }
    s[start..end].to_owned()
}

/// Snap `byte_pos` down to the nearest valid UTF-8 character boundary in `s`.
fn floor_char_boundary(s: &str, mut byte_pos: usize) -> usize {
    if byte_pos >= s.len() {
        return s.len();
    }
    // Walk back until we land on a UTF-8 leading byte
    while byte_pos > 0 && !s.is_char_boundary(byte_pos) {
        byte_pos -= 1;
    }
    byte_pos
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

// ── combined slicer ───────────────────────────────────────────────────────────

/// Paginate `content` starting at `char_offset` for up to `char_length` chars.
///
/// When `snap_to_line_boundary` is true the slice always starts at the
/// beginning of the containing line and ends at the end of the last complete
/// line within the window — equivalent to the TypeScript `sliceByCharRespectLines`
/// (dead code, 0 callers confirmed by LSP) merged with the char-mode path of
/// `applyPagination`.
pub(crate) fn slice_content_inner(
    content: &str,
    char_offset: usize,
    char_length: usize,
    options: Option<SliceContentOptions>,
) -> SliceContentResult {
    let snap = options
        .as_ref()
        .and_then(|o| o.snap_to_line_boundary)
        .unwrap_or(false);

    let total_chars = utf16_len(content);

    if total_chars == 0 {
        return SliceContentResult {
            text: String::new(),
            char_offset: 0,
            char_length: 0,
            byte_offset: 0,
            byte_length: 0,
            has_more: false,
            next_char_offset: None,
        };
    }

    let start_char = char_offset.min(total_chars);
    let raw_end_char = (start_char + char_length).min(total_chars);

    let (actual_start, actual_end) = if snap {
        snap_to_lines(content, start_char, raw_end_char)
    } else {
        (start_char, raw_end_char)
    };

    let start_byte = char_to_byte_offset_inner(content, actual_start);
    let end_byte = char_to_byte_offset_inner(content, actual_end);
    let text = content[start_byte..end_byte].to_owned();
    let actual_char_length = actual_end - actual_start;
    let has_more = actual_end < total_chars;

    SliceContentResult {
        text,
        char_offset: actual_start as u32,
        char_length: actual_char_length as u32,
        byte_offset: start_byte as u32,
        byte_length: (end_byte - start_byte) as u32,
        has_more,
        next_char_offset: if has_more {
            Some(actual_end as u32)
        } else {
            None
        },
    }
}

/// Snap `(start_char, end_char)` to line boundaries: push start back to line
/// start, extend end to line end (or next line start).
fn snap_to_lines(content: &str, start_char: usize, end_char: usize) -> (usize, usize) {
    // Single pass over the content (no allocated line table, no second utf16_len
    // walk): track the last line start at or before start_char, and the first
    // line start after end_char. Offsets are JavaScript UTF-16 code units.
    let mut char_idx = 0usize;
    let mut actual_start = 0usize;
    let mut actual_end: Option<usize> = None;
    for ch in content.chars() {
        char_idx += ch.len_utf16();
        if ch == '\n' {
            let line_start = char_idx; // start of the next line
            if line_start <= start_char {
                actual_start = line_start;
            }
            if actual_end.is_none() && line_start > end_char {
                actual_end = Some(line_start);
            }
        }
    }
    // char_idx is now the total UTF-16 length: the fallback when end_char sits in
    // the final line (no newline after it).
    (actual_start, actual_end.unwrap_or(char_idx))
}

// ── LineIndex ─────────────────────────────────────────────────────────────────

/// Maps byte offsets to/from 0-based `(line, UTF-16 code-unit column)`
/// positions, and exposes the per-line UTF-16 line-start table. Built once in
/// a single pass over `content`; every lookup after that is O(log n) via
/// binary search over `line_starts_byte`.
///
/// This is the single shared implementation behind what were five
/// independent reimplementations of "UTF-16 units per byte range":
/// `structural/octo.rs`, `signatures/js_oxc.rs`, `signatures/graph_facts.rs`,
/// and `signatures/mod.rs::build_js_char_offset_table`. Each of those keeps a
/// thin, domain-specific wrapper (different method names/return types to
/// match its own serde output or tree-sitter point convention) but delegates
/// the actual counting to this struct.
pub(crate) struct LineIndex<'a> {
    content: &'a str,
    /// Byte offset of the first byte of each 0-based line.
    line_starts_byte: Vec<u32>,
    /// UTF-16 code-unit offset of the first unit of each 0-based line — the
    /// JS-string-offset equivalent of `line_starts_byte`.
    line_starts_utf16: Vec<u32>,
}

impl<'a> LineIndex<'a> {
    pub(crate) fn new(content: &'a str) -> Self {
        let mut line_starts_byte = vec![0u32];
        let mut line_starts_utf16 = vec![0u32];
        let mut utf16_units: u32 = 0;
        for (byte_idx, ch) in content.char_indices() {
            utf16_units = utf16_units.saturating_add(ch.len_utf16() as u32);
            if ch == '\n' {
                line_starts_byte.push((byte_idx + ch.len_utf8()) as u32);
                line_starts_utf16.push(utf16_units);
            }
        }
        Self {
            content,
            line_starts_byte,
            line_starts_utf16,
        }
    }

    /// UTF-16 code-unit offset of the first unit of each 0-based line.
    /// `table[i]` is the offset of the first unit on line `i` (0-based).
    pub(crate) fn line_starts_utf16(&self) -> &[u32] {
        &self.line_starts_utf16
    }

    /// 0-based `(line, UTF-16 column)` for a byte offset into `content`.
    /// Clamps `byte_offset` beyond `content.len()` to the end of content.
    pub(crate) fn byte_to_position(&self, byte_offset: u32) -> (u32, u32) {
        let line = self
            .line_starts_byte
            .partition_point(|&start| start <= byte_offset)
            .saturating_sub(1);
        let line_start = self.line_starts_byte.get(line).copied().unwrap_or(0) as usize;
        // Snap an offset that lands inside a multi-byte character down to that
        // character's start; slicing on a non-char-boundary returns None and
        // would otherwise silently collapse the column to 0.
        let end = floor_char_boundary(self.content, (byte_offset as usize).min(self.content.len()));
        let character = if line_start <= end {
            self.content
                .get(line_start..end)
                .map(|slice| slice.chars().map(char::len_utf16).sum::<usize>() as u32)
                .unwrap_or(0)
        } else {
            0
        };
        (line as u32, character)
    }

    /// Inverse of [`byte_to_position`](Self::byte_to_position): a 0-based
    /// `(line, UTF-16 column)` to a byte offset into `content`. Clamps
    /// out-of-range input to a valid offset.
    pub(crate) fn position_to_byte(&self, line: u32, character: u32) -> u32 {
        let line_start = self
            .line_starts_byte
            .get(line as usize)
            .copied()
            .unwrap_or(self.content.len() as u32) as usize;
        let mut utf16 = 0u32;
        let mut byte = line_start;
        for ch in self.content.get(line_start..).unwrap_or("").chars() {
            if utf16 >= character || ch == '\n' {
                break;
            }
            utf16 += ch.len_utf16() as u32;
            byte += ch.len_utf8();
        }
        byte as u32
    }

    /// UTF-16 column for a tree-sitter-style `(row, byte_column)` point,
    /// where `row` is already known (no binary search needed) and
    /// `byte_column` is a byte offset within that row. Clamped to the row's
    /// bounds.
    pub(crate) fn row_col_to_utf16_column(&self, row: u32, byte_column: u32) -> u32 {
        let row = row as usize;
        let line_start = self.line_starts_byte.get(row).copied().unwrap_or(0) as usize;
        let line_end = self
            .line_starts_byte
            .get(row + 1)
            .map(|start| (*start as usize).saturating_sub(1))
            .unwrap_or(self.content.len())
            .min(self.content.len());
        let byte_end = line_start
            .saturating_add(byte_column as usize)
            .min(line_end);
        self.content
            .get(line_start..byte_end)
            .map(|slice| slice.chars().map(char::len_utf16).sum::<usize>() as u32)
            .unwrap_or(byte_column)
    }
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── char_to_byte_offset_inner ─────────────────────────────────────────────

    #[test]
    fn snap_to_lines_snaps_start_back_and_end_forward() {
        let content = "aa\nbbb\ncccc"; // line starts at UTF-16 offsets 0, 3, 7
        assert_eq!(snap_to_lines(content, 4, 4), (3, 7));
        // end_char in the final line → snap end to total length (11)
        assert_eq!(snap_to_lines(content, 8, 8), (7, 11));
        // start at offset 0 stays at 0
        assert_eq!(snap_to_lines(content, 0, 1), (0, 3));
    }

    #[test]
    fn snap_to_lines_handles_multibyte() {
        // "é\nb": é is 1 UTF-16 unit, '\n' at offset 1, line 2 starts at 2.
        let content = "é\nbb";
        assert_eq!(snap_to_lines(content, 2, 2), (2, 4));
    }

    #[test]
    fn char_to_byte_ascii_identity() {
        assert_eq!(char_to_byte_offset_inner("hello", 3), 3);
        assert_eq!(char_to_byte_offset_inner("hello", 0), 0);
        assert_eq!(char_to_byte_offset_inner("hello", 5), 5);
    }

    #[test]
    fn char_to_byte_multibyte() {
        // "café" → c(1) a(1) f(1) é(2) = 5 bytes for 4 chars
        let s = "café";
        assert_eq!(char_to_byte_offset_inner(s, 0), 0);
        assert_eq!(char_to_byte_offset_inner(s, 3), 3); // up to 'é'
        assert_eq!(char_to_byte_offset_inner(s, 4), 5); // after 'é'
    }

    #[test]
    fn char_to_byte_uses_javascript_utf16_indices() {
        let s = "a🌍b";
        assert_eq!(char_to_byte_offset_inner(s, 0), 0);
        assert_eq!(char_to_byte_offset_inner(s, 1), 1);
        assert_eq!(char_to_byte_offset_inner(s, 2), 1); // inside surrogate pair snaps down
        assert_eq!(char_to_byte_offset_inner(s, 3), 5); // after emoji
        assert_eq!(char_to_byte_offset_inner(s, 4), 6);
    }

    #[test]
    fn char_to_byte_clamps_beyond_length() {
        assert_eq!(char_to_byte_offset_inner("hi", 100), 2);
    }

    // ── byte_to_char_offset_inner ─────────────────────────────────────────────

    #[test]
    fn byte_to_char_ascii_identity() {
        assert_eq!(byte_to_char_offset_inner("hello", 3), 3);
        assert_eq!(byte_to_char_offset_inner("hello", 0), 0);
    }

    #[test]
    fn byte_to_char_multibyte() {
        let s = "café"; // c=0, a=1, f=2, é=3..4
        assert_eq!(byte_to_char_offset_inner(s, 0), 0);
        assert_eq!(byte_to_char_offset_inner(s, 3), 3); // at start of 'é'
        assert_eq!(byte_to_char_offset_inner(s, 5), 4); // after 'é'
    }

    #[test]
    fn byte_to_char_uses_javascript_utf16_indices() {
        let s = "a🌍b";
        assert_eq!(byte_to_char_offset_inner(s, 0), 0);
        assert_eq!(byte_to_char_offset_inner(s, 1), 1);
        assert_eq!(byte_to_char_offset_inner(s, 5), 3);
        assert_eq!(byte_to_char_offset_inner(s, 6), 4);
    }

    #[test]
    fn byte_to_char_clamps_beyond_length() {
        assert_eq!(byte_to_char_offset_inner("hi", 100), 2);
    }

    // ── byte_slice_content_inner ──────────────────────────────────────────────

    #[test]
    fn byte_slice_ascii() {
        assert_eq!(byte_slice_content_inner("hello world", 6, 11), "world");
    }

    #[test]
    fn byte_slice_multibyte() {
        let s = "café"; // bytes: 63 61 66 C3 A9
        assert_eq!(byte_slice_content_inner(s, 3, 5), "é");
    }

    #[test]
    fn byte_slice_empty_on_bad_range() {
        assert_eq!(byte_slice_content_inner("hello", 3, 2), "");
        assert_eq!(byte_slice_content_inner("hello", 10, 20), "");
    }

    // ── slice_content_inner ───────────────────────────────────────────────────

    #[test]
    fn slice_content_basic_window() {
        let content = "abcdefghij";
        let r = slice_content_inner(content, 3, 4, None);
        assert_eq!(r.text, "defg");
        assert_eq!(r.char_offset, 3);
        assert_eq!(r.char_length, 4);
        assert!(r.has_more);
    }

    #[test]
    fn slice_content_last_page_no_more() {
        let content = "abcde";
        let r = slice_content_inner(content, 3, 10, None);
        assert_eq!(r.text, "de");
        assert!(!r.has_more);
        assert!(r.next_char_offset.is_none());
    }

    #[test]
    fn slice_content_snap_to_line_start() {
        let content = "line1\nline2\nline3\n";
        // char 3 is inside "line1", should snap back to 0
        let r = slice_content_inner(
            content,
            3,
            8,
            Some(SliceContentOptions {
                snap_to_line_boundary: Some(true),
            }),
        );
        assert!(r.text.starts_with("line1"));
    }

    #[test]
    fn slice_content_snap_to_line_end() {
        let content = "line1\nline2\nline3\n";
        // start at 0 with 4 chars → raw end mid-"line1", snap extends to end of line1
        let r = slice_content_inner(
            content,
            0,
            4,
            Some(SliceContentOptions {
                snap_to_line_boundary: Some(true),
            }),
        );
        // "line1\n" = 6 chars, so snapped end should be at line2 start (char 6)
        assert_eq!(r.char_offset, 0);
        assert!(r.char_length >= 5); // at least "line1"
    }

    #[test]
    fn slice_content_empty_input() {
        let r = slice_content_inner("", 0, 100, None);
        assert_eq!(r.text, "");
        assert!(!r.has_more);
    }

    #[test]
    fn slice_content_offset_past_eof_reports_clamped_offset() {
        // char_offset far beyond total_chars must clamp char_offset to
        // total_chars (not reset it to 0) and report has_more: false —
        // otherwise callers get a bogus next_char_offset that loops back to
        // the start of the file.
        let content = "abcde";
        let r = slice_content_inner(content, 1000, 10, None);
        assert_eq!(r.text, "");
        assert_eq!(r.char_offset, 5);
        assert_eq!(r.char_length, 0);
        assert!(!r.has_more);
        assert!(r.next_char_offset.is_none());
    }

    #[test]
    fn slice_content_zero_length_preserves_offset() {
        // An explicit char_length:0 request mid-content must not be treated
        // as "no offset given" — it should report has_more relative to the
        // requested offset, not the start of the file.
        let content = "abcdefghij";
        let r = slice_content_inner(content, 3, 0, None);
        assert_eq!(r.text, "");
        assert_eq!(r.char_offset, 3);
        assert_eq!(r.char_length, 0);
        assert!(r.has_more);
        assert_eq!(r.next_char_offset, Some(3));
    }

    #[test]
    fn slice_content_multibyte_chars() {
        let content = "café world";
        let r = slice_content_inner(content, 0, 4, None);
        assert_eq!(r.text, "café");
        assert_eq!(r.char_length, 4);
        assert_eq!(r.byte_length, 5); // é = 2 bytes
    }

    #[test]
    fn slice_content_uses_javascript_utf16_indices() {
        let content = "a🌍b";
        let r = slice_content_inner(content, 0, 3, None);
        assert_eq!(r.text, "a🌍");
        assert_eq!(r.char_length, 3);
        assert_eq!(r.byte_length, 5);
        assert!(r.has_more);
        assert_eq!(r.next_char_offset, Some(3));
    }

    #[test]
    fn byte_offset_roundtrip() {
        let s = "hello 世界 world";
        let js_boundaries = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
        for char_idx in js_boundaries {
            let byte_off = char_to_byte_offset_inner(s, char_idx);
            let char_back = byte_to_char_offset_inner(s, byte_off);
            assert_eq!(
                char_back, char_idx,
                "roundtrip failed at char_idx={char_idx}"
            );
        }
    }

    // ── LineIndex — shared line/UTF-16 index (consolidates the formerly
    // duplicated implementations in structural/octo.rs, signatures/js_oxc.rs,
    // and signatures/mod.rs::build_js_char_offset_table) ─────────────────────

    #[test]
    fn line_index_utf16_line_starts_counts_utf16_units() {
        // ASCII-only: each char = 1 JS unit. Mirrors the former
        // `build_js_char_offset_table` test in signatures/mod.rs.
        let src = "ab\ncd\n";
        let index = LineIndex::new(src);
        assert_eq!(index.line_starts_utf16(), &[0, 3, 6]);
    }

    #[test]
    fn line_index_utf16_line_starts_counts_surrogate_pairs() {
        // "a🌍\nbb": 🌍 is 2 UTF-16 units, so line 2 starts at unit 4 (a=1,🌍=2,\n=1).
        let src = "a🌍\nbb";
        let index = LineIndex::new(src);
        assert_eq!(index.line_starts_utf16(), &[0, 4]);
    }

    #[test]
    fn line_index_byte_to_position_ascii() {
        let src = "line1\nline2\nline3";
        let index = LineIndex::new(src);
        assert_eq!(index.byte_to_position(0), (0, 0));
        assert_eq!(index.byte_to_position(3), (0, 3)); // inside "line1"
        assert_eq!(index.byte_to_position(6), (1, 0)); // start of "line2"
        assert_eq!(index.byte_to_position(12), (2, 0)); // start of "line3"
    }

    #[test]
    fn line_index_byte_to_position_multibyte() {
        // "a🌍b\ncd": line 0 is "a🌍b" (byte len 6), line 1 is "cd".
        let src = "a🌍b\ncd";
        let index = LineIndex::new(src);
        assert_eq!(index.byte_to_position(0), (0, 0)); // 'a'
        assert_eq!(index.byte_to_position(1), (0, 1)); // start of 🌍
        assert_eq!(index.byte_to_position(5), (0, 3)); // 'b', after 2-unit emoji
        assert_eq!(index.byte_to_position(7), (1, 0)); // start of "cd"
    }

    #[test]
    fn line_index_byte_to_position_clamps_beyond_length() {
        let src = "hi";
        let index = LineIndex::new(src);
        assert_eq!(index.byte_to_position(100), (0, 2));
    }

    #[test]
    fn line_index_byte_to_position_snaps_mid_multibyte_offset_down() {
        // A byte offset landing inside 🌍 (bytes 1..5) must report the column of
        // the character it falls in — 🌍 starts at UTF-16 column 1 — rather than
        // collapse to column 0 as a non-char-boundary slice silently would.
        let src = "a🌍b\ncd";
        let index = LineIndex::new(src);
        assert_eq!(index.byte_to_position(2), (0, 1));
        assert_eq!(index.byte_to_position(3), (0, 1));
        assert_eq!(index.byte_to_position(4), (0, 1));
        // Character boundaries are unaffected by the snap.
        assert_eq!(index.byte_to_position(1), (0, 1));
        assert_eq!(index.byte_to_position(5), (0, 3));
    }

    #[test]
    fn line_index_position_to_byte_is_inverse_of_byte_to_position() {
        let src = "hello\nworld\n世界 line";
        let index = LineIndex::new(src);
        for byte in [0usize, 1, 5, 6, 9, 12, 15, 20] {
            let (line, character) = index.byte_to_position(byte as u32);
            let back = index.position_to_byte(line, character);
            // byte_to_position clamps to the nearest char boundary at/after
            // `byte`'s line-relative UTF-16 unit, so round-tripping must land
            // on a byte offset that maps back to the same (line, character).
            assert_eq!(
                index.byte_to_position(back),
                (line, character),
                "roundtrip failed at byte={byte}"
            );
        }
    }

    #[test]
    fn line_index_row_col_to_utf16_column_matches_byte_to_position() {
        // row_col_to_utf16_column (used by structural/octo.rs, which already
        // has a tree-sitter (row, byte_column) point in hand) must agree with
        // byte_to_position's within-line UTF-16 column for the same location.
        let src = "abc\nd🌍fg\nhij";
        let index = LineIndex::new(src);
        assert_eq!(index.row_col_to_utf16_column(0, 2), 2); // "ab" -> col 2
        assert_eq!(index.row_col_to_utf16_column(1, 5), 3); // "d🌍" -> col 3 (1 + 2 units)
        assert_eq!(index.row_col_to_utf16_column(2, 3), 3); // "hij" -> col 3
    }
}
