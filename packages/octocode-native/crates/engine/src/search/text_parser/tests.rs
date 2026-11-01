use super::*;

/// Assembles `f.ts` from `(line_number, text, Some(column))` matches and
/// `(line_number, text, None)` context lines.
fn assemble(lines: &[(u32, &str, Option<u32>)], context_lines: u32) -> TextSearchFile {
    let mut entry = FileEntry::new();
    for &(line_number, text, column) in lines {
        match column {
            Some(column) => entry.raw_matches.push(RawMatch {
                line_text: text.to_owned(),
                line_number,
                column,
            }),
            None => {
                entry.contexts.insert(line_number, text.to_owned());
            }
        }
    }
    assemble_file(
        "f.ts".to_owned(),
        entry,
        SearchContext::symmetric(context_lines),
    )
}

/// A match's byte offset locates it in the line for a byte-window read,
/// whatever the width of the characters before it.
#[test]
fn a_match_carries_its_byte_offset_in_its_line() {
    // "é" is 2 bytes and 1 UTF-16 unit; "😀" is 4 bytes and 2 units.
    let line = format!("{}😀needle{}", "é".repeat(300), "x".repeat(300));
    let column = 300 + 2;
    for context_lines in [0, 1] {
        let f = assemble_file(
            "f.ts".to_owned(),
            {
                let mut entry = FileEntry::new();
                entry.raw_matches.push(RawMatch {
                    line_text: line.clone(),
                    line_number: 1,
                    column,
                });
                entry
            },
            SearchContext::symmetric(context_lines),
        );
        let offset = f.matches[0].byte_offset as usize;
        assert_eq!(offset, 600 + 4, "context {context_lines}");
        assert!(
            line[offset..].starts_with("needle"),
            "context {context_lines}"
        );
    }
}

#[test]
fn assembles_single_match() {
    let f = assemble(&[(10, "  const x = 1;", Some(8))], 0);
    assert_eq!(f.path, "f.ts");
    assert_eq!(f.match_count, 1);
    assert_eq!(f.matches[0].line, 10);
    assert_eq!(f.matches[0].column, 8);
    assert_eq!(f.matches[0].value, "  const x = 1;");
}

#[test]
fn each_match_carries_its_lexical_hit_rank() {
    let f = assemble(
        &[
            (1, "   * see maximumSize", Some(9)),
            (2, "    check(maximumSize);", Some(10)),
            (3, "    this.maximumSize = maximumSize;", Some(9)),
        ],
        0,
    );
    let ranks = f.matches.iter().map(|m| m.rank).collect::<Vec<_>>();
    assert_eq!(ranks, vec![Some(0), Some(1), Some(2)]);
}

#[test]
fn strips_one_trailing_newline() {
    assert_eq!(strip_trailing_newline("line\n".to_owned()), "line");
    assert_eq!(strip_trailing_newline("line\r\n".to_owned()), "line");
    assert_eq!(strip_trailing_newline("line\n\n".to_owned()), "line\n");
}

#[test]
fn assembles_context_window() {
    let f = assemble(
        &[
            (9, "before", None),
            (10, "match", Some(0)),
            (11, "after", None),
        ],
        1,
    );
    assert_eq!(f.matches[0].value, "before\nmatch\nafter");
}

/// Regression: a blank leading context line (or blank match line) was
/// dropped because the joiner skipped the separator while the buffer was
/// still empty, shifting every later line off its line number.
#[test]
fn blank_leading_context_lines_keep_their_line_slot() {
    let f = assemble(
        &[
            (8, "", None),
            (9, "", None),
            (10, "match", Some(0)),
            (11, "", None),
            (12, "", Some(0)),
        ],
        2,
    );
    assert_eq!(f.matches[0].value, "\n\nmatch\n\n");
    assert_eq!(f.matches[1].value, "match\n\n");
}

/// Regression: a match at `u32::MAX` with forward context lines must not
/// overflow on `line_number + i` (release has no overflow checks).
#[test]
fn forward_context_does_not_overflow_at_u32_max() {
    let f = assemble(&[(u32::MAX, "match", Some(0))], 3);
    assert_eq!(f.matches[0].line, u32::MAX);
    assert!(f.matches[0].value.contains("match"));
}

#[test]
fn complete_values_preserve_long_unicode_context_and_identity() {
    let before = "é😀".repeat(1_000);
    let hit = format!("{}needle", "界".repeat(1_000));
    let after = "tail".repeat(1_000);
    let file = assemble(
        &[
            (8, &before, None),
            (9, "", None),
            (10, &hit, Some(1000)),
            (11, &after, None),
        ],
        2,
    );
    let expected = format!("{before}\n\n{hit}\n{after}");
    assert_eq!(file.matches[0].value, expected);
    assert_eq!(
        file.matches[0].value_digest,
        TextSearchMatch::digest_of(&expected)
    );
    assert_eq!(file.matches[0].byte_offset, 3000);
}

#[test]
fn complete_distinct_values_keep_distinct_identity_and_whitespace() {
    let first = format!("needle {} first", "a".repeat(5000));
    let second = format!("needle {} second", "a".repeat(5000));
    let padded = format!("  {first}  ");
    let file = assemble(
        &[
            (1, &first, Some(0)),
            (2, &second, Some(0)),
            (3, &padded, Some(2)),
        ],
        0,
    );
    assert_eq!(file.matches[0].value, first);
    assert_eq!(file.matches[1].value, second);
    assert_eq!(file.matches[2].value, padded);
    assert_ne!(file.matches[0].value_digest, file.matches[1].value_digest);
    assert_eq!(file.matches[0].value_digest, file.matches[2].value_digest);
}

#[test]
fn every_line_value_is_complete_at_utf16_columns() {
    for prefix in ["x", "é", "😀"] {
        let line = format!("{}needle", prefix.repeat(1000));
        let column = u32::try_from(prefix.encode_utf16().count() * 1000).unwrap();
        for context in [0, 1] {
            let file = assemble(&[(1, &line, Some(column))], context);
            assert_eq!(file.matches[0].value, line);
            assert_eq!(file.matches[0].byte_offset as usize, prefix.len() * 1000);
        }
    }
}
