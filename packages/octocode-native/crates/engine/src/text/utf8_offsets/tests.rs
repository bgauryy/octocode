use super::*;

#[test]
fn line_starts_split_at_newlines_and_line_at_finds_the_line() {
    let text = "ab\ncd\n\nx";
    let starts = line_starts(text);
    assert_eq!(starts, [0, 3, 6, 7]);
    assert_eq!(line_at(&starts, 0), 1);
    assert_eq!(line_at(&starts, 2), 1);
    assert_eq!(line_at(&starts, 3), 2);
    assert_eq!(line_at(&starts, 6), 3);
    assert_eq!(line_at(&starts, 9), 4);
    assert_eq!(line_starts(""), [0]);
}

#[test]
fn one_lookup_line_matches_the_table_lookup_at_every_offset() {
    for text in ["", "ab\ncd\n\nx", "é\n\n", "no newline"] {
        let starts = line_starts(text);
        for offset in 0..=text.len() + 2 {
            assert_eq!(
                line_containing(text, offset),
                line_at(&starts, offset.min(text.len())),
                "{text:?} @{offset}"
            );
        }
    }
}

/// Number of UTF-8 bytes up to (not including) the `char_index`-th
/// UTF-16 code unit in `s`. Clamps to `s.len()` if `char_index` exceeds the string.
fn utf16_to_byte_offset(s: &str, char_index: usize) -> usize {
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
fn bom_len_counts_only_a_leading_bom() {
    assert_eq!(bom_len("\u{feff}foo".as_bytes()), 3);
    assert_eq!(bom_len(b"foo\xef\xbb\xbf"), 0);
    assert_eq!(bom_len(b"\xef\xbb"), 0);
    assert_eq!(bom_len(b""), 0);
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

/// Byte offsets map to a (line, UTF-16 column) position.
#[test]
fn line_index_byte_to_position_cases() {
    // (case, source, byte offset → (line, column) pairs)
    type Case = (&'static str, &'static str, &'static [(u32, (u32, u32))]);
    let cases: [Case; 4] = [
        (
            "ascii",
            "line1\nline2\nline3",
            // inside "line1", start of "line2", start of "line3"
            &[(0, (0, 0)), (3, (0, 3)), (6, (1, 0)), (12, (2, 0))],
        ),
        (
            // Line 0 is "a🌍b" (byte len 6), line 1 is "cd".
            "multibyte",
            "a🌍b\ncd",
            // 'a', start of 🌍, 'b' after the 2-unit emoji, start of "cd"
            &[(0, (0, 0)), (1, (0, 1)), (5, (0, 3)), (7, (1, 0))],
        ),
        ("clamps beyond length", "hi", &[(100, (0, 2))]),
        (
            // A byte offset landing inside 🌍 (bytes 1..5) must report the column of
            // the character it falls in — 🌍 starts at UTF-16 column 1 — rather than
            // collapse to column 0 as a non-char-boundary slice silently would.
            // Character boundaries (1 and 5) are unaffected by the snap.
            "snaps a mid-multibyte offset down",
            "a🌍b\ncd",
            &[
                (2, (0, 1)),
                (3, (0, 1)),
                (4, (0, 1)),
                (1, (0, 1)),
                (5, (0, 3)),
            ],
        ),
    ];
    for (case, src, expected) in cases {
        let index = LineIndex::tree_sitter(src);
        for &(byte, position) in expected {
            assert_eq!(
                index.byte_to_position(src, byte),
                position,
                "{case}: byte {byte}"
            );
        }
    }
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

// The offset helpers silently corrupt positions if they mishandle multibyte
// characters, surrogate pairs, CRLF, or trailing newlines, so the invariants
// are exercised over generated content mixing exactly those shapes.
mod proptests;

mod checkpoints;

mod fuzz_regressions;
