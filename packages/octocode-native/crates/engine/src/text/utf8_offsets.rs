//! Zero-allocation UTF-8 offset helpers.
//!
//! All functions here walk the UTF-8 byte sequence in-place via `str::char_indices()`
//! with no heap allocation proportional to content length.

// ── core offset helpers ───────────────────────────────────────────────────────

/// UTF-16 code-unit offset of the first `byte_offset` bytes of `s` (snapped
/// down to a char boundary). Clamps to the UTF-16 length of `s` if
/// `byte_offset` exceeds `s.len()`.
pub(crate) fn byte_to_utf16_offset(s: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(s.len());
    // Safe: we snap to the nearest valid boundary
    let valid_offset = floor_char_boundary(s, clamped);
    utf16_len(&s[..valid_offset])
}

/// Snap `byte_pos` down to the nearest valid UTF-8 character boundary in `s`
/// (clamped to `s.len()`).
pub(crate) fn floor_char_boundary(s: &str, mut byte_pos: usize) -> usize {
    if byte_pos >= s.len() {
        return s.len();
    }
    // Walk back until we land on a UTF-8 leading byte
    while byte_pos > 0 && !s.is_char_boundary(byte_pos) {
        byte_pos -= 1;
    }
    byte_pos
}

/// Smallest char boundary `>= i` (clamped to `s.len()`).
pub(crate) fn ceil_char_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// A leading byte-order mark. Editors and LSP clients hide it, so user-facing
/// row-0 columns and line text exclude it while byte offsets keep counting it.
pub(crate) const BOM: char = '\u{feff}';

/// Hide a leading BOM from a row-0 line: returns the line text without it and
/// `byte_column` rebased past it. Other rows (and BOM-free text) pass through.
pub(crate) fn hide_bom_in_line(row: usize, line: &str, byte_column: usize) -> (&str, usize) {
    match line.strip_prefix(BOM) {
        Some(rest) if row == 0 => (rest, byte_column.saturating_sub(BOM.len_utf8())),
        _ => (line, byte_column),
    }
}

// ── LineIndex ─────────────────────────────────────────────────────────────────

/// Maps byte offsets to/from 0-based lines, byte columns, and UTF-16
/// code-unit columns. Built once in a single pass; every lookup after that is
/// O(log n) via binary search over the line-start table. The index holds no
/// reference to the text: methods that read characters take the same
/// `content` the index was built from.
///
/// The one line index of the engine. Two line-break rules exist, and the
/// constructor names the one in play:
/// - [`LineIndex::new`]: the LSP rule. `\r\n`, `\n`, and a lone `\r` each end
///   a line, as language servers count lines. A text ending in a line break
///   has a final empty line.
/// - [`LineIndex::tree_sitter`]: `\n` only, as tree-sitter rows and ripgrep
///   line numbers count lines. `\r\n` keeps its `\r` on the line; a lone `\r`
///   and U+2028/U+2029 do not break.
///
/// The rules give different line numbers only for text with a lone `\r`.
#[derive(Debug)]
pub struct LineIndex {
    /// Byte offset of the first byte of each 0-based line.
    line_starts_byte: Vec<u32>,
    /// Byte offset where each line's text ends, its terminator excluded.
    line_ends_byte: Vec<u32>,
    /// UTF-16 code-unit offset of the first unit of each 0-based line.
    line_starts_utf16: Vec<u32>,
    /// UTF-16 width of a leading U+FEFF (0 or 1). Editors and LSP clients strip
    /// the BOM, so row-0 columns exclude it; byte offsets and the raw
    /// `line_starts_utf16` table keep counting it.
    bom_utf16: u32,
    /// `(byte offset, cumulative UTF-16 units from content start)` sampled
    /// every [`UTF16_CHECKPOINT_BYTES`] (snapped to a char boundary). Column
    /// math scans at most one checkpoint gap instead of the whole line, which
    /// keeps minified single-line files linear instead of quadratic.
    utf16_checkpoints: Vec<(u32, u32)>,
    /// All-ASCII content: UTF-16 columns equal byte columns.
    ascii: bool,
    /// The text ends with a line break, so the last line is empty.
    ends_with_break: bool,
}

/// Byte spacing of [`LineIndex`] UTF-16 checkpoints.
const UTF16_CHECKPOINT_BYTES: usize = 1024;

impl LineIndex {
    /// The LSP rule: `\r\n`, `\n`, and a lone `\r` each end a line.
    pub fn new(content: &str) -> Self {
        Self::build(content, true)
    }

    /// `\n` only, aligned with tree-sitter rows: every position derived from
    /// a syntax tree (or a ripgrep line) must use this rule, or a lone `\r`
    /// shifts every later line against the grammar's own row numbering.
    pub(crate) fn tree_sitter(content: &str) -> Self {
        Self::build(content, false)
    }

    fn build(content: &str, lone_cr_breaks: bool) -> Self {
        let bytes = content.as_bytes();
        let mut line_starts_byte = vec![0u32];
        let mut line_ends_byte = Vec::new();
        let mut line_starts_utf16 = vec![0u32];
        let mut utf16_checkpoints = vec![(0u32, 0u32)];
        let mut utf16_units: u32 = 0;
        for (byte_idx, ch) in content.char_indices() {
            if byte_idx >= utf16_checkpoints.len() * UTF16_CHECKPOINT_BYTES {
                utf16_checkpoints.push((byte_idx as u32, utf16_units));
            }
            utf16_units = utf16_units.saturating_add(ch.len_utf16() as u32);
            let next = byte_idx + 1;
            match ch {
                // `\r\n` under the LSP rule already broke at the `\r`.
                '\n' if lone_cr_breaks && byte_idx > 0 && bytes[byte_idx - 1] == b'\r' => {}
                '\n' => {
                    line_ends_byte.push(byte_idx as u32);
                    line_starts_byte.push(next as u32);
                    line_starts_utf16.push(utf16_units);
                }
                '\r' if lone_cr_breaks => {
                    let crlf = bytes.get(next) == Some(&b'\n');
                    line_ends_byte.push(byte_idx as u32);
                    line_starts_byte.push((next + usize::from(crlf)) as u32);
                    line_starts_utf16.push(utf16_units + u32::from(crlf));
                }
                _ => {}
            }
        }
        line_ends_byte.push(content.len() as u32);
        Self {
            ends_with_break: !content.is_empty()
                && line_starts_byte.last().copied() == Some(content.len() as u32),
            line_starts_byte,
            line_ends_byte,
            line_starts_utf16,
            bom_utf16: u32::from(content.starts_with(BOM)),
            utf16_checkpoints,
            ascii: content.is_ascii(),
        }
    }

    /// Number of lines, including the empty last line after a final break.
    pub(crate) fn len(&self) -> usize {
        self.line_starts_byte.len()
    }

    /// Number of lines that hold content: the final empty line after a
    /// trailing break is not counted (matches `str::lines`).
    pub fn content_len(&self) -> usize {
        self.len() - usize::from(self.ends_with_break)
    }

    /// Byte offset where `line` starts, or `None` past the last line.
    pub fn line_start(&self, line: usize) -> Option<usize> {
        self.line_starts_byte.get(line).map(|&start| start as usize)
    }

    /// Text of `line`, without its terminator.
    pub(crate) fn line<'c>(&self, content: &'c str, line: usize) -> Option<&'c str> {
        let start = *self.line_starts_byte.get(line)? as usize;
        let end = *self.line_ends_byte.get(line)? as usize;
        content.get(start..end)
    }

    /// Every line's text, without terminators.
    pub(crate) fn lines<'c>(&self, content: &'c str) -> Vec<&'c str> {
        (0..self.len())
            .map(|line| self.line(content, line).unwrap_or_default())
            .collect()
    }

    /// `(line, byte column)` of a byte offset. An offset inside a line
    /// terminator maps to the end of its line.
    pub(crate) fn position_of(&self, byte: usize) -> (usize, usize) {
        let line = self.line_of(byte);
        let start = self.line_starts_byte[line] as usize;
        let end = self.line_ends_byte[line] as usize;
        (line, byte.min(end).saturating_sub(start))
    }

    /// The 0-based line holding byte offset `byte`.
    fn line_of(&self, byte: usize) -> usize {
        self.line_starts_byte
            .partition_point(|&start| start as usize <= byte)
            .saturating_sub(1)
    }

    /// UTF-16 units from content start up to the char boundary `byte`
    /// (callers pass a boundary). Scans at most one checkpoint gap.
    fn utf16_before(&self, content: &str, byte: usize) -> u32 {
        if self.ascii {
            return byte as u32;
        }
        let slot = (byte / UTF16_CHECKPOINT_BYTES).min(self.utf16_checkpoints.len() - 1);
        // Checkpoints snap forward to a char boundary, so the slot's byte can
        // exceed `byte`; step back until it does not.
        let slot = (0..=slot)
            .rev()
            .find(|&i| self.utf16_checkpoints[i].0 as usize <= byte)
            .unwrap_or(0);
        let (base_byte, base_units) = self.utf16_checkpoints[slot];
        base_units
            + content
                .get(base_byte as usize..byte)
                .map(|slice| slice.chars().map(char::len_utf16).sum::<usize>() as u32)
                .unwrap_or(0)
    }

    /// UTF-16 column of char boundary `byte` on 0-based `line`.
    fn utf16_column(&self, content: &str, line: usize, byte: usize) -> u32 {
        let line_units = self.line_starts_utf16.get(line).copied().unwrap_or(0);
        self.utf16_before(content, byte).saturating_sub(line_units)
    }

    /// UTF-16 code-unit offset of the first unit of each 0-based line.
    /// `table[i]` is the offset of the first unit on line `i` (0-based).
    pub(crate) fn line_starts_utf16(&self) -> &[u32] {
        &self.line_starts_utf16
    }

    /// 0-based `(line, UTF-16 column)` for a byte offset into `content`.
    /// Clamps `byte_offset` beyond `content.len()` to the end of content.
    pub(crate) fn byte_to_position(&self, content: &str, byte_offset: u32) -> (u32, u32) {
        let line = self.line_of(byte_offset as usize);
        let line_start = self.line_starts_byte.get(line).copied().unwrap_or(0) as usize;
        // Snap an offset that lands inside a multi-byte character down to that
        // character's start; slicing on a non-char-boundary returns None and
        // would otherwise silently collapse the column to 0.
        let end = floor_char_boundary(content, (byte_offset as usize).min(content.len()));
        let character = if line_start <= end {
            self.utf16_column(content, line, end)
        } else {
            0
        };
        (line as u32, self.hide_bom(line, character))
    }

    /// Inverse of [`byte_to_position`](Self::byte_to_position): a 0-based
    /// `(line, UTF-16 column)` to a byte offset into `content`. Clamps
    /// out-of-range input to a valid offset.
    pub(crate) fn position_to_byte(&self, content: &str, line: u32, character: u32) -> u32 {
        let line_start = self
            .line_starts_byte
            .get(line as usize)
            .copied()
            .unwrap_or(content.len() as u32) as usize;
        let line_end = self
            .line_ends_byte
            .get(line as usize)
            .map_or(content.len(), |&end| end as usize);
        let character = if line == 0 {
            character.saturating_add(self.bom_utf16)
        } else {
            character
        };
        if self.ascii {
            return line_start.saturating_add(character as usize).min(line_end) as u32;
        }
        // Jump to the last checkpoint at or before the target column on this
        // line, then scan forward (at most one checkpoint gap).
        let line_units = self
            .line_starts_utf16
            .get(line as usize)
            .copied()
            .unwrap_or(0);
        let target = line_units.saturating_add(character);
        let next_line_start = self
            .line_starts_byte
            .get(line as usize + 1)
            .map_or(usize::MAX, |start| *start as usize);
        let slot = self
            .utf16_checkpoints
            .partition_point(|&(_, units)| units <= target)
            .saturating_sub(1);
        let (mut byte, mut utf16) = match self.utf16_checkpoints.get(slot) {
            // Only a checkpoint inside this line is a valid starting point.
            Some(&(cp_byte, cp_units))
                if (cp_byte as usize) > line_start && (cp_byte as usize) < next_line_start =>
            {
                (cp_byte as usize, cp_units - line_units)
            }
            _ => (line_start, 0),
        };
        for ch in content.get(byte..).unwrap_or("").chars() {
            if utf16 >= character || byte >= line_end {
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
    pub(crate) fn row_col_to_utf16_column(&self, content: &str, row: u32, byte_column: u32) -> u32 {
        let row = row as usize;
        let line_start = self.line_starts_byte.get(row).copied().unwrap_or(0) as usize;
        let line_end = self
            .line_ends_byte
            .get(row)
            .map_or(content.len(), |&end| end as usize)
            .min(content.len());
        let byte_end = line_start
            .saturating_add(byte_column as usize)
            .min(line_end);
        let column = if content.is_char_boundary(byte_end) {
            self.utf16_column(content, row, byte_end)
        } else {
            content
                .get(line_start..byte_end)
                .map(|slice| slice.chars().map(char::len_utf16).sum::<usize>() as u32)
                .unwrap_or(byte_column)
        };
        self.hide_bom(row, column)
    }

    /// Drops the leading BOM's UTF-16 unit from a row-0 column.
    fn hide_bom(&self, row: usize, column: u32) -> u32 {
        if row == 0 {
            column.saturating_sub(self.bom_utf16)
        } else {
            column
        }
    }
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Number of UTF-8 bytes up to (not including) the `char_index`-th
    /// UTF-16 code unit in `s`. Clamps to `s.len()` if `char_index` exceeds the string.
    pub(super) fn utf16_to_byte_offset(s: &str, char_index: usize) -> usize {
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

    // ── utf16_to_byte_offset ─────────────────────────────────────────────

    #[test]
    fn char_to_byte_ascii_identity() {
        assert_eq!(utf16_to_byte_offset("hello", 3), 3);
        assert_eq!(utf16_to_byte_offset("hello", 0), 0);
        assert_eq!(utf16_to_byte_offset("hello", 5), 5);
    }

    #[test]
    fn char_to_byte_multibyte() {
        // "café" → c(1) a(1) f(1) é(2) = 5 bytes for 4 chars
        let s = "café";
        assert_eq!(utf16_to_byte_offset(s, 0), 0);
        assert_eq!(utf16_to_byte_offset(s, 3), 3); // up to 'é'
        assert_eq!(utf16_to_byte_offset(s, 4), 5); // after 'é'
    }

    #[test]
    fn char_to_byte_uses_javascript_utf16_indices() {
        let s = "a🌍b";
        assert_eq!(utf16_to_byte_offset(s, 0), 0);
        assert_eq!(utf16_to_byte_offset(s, 1), 1);
        assert_eq!(utf16_to_byte_offset(s, 2), 1); // inside surrogate pair snaps down
        assert_eq!(utf16_to_byte_offset(s, 3), 5); // after emoji
        assert_eq!(utf16_to_byte_offset(s, 4), 6);
    }

    #[test]
    fn char_to_byte_clamps_beyond_length() {
        assert_eq!(utf16_to_byte_offset("hi", 100), 2);
    }

    // ── byte_to_utf16_offset ─────────────────────────────────────────────

    #[test]
    fn byte_to_char_ascii_identity() {
        assert_eq!(byte_to_utf16_offset("hello", 3), 3);
        assert_eq!(byte_to_utf16_offset("hello", 0), 0);
    }

    #[test]
    fn byte_to_char_multibyte() {
        let s = "café"; // c=0, a=1, f=2, é=3..4
        assert_eq!(byte_to_utf16_offset(s, 0), 0);
        assert_eq!(byte_to_utf16_offset(s, 3), 3); // at start of 'é'
        assert_eq!(byte_to_utf16_offset(s, 5), 4); // after 'é'
    }

    #[test]
    fn byte_to_char_uses_javascript_utf16_indices() {
        let s = "a🌍b";
        assert_eq!(byte_to_utf16_offset(s, 0), 0);
        assert_eq!(byte_to_utf16_offset(s, 1), 1);
        assert_eq!(byte_to_utf16_offset(s, 5), 3);
        assert_eq!(byte_to_utf16_offset(s, 6), 4);
    }

    #[test]
    fn byte_to_char_clamps_beyond_length() {
        assert_eq!(byte_to_utf16_offset("hi", 100), 2);
    }

    #[test]
    fn byte_offset_roundtrip() {
        let s = "hello 世界 world";
        let js_boundaries = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
        for char_idx in js_boundaries {
            let byte_off = utf16_to_byte_offset(s, char_idx);
            let char_back = byte_to_utf16_offset(s, byte_off);
            assert_eq!(
                char_back, char_idx,
                "roundtrip failed at char_idx={char_idx}"
            );
        }
    }

    // ── LineIndex ─────────────────────────────────────────────────────────────

    #[test]
    fn line_index_utf16_line_starts_counts_utf16_units() {
        // ASCII-only: each char = 1 JS unit.
        let src = "ab\ncd\n";
        let index = LineIndex::tree_sitter(src);
        assert_eq!(index.line_starts_utf16(), &[0, 3, 6]);
    }

    #[test]
    fn line_index_utf16_line_starts_counts_surrogate_pairs() {
        // "a🌍\nbb": 🌍 is 2 UTF-16 units, so line 2 starts at unit 4 (a=1,🌍=2,\n=1).
        let src = "a🌍\nbb";
        let index = LineIndex::tree_sitter(src);
        assert_eq!(index.line_starts_utf16(), &[0, 4]);
    }

    #[test]
    fn line_index_hides_leading_bom_from_row_zero_columns() {
        // Editors and LSP clients strip a leading U+FEFF, so user-facing UTF-16
        // columns on row 0 must not count it. Byte offsets stay raw (tree-sitter
        // and oxc spans include the 3 BOM bytes).
        let src = "\u{feff}foo(z);\nbar();";
        let index = LineIndex::tree_sitter(src);
        assert_eq!(index.byte_to_position(src, 3), (0, 0)); // first char after BOM
        assert_eq!(index.byte_to_position(src, 0), (0, 0)); // inside the BOM clamps to 0
        assert_eq!(index.byte_to_position(src, 7), (0, 4)); // `z`
        assert_eq!(index.row_col_to_utf16_column(src, 0, 7), 4);
        assert_eq!(index.row_col_to_utf16_column(src, 0, 3), 0);
        // Inverse stays consistent on row 0 and later rows are unaffected.
        assert_eq!(index.position_to_byte(src, 0, 0), 3);
        assert_eq!(index.position_to_byte(src, 0, 4), 7);
        let row1 = src.find("bar").unwrap() as u32;
        assert_eq!(index.byte_to_position(src, row1), (1, 0));
        assert_eq!(index.position_to_byte(src, 1, 0), row1);
        // The raw JS-string line-start table is unchanged (it counts U+FEFF).
        assert_eq!(index.line_starts_utf16(), &[0, 9]);
    }

    #[test]
    fn hide_bom_in_line_rebases_row_zero_only() {
        assert_eq!(hide_bom_in_line(0, "\u{feff}foo", 3), ("foo", 0));
        assert_eq!(hide_bom_in_line(0, "\u{feff}foo", 5), ("foo", 2));
        assert_eq!(hide_bom_in_line(0, "\u{feff}foo", 0), ("foo", 0));
        assert_eq!(hide_bom_in_line(1, "\u{feff}foo", 3), ("\u{feff}foo", 3));
        assert_eq!(hide_bom_in_line(0, "foo", 1), ("foo", 1));
    }

    #[test]
    fn line_index_only_newline_starts_a_line() {
        // Documented, deliberate behavior (see `LineIndex::tree_sitter`): line
        // breaks are `\n` only. This matches tree-sitter's row convention and
        // the byte offsets LSP position math is aligned to. `\r\n` keeps `\r` on
        // the preceding line; lone `\r` (classic Mac) and the Unicode line/
        // paragraph separators U+2028/U+2029 do NOT start a new line here.
        // Changing this would silently shift every downstream LSP position.
        let crlf_src = "a\r\nb";
        let crlf = LineIndex::tree_sitter(crlf_src);
        assert_eq!(
            crlf.line_starts_utf16(),
            &[0, 3],
            "\\r\\n: one break after \\n"
        );
        assert_eq!(crlf.byte_to_position(crlf_src, 0), (0, 0));
        assert_eq!(
            crlf.byte_to_position(crlf_src, 3),
            (1, 0),
            "b is the start of line 1"
        );

        let lone_cr = LineIndex::tree_sitter("a\rb");
        assert_eq!(
            lone_cr.line_starts_utf16(),
            &[0],
            "lone \\r is not a line break"
        );

        let separators = LineIndex::tree_sitter("a\u{2028}b\u{2029}c");
        assert_eq!(
            separators.line_starts_utf16(),
            &[0],
            "U+2028/U+2029 are not line breaks"
        );
    }

    #[test]
    fn one_index_names_both_line_break_rules() {
        // The LSP rule breaks on a lone `\r`; the tree-sitter rule does not.
        // Both are the same type, so a caller always states which it counts.
        let text = "a\rb\r\nc\nd";
        let lsp = LineIndex::new(text);
        assert_eq!(lsp.lines(text), ["a", "b", "c", "d"]);
        assert_eq!(lsp.position_of(text.find('d').unwrap()), (3, 0));
        assert_eq!(
            lsp.byte_to_position(text, text.find('c').unwrap() as u32),
            (2, 0)
        );
        assert_eq!(lsp.position_to_byte(text, 1, 5), 3, "clamped before `\r\n`");
        let rows = LineIndex::tree_sitter(text);
        assert_eq!(rows.lines(text), ["a\rb\r", "c", "d"]);
        assert_eq!(rows.position_of(text.find('d').unwrap()), (2, 0));
        assert_eq!(
            rows.byte_to_position(text, text.find('b').unwrap() as u32),
            (0, 2)
        );
    }

    #[test]
    fn lsp_rule_counts_a_final_empty_line() {
        let text = "x\r\ny\r\n";
        let index = LineIndex::new(text);
        assert_eq!(index.lines(text), ["x", "y", ""]);
        assert_eq!(index.content_len(), 2);
        assert_eq!(index.line_start(1), Some(3));
        assert_eq!(LineIndex::new("").lines(""), [""]);
        assert_eq!(LineIndex::new("").content_len(), 1);
    }

    #[test]
    fn line_index_byte_to_position_ascii() {
        let src = "line1\nline2\nline3";
        let index = LineIndex::tree_sitter(src);
        assert_eq!(index.byte_to_position(src, 0), (0, 0));
        assert_eq!(index.byte_to_position(src, 3), (0, 3)); // inside "line1"
        assert_eq!(index.byte_to_position(src, 6), (1, 0)); // start of "line2"
        assert_eq!(index.byte_to_position(src, 12), (2, 0)); // start of "line3"
    }

    #[test]
    fn line_index_byte_to_position_multibyte() {
        // "a🌍b\ncd": line 0 is "a🌍b" (byte len 6), line 1 is "cd".
        let src = "a🌍b\ncd";
        let index = LineIndex::tree_sitter(src);
        assert_eq!(index.byte_to_position(src, 0), (0, 0)); // 'a'
        assert_eq!(index.byte_to_position(src, 1), (0, 1)); // start of 🌍
        assert_eq!(index.byte_to_position(src, 5), (0, 3)); // 'b', after 2-unit emoji
        assert_eq!(index.byte_to_position(src, 7), (1, 0)); // start of "cd"
    }

    #[test]
    fn line_index_byte_to_position_clamps_beyond_length() {
        let src = "hi";
        let index = LineIndex::tree_sitter(src);
        assert_eq!(index.byte_to_position(src, 100), (0, 2));
    }

    #[test]
    fn line_index_byte_to_position_snaps_mid_multibyte_offset_down() {
        // A byte offset landing inside 🌍 (bytes 1..5) must report the column of
        // the character it falls in — 🌍 starts at UTF-16 column 1 — rather than
        // collapse to column 0 as a non-char-boundary slice silently would.
        let src = "a🌍b\ncd";
        let index = LineIndex::tree_sitter(src);
        assert_eq!(index.byte_to_position(src, 2), (0, 1));
        assert_eq!(index.byte_to_position(src, 3), (0, 1));
        assert_eq!(index.byte_to_position(src, 4), (0, 1));
        // Character boundaries are unaffected by the snap.
        assert_eq!(index.byte_to_position(src, 1), (0, 1));
        assert_eq!(index.byte_to_position(src, 5), (0, 3));
    }

    #[test]
    fn line_index_position_to_byte_is_inverse_of_byte_to_position() {
        let src = "hello\nworld\n世界 line";
        let index = LineIndex::tree_sitter(src);
        for byte in [0usize, 1, 5, 6, 9, 12, 15, 20] {
            let (line, character) = index.byte_to_position(src, byte as u32);
            let back = index.position_to_byte(src, line, character);
            // byte_to_position clamps to the nearest char boundary at/after
            // `byte`'s line-relative UTF-16 unit, so round-tripping must land
            // on a byte offset that maps back to the same (line, character).
            assert_eq!(
                index.byte_to_position(src, back),
                (line, character),
                "roundtrip failed at byte={byte}"
            );
        }
    }

    #[test]
    fn line_index_row_col_to_utf16_column_matches_byte_to_position() {
        // A tree-sitter (row, byte_column) point must agree with
        // byte_to_position's within-line UTF-16 column for the same location.
        let src = "abc\nd🌍fg\nhij";
        let index = LineIndex::tree_sitter(src);
        assert_eq!(index.row_col_to_utf16_column(src, 0, 2), 2); // "ab" -> col 2
        assert_eq!(index.row_col_to_utf16_column(src, 1, 5), 3); // "d🌍" -> col 3 (1 + 2 units)
        assert_eq!(index.row_col_to_utf16_column(src, 2, 3), 3); // "hij" -> col 3
    }
}

// ── property tests ───────────────────────────────────────────────────────────
//
// The offset helpers silently corrupt positions if they mishandle multibyte
// characters, surrogate pairs, CRLF, or trailing newlines, so the invariants
// are exercised over generated content mixing exactly those shapes.

#[cfg(test)]
mod proptests {
    use super::tests::utf16_to_byte_offset;
    use super::*;
    use proptest::prelude::*;

    /// ASCII, combining accents, multibyte BMP, astral (surrogate pairs in
    /// UTF-16), and both newline conventions.
    fn content_strategy() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                proptest::char::range('a', 'z'),
                Just('é'),
                Just('中'),
                Just('😀'),
                Just('\n'),
                Just('\r'),
            ],
            0..64,
        )
        .prop_map(|chars| chars.into_iter().collect())
    }

    proptest! {
        #[test]
        fn char_byte_offsets_round_trip_on_utf16_boundaries(content in content_strategy()) {
            let mut utf16 = 0usize;
            let mut byte = 0usize;
            for ch in content.chars() {
                prop_assert_eq!(utf16_to_byte_offset(&content, utf16), byte);
                prop_assert_eq!(byte_to_utf16_offset(&content, byte), utf16);
                utf16 += ch.len_utf16();
                byte += ch.len_utf8();
            }
            // Trailing boundary (covers trailing-newline content).
            prop_assert_eq!(utf16_to_byte_offset(&content, utf16), byte);
            prop_assert_eq!(byte_to_utf16_offset(&content, byte), utf16);
        }

        #[test]
        fn arbitrary_offsets_snap_to_valid_boundaries(
            content in content_strategy(),
            offset in 0usize..96,
        ) {
            let byte = utf16_to_byte_offset(&content, offset);
            prop_assert!(byte <= content.len());
            prop_assert!(content.is_char_boundary(byte));
            let units = byte_to_utf16_offset(&content, offset);
            prop_assert!(units <= content.chars().map(char::len_utf16).sum::<usize>());
        }

        #[test]
        fn line_index_positions_round_trip_on_char_boundaries(content in content_strategy()) {
            let index = LineIndex::tree_sitter(&content);
            for (byte, _) in content.char_indices().chain([(content.len(), '\0')]) {
                let (line, column) = index.byte_to_position(&content, byte as u32);
                prop_assert_eq!(
                    index.position_to_byte(&content, line, column),
                    byte as u32,
                    "byte {} in {:?}", byte, content
                );
            }
        }

        #[test]
        fn lsp_rule_positions_round_trip_on_char_boundaries(content in content_strategy()) {
            let index = LineIndex::new(&content);
            for (byte, ch) in content.char_indices().chain([(content.len(), '\0')]) {
                // Bytes inside a terminator map to the end of their line.
                if matches!(ch, '\r' | '\n') {
                    continue;
                }
                let (line, column) = index.byte_to_position(&content, byte as u32);
                prop_assert_eq!(index.position_of(byte).0, line as usize);
                prop_assert_eq!(
                    index.position_to_byte(&content, line, column),
                    byte as u32,
                    "byte {} in {:?}", byte, content
                );
            }
        }

        #[test]
        fn mid_character_bytes_floor_to_the_character_start(content in content_strategy()) {
            let index = LineIndex::tree_sitter(&content);
            for byte in 0..=content.len() {
                let (line, column) = index.byte_to_position(&content, byte as u32);
                let floored = {
                    let mut b = byte;
                    while b > 0 && !content.is_char_boundary(b) {
                        b -= 1;
                    }
                    b
                };
                prop_assert_eq!(index.position_to_byte(&content, line, column), floored as u32);
            }
        }

        #[test]
        fn row_column_conversion_matches_absolute_byte_conversion(content in content_strategy()) {
            let index = LineIndex::tree_sitter(&content);
            for (byte, ch) in content.char_indices() {
                if ch == '\n' {
                    continue;
                }
                let (line, column) = index.byte_to_position(&content, byte as u32);
                let line_start = index.line_starts_byte[line as usize] as usize;
                prop_assert_eq!(
                    index.row_col_to_utf16_column(&content, line, (byte - line_start) as u32),
                    column
                );
            }
        }

    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;

    /// Naive per-call scan from the line start — the pre-checkpoint behavior.
    fn naive_position(content: &str, byte: usize) -> (u32, u32) {
        let byte = floor_char_boundary(content, byte.min(content.len()));
        let line_start = content[..byte].rfind('\n').map_or(0, |nl| nl + 1);
        let line = content[..line_start].matches('\n').count() as u32;
        let mut column: u32 = content[line_start..byte]
            .chars()
            .map(|c| c.len_utf16() as u32)
            .sum();
        if line == 0 && content.starts_with(BOM) {
            column = column.saturating_sub(1);
        }
        (line, column)
    }

    fn long_mixed_content() -> String {
        let mut content = String::from(BOM);
        for i in 0..6_000 {
            content.push_str(match i % 7 {
                0 => "é",
                1 => "😀",
                2 => "\r\n",
                3 => "abc",
                4 => "\n",
                5 => "中",
                _ => "x",
            });
            if i % 997 == 0 {
                content.push_str(&"z".repeat(3_000)); // very long line runs
            }
        }
        content
    }

    #[test]
    fn checkpointed_columns_match_a_naive_scan_everywhere() {
        let content = long_mixed_content();
        let index = LineIndex::tree_sitter(&content);
        for (byte, _) in content.char_indices().step_by(7) {
            assert_eq!(
                index.byte_to_position(&content, byte as u32),
                naive_position(&content, byte),
                "byte {byte}"
            );
            if byte == 0 {
                continue; // the hidden BOM maps back to the first byte after it
            }
            let (line, column) = index.byte_to_position(&content, byte as u32);
            // position_to_byte inverts byte_to_position on char boundaries.
            assert_eq!(
                index.position_to_byte(&content, line, column) as usize,
                byte,
                "round trip at byte {byte}"
            );
        }
    }

    #[test]
    fn a_minified_single_line_stays_linear() {
        // 2 MB on one line with multibyte chars: a per-call line scan would
        // make 20k lookups ~quadratic; checkpoints keep it to one gap per lookup.
        let content = "é😀ab".repeat(250_000);
        let index = LineIndex::tree_sitter(&content);
        let started = std::time::Instant::now();
        let mut total = 0u64;
        for (byte, _) in content.char_indices().step_by(50) {
            total += u64::from(index.byte_to_position(&content, byte as u32).1);
        }
        assert!(total > 0);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
    }
}
