//! libFuzzer target: the engine's UTF-8 ⇄ UTF-16 offset conversions
//! (`crates/engine/src/text/utf8_offsets.rs`) against a naive oracle.
//!
//! Input: one header byte, then the text. Header bit 0 selects the line rule
//! (set: the LSP rule `LineIndex::new`; clear: `LineIndex::tree_sitter`). Bit 1
//! selects the text decoding (set: the raw bytes, lossily as UTF-8; clear: each
//! byte picks a token from `TOKENS`, or with the high bit set a run of `z`, so
//! short inputs reach the 1024-byte UTF-16 checkpoints and every line break).
#![no_main]

use libfuzzer_sys::fuzz_target;

// The real implementation, compiled by path: it depends on std only.
#[allow(dead_code)]
#[path = "../../crates/engine/src/text/utf8_offsets.rs"]
mod utf8_offsets;

use utf8_offsets::{BOM, LineIndex, byte_to_utf16_offset, line_at, line_starts};

/// ASCII, 2-, 3- and 4-byte characters (the last a UTF-16 surrogate pair),
/// a combining mark, the BOM, U+2028, and every line-break form.
const TOKENS: [&str; 14] = [
    "a", "bc", "é", "中", "😀", "\n", "\r", "\r\n", "\u{feff}", "\u{2028}", "\u{301}", "\t", " ",
    "zz",
];

/// Bounds the per-input cost while still crossing two UTF-16 checkpoints.
const MAX_TEXT_BYTES: usize = 2560;

/// The engine's UTF-16 checkpoint spacing (`UTF16_CHECKPOINT_BYTES`).
const CHECKPOINT: usize = 1024;

/// Texts and lines up to this size are probed at every offset; longer ones
/// near every mark (line edges, checkpoints) and at a stride.
const EXHAUSTIVE: usize = 512;

fuzz_target!(|data: &[u8]| {
    if let Some((lsp, text)) = decode(data) {
        check(&text, lsp);
    }
});

fn decode(data: &[u8]) -> Option<(bool, String)> {
    let (&header, body) = data.split_first()?;
    let mut text = if header & 2 != 0 {
        String::from_utf8_lossy(body).into_owned()
    } else {
        let mut text = String::new();
        for &byte in body {
            if byte & 0x80 != 0 {
                text.extend(std::iter::repeat_n('z', usize::from(byte & 0x7f) * 8));
            } else {
                text.push_str(TOKENS[usize::from(byte) % TOKENS.len()]);
            }
        }
        text
    };
    if text.len() > MAX_TEXT_BYTES {
        text.truncate(floor(&text, MAX_TEXT_BYTES));
    }
    Some((header & 1 != 0, text))
}

/// One line: its first byte and the byte where its text ends (terminator excluded).
struct Line {
    start: usize,
    end: usize,
}

/// Lines by byte scan: `\n` always ends a line; under the LSP rule `\r\n`
/// and a lone `\r` do too.
fn naive_lines(text: &str, lsp: bool) -> Vec<Line> {
    let bytes = text.as_bytes();
    let (mut lines, mut start, mut at) = (Vec::new(), 0, 0);
    while at < bytes.len() {
        let width = match bytes[at] {
            b'\n' => 1,
            b'\r' if lsp => 1 + usize::from(bytes.get(at + 1) == Some(&b'\n')),
            _ => 0,
        };
        if width == 0 {
            at += 1;
            continue;
        }
        lines.push(Line { start, end: at });
        start = at + width;
        at = start;
    }
    lines.push(Line {
        start,
        end: text.len(),
    });
    lines
}

fn floor(text: &str, mut byte: usize) -> usize {
    byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// `0..=last` when short, else every value within 8 of a mark plus a stride.
fn probes(last: usize, marks: impl IntoIterator<Item = usize>) -> Vec<usize> {
    if last <= EXHAUSTIVE {
        return (0..=last).collect();
    }
    let mut out: Vec<usize> = (0..=last).step_by(61).chain([last]).collect();
    for mark in marks {
        out.extend(mark.saturating_sub(8)..=mark.saturating_add(8).min(last));
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Expected `(line, visible UTF-16 column, UTF-16 units from content start)`
/// for every byte offset `0..=len`. A byte inside a character takes the
/// character start's values; row 0 hides a leading BOM.
fn expected_positions(text: &str, lines: &[Line]) -> Vec<(usize, usize, usize)> {
    let bom = text.starts_with(BOM);
    let mut table = vec![(0, 0, 0); text.len() + 1];
    let (mut line, mut column, mut total) = (0usize, 0usize, 0usize);
    let ends = text.char_indices().chain([(text.len(), '\0')]);
    for (byte, ch) in ends {
        while line + 1 < lines.len() && lines[line + 1].start <= byte {
            line += 1;
            column = 0;
        }
        let shown = if line == 0 && bom {
            column.saturating_sub(1)
        } else {
            column
        };
        let width = if byte == text.len() { 1 } else { ch.len_utf8() };
        table[byte..byte + width].fill((line, shown, total));
        column += ch.len_utf16();
        total += ch.len_utf16();
    }
    table
}

fn check(text: &str, lsp: bool) {
    let index = if lsp {
        LineIndex::new(text)
    } else {
        LineIndex::tree_sitter(text)
    };
    let lines = naive_lines(text, lsp);
    let bom = text.starts_with(BOM);
    assert_eq!(index.len(), lines.len(), "line count");
    let ends_with_break = !text.is_empty() && lines[lines.len() - 1].start == text.len();
    assert_eq!(
        index.content_len(),
        lines.len() - usize::from(ends_with_break)
    );
    for (row, line) in lines.iter().enumerate() {
        assert_eq!(index.line_start(row), Some(line.start), "line_start({row})");
        assert_eq!(index.line(text, row), text.get(line.start..line.end));
    }
    let checkpoints = (CHECKPOINT..=text.len()).step_by(CHECKPOINT);
    let edges = lines.iter().flat_map(|line| [line.start, line.end]);
    let bytes = probes(text.len() + 2, edges.chain(checkpoints));
    check_positions(text, &index, &lines, bom, &bytes);
    check_columns(text, &index, &lines, bom);
    if !lsp {
        let starts = line_starts(text);
        let naive: Vec<usize> = lines.iter().map(|line| line.start).collect();
        assert_eq!(starts, naive, "line_starts");
        for &byte in bytes.iter().filter(|&&byte| byte <= text.len()) {
            assert_eq!(line_at(&starts, byte), index.position_of(byte).0 + 1);
        }
    }
}

/// Byte offset → position, and back on character boundaries.
fn check_positions(text: &str, index: &LineIndex, lines: &[Line], bom: bool, bytes: &[usize]) {
    let table = expected_positions(text, lines);
    for &byte in bytes {
        let (line, column, total) = table[byte.min(text.len())];
        let got = index.byte_to_position(text, byte as u32);
        assert_eq!(
            got,
            (line as u32, column as u32),
            "byte_to_position({byte})"
        );
        assert_eq!(
            byte_to_utf16_offset(text, byte),
            total,
            "utf16 offset {byte}"
        );
        let row = &lines[line];
        let held = byte.min(row.end).max(row.start) - row.start;
        assert_eq!(index.position_of(byte.min(text.len())), (line, held));
        let boundary = byte <= text.len() && text.is_char_boundary(byte);
        if boundary && byte <= row.end && !(bom && byte == 0) {
            let back = index.position_to_byte(text, got.0, got.1);
            assert_eq!(back as usize, byte, "round trip of byte {byte}");
        }
        if byte <= row.end {
            let byte_column = (byte - row.start) as u32;
            let column_of = index.row_col_to_utf16_column(text, line as u32, byte_column);
            assert_eq!(column_of, got.1, "row {line} byte column {byte_column}");
        }
    }
}

/// Column → byte offset: monotonic, on character boundaries inside the line,
/// and clamped to the line end (or the text end past the last line).
fn check_columns(text: &str, index: &LineIndex, lines: &[Line], bom: bool) {
    for (row, line) in lines.iter().enumerate() {
        let width = units(&text[line.start..line.end]) - usize::from(row == 0 && bom);
        let hidden = usize::from(row == 0 && bom);
        let checkpoints = (CHECKPOINT..=line.end)
            .step_by(CHECKPOINT)
            .filter(|&byte| byte > line.start)
            .map(|byte| units(&text[line.start..floor(text, byte)]).saturating_sub(hidden));
        let columns = probes(width + 2, [0, width].into_iter().chain(checkpoints));
        let mut previous = line.start;
        for column in columns
            .into_iter()
            .map(|column| column as u32)
            .chain([u32::MAX])
        {
            let byte = index.position_to_byte(text, row as u32, column) as usize;
            assert!(text.is_char_boundary(byte), "row {row} column {column}");
            assert!(
                (previous..=line.end).contains(&byte),
                "row {row} column {column} -> {byte}, line {}..{}, previous {previous}",
                line.start,
                line.end
            );
            if column as usize >= width {
                assert_eq!(
                    byte, line.end,
                    "row {row} column {column} clamps to the line end"
                );
            }
            previous = byte;
        }
    }
    for row in [lines.len() as u32, u32::MAX] {
        assert_eq!(index.position_to_byte(text, row, 0) as usize, text.len());
    }
}
