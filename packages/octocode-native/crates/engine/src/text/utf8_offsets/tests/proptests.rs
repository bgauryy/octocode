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
