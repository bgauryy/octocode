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
    let valid_offset = s.floor_char_boundary(clamped);
    utf16_len(&s[..valid_offset])
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// A leading byte-order mark. Editors and LSP clients hide it, so user-facing
/// row-0 columns and line text exclude it while byte offsets keep counting it.
pub(crate) const BOM: char = '\u{feff}';

/// The one leading-BOM rule: the bytes of the UTF-8 byte-order mark `text`
/// starts with (3), or 0. A U+FEFF anywhere else is text.
pub fn bom_len(text: &[u8]) -> usize {
    let mut mark = [0; 3];
    if text.starts_with(BOM.encode_utf8(&mut mark).as_bytes()) {
        mark.len()
    } else {
        0
    }
}

/// Hide a leading BOM from a row-0 line: returns the line text without it and
/// `byte_column` rebased past it. Other rows (and BOM-free text) pass through.
pub(crate) fn hide_bom_in_line(row: usize, line: &str, byte_column: usize) -> (&str, usize) {
    match bom_len(line.as_bytes()) {
        bom if row == 0 && bom > 0 => (&line[bom..], byte_column.saturating_sub(bom)),
        _ => (line, byte_column),
    }
}

// ── LineIndex ─────────────────────────────────────────────────────────────────

/// Byte offset of the first byte of every line of `text`, lines ending at
/// `\n` (the tree-sitter and text-search rule): `[0, …]`, one entry per line,
/// a text ending in `\n` closing with the empty line after it.
pub fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

/// The 1-based line of byte `offset` under [`line_starts`].
pub fn line_at(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|start| *start <= offset).max(1)
}

/// The 1-based line of byte `offset` of `text`, counted in place: the same
/// answer as [`line_at`] over [`line_starts`] for one lookup, without
/// building the table. `offset` past the end is the last line.
pub fn line_containing(text: &str, offset: usize) -> usize {
    1 + text.as_bytes()[..offset.min(text.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

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
/// - [`LineIndex::tree_sitter`]: `\n` only, as tree-sitter rows and the text search
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
    /// a syntax tree (or a text-search line) must use this rule, or a lone `\r`
    /// shifts every later line against the grammar's own row numbering.
    pub fn tree_sitter(content: &str) -> Self {
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
            bom_utf16: u32::from(bom_len(content.as_bytes()) > 0),
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
    #[cfg(test)]
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
        let end = content.floor_char_boundary(byte_offset as usize);
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
    pub fn position_to_byte(&self, content: &str, line: u32, character: u32) -> u32 {
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
        let slot = self
            .utf16_checkpoints
            .partition_point(|&(_, units)| units <= target)
            .saturating_sub(1);
        let (mut byte, mut utf16) = match self.utf16_checkpoints.get(slot) {
            // Only a checkpoint inside this line's text is a valid starting
            // point: one on the `\n` of a `\r\n` is past the line end.
            Some(&(cp_byte, cp_units))
                if (cp_byte as usize) > line_start && (cp_byte as usize) <= line_end =>
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
    /// bounds; a byte inside a character snaps down to the character's start,
    /// as in [`byte_to_position`](Self::byte_to_position).
    pub(crate) fn row_col_to_utf16_column(&self, content: &str, row: u32, byte_column: u32) -> u32 {
        let row = row as usize;
        let line_start = self.line_starts_byte.get(row).copied().unwrap_or(0) as usize;
        let line_end = self
            .line_ends_byte
            .get(row)
            .map_or(content.len(), |&end| end as usize)
            .min(content.len());
        let byte_end = content.floor_char_boundary(
            line_start
                .saturating_add(byte_column as usize)
                .min(line_end),
        );
        self.hide_bom(row, self.utf16_column(content, row, byte_end))
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

#[cfg(test)]
mod tests;
