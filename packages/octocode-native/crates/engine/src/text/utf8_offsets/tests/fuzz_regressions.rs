//! Deterministic replays of failures the `utf8_offsets` libFuzzer target
//! (`packages/octocode-native/fuzz`) found. Each test is the minimized input,
//! decoded to text, with the oracle's expectation.
use super::*;

/// fuzz: a tree-sitter byte column inside a multibyte character (here the
/// leading BOM) returned the raw byte count, past the character's start and
/// unlike `byte_to_position`, which snaps the same byte down to the character.
#[test]
fn row_column_inside_a_character_snaps_down_like_byte_offsets() {
    let content = "\u{feff}éz\n😀b";
    let index = LineIndex::tree_sitter(content);
    for byte in 0..content.len() {
        let (line, column) = index.byte_to_position(content, byte as u32);
        let line_start = index.line_start(line as usize).unwrap_or(0);
        if byte < line_start {
            continue;
        }
        assert_eq!(
            index.row_col_to_utf16_column(content, line, (byte - line_start) as u32),
            column,
            "byte {byte}"
        );
    }
}

/// fuzz: under the LSP rule, a UTF-16 checkpoint can land on the `\n` of a
/// `\r\n` (here the 1024th byte). A column past the end of that line started
/// from the checkpoint and returned the `\n` byte instead of clamping to the
/// line end (the `\r`), which is where every other line's overflow clamps.
#[test]
fn column_past_a_crlf_line_clamps_to_the_line_end_at_a_checkpoint() {
    let line = format!("{}中😀", "z".repeat(1016));
    let content = format!("{line}\r\nnext");
    assert_eq!(
        content.find('\n'),
        Some(1024),
        "a checkpoint lands on the \\n"
    );
    let index = LineIndex::new(&content);
    let width = line.encode_utf16().count() as u32;
    for column in [width, width + 1, width + 2, u32::MAX] {
        assert_eq!(
            index.position_to_byte(&content, 0, column) as usize,
            line.len(),
            "column {column}"
        );
    }
}
