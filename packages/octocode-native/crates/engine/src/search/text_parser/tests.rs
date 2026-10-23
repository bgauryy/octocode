use super::*;

/// Assembles `f.ts` from `(line_number, text, Some(column))` matches and
/// `(line_number, text, None)` context lines.
fn assemble(
    lines: &[(u32, &str, Option<u32>)],
    context_lines: u32,
    max_snippet: usize,
) -> TextSearchFile {
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
        max_snippet,
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
            40,
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
    let f = assemble(&[(10, "  const x = 1;", Some(8))], 0, 500);
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
        500,
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
        500,
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
        500,
    );
    assert_eq!(f.matches[0].value, "\n\nmatch\n\n");
    assert_eq!(f.matches[1].value, "match\n\n");
}

/// Regression: a match at `u32::MAX` with forward context lines must not
/// overflow on `line_number + i` (release has no overflow checks).
#[test]
fn forward_context_does_not_overflow_at_u32_max() {
    let f = assemble(&[(u32::MAX, "match", Some(0))], 3, 500);
    assert_eq!(f.matches[0].line, u32::MAX);
    assert!(f.matches[0].value.contains("match"));
}

#[test]
fn truncated_content_snippet_records_original_char_length() {
    // A clipped content-view snippet carries the pre-truncation
    // Unicode length so callers can surface a truncation indicator.
    let long = "a".repeat(600);
    let f = assemble(&[(1, &long, Some(0))], 0, 500);
    assert_eq!(f.matches[0].original_chars, Some(600));
    assert_eq!(f.matches[0].original_bytes, Some(600));
    // Bytes are counted apart from characters: `中` takes three.
    let wide = "中".repeat(600);
    for context in [0, 1] {
        let f = assemble(&[(1, &wide, Some(0))], context, 500);
        assert_eq!(f.matches[0].original_chars, Some(600), "{context}");
        assert_eq!(f.matches[0].original_bytes, Some(1_800), "{context}");
    }
    let short = assemble(&[(1, "match", Some(0))], 0, 500);
    assert_eq!(short.matches[0].original_bytes, None);
}

#[test]
fn oversized_context_drops_far_leading_lines_before_the_match() {
    let far = "f".repeat(60);
    let near = "n".repeat(20);
    let f = assemble(
        &[
            (8, &far, None),
            (9, &near, None),
            (10, "fn target() {", Some(3)),
            (11, "    body();", None),
            (12, &far, None),
        ],
        2,
        80,
    );
    let val = &f.matches[0].value;
    assert!(val.starts_with(&near), "{val}");
    assert!(val.contains("fn target() {\n    body();"), "{val}");
    assert!(val.chars().count() <= 80, "{val}");
}

/// A hit line kept whole is moved into its value; every value still
/// equals the clip of the line, including a fitting line whose match
/// sits in its tail (that line stays whole).
#[test]
fn moved_line_values_equal_the_clipped_line() {
    let lines = [
        ("short line", 0),
        ("ab", 1),
        (&*"x".repeat(100), 95),
        (&*"y".repeat(100), 10),
        (&*"z".repeat(120), 110),
        ("café → naïve ✓ needle", 15),
        ("", 0),
    ];
    for max in [0, 2, 3, 4, 50, 100, 500] {
        for (line, column) in lines {
            let f = assemble(&[(1, line, Some(column))], 0, max);
            assert_eq!(
                f.matches[0].value,
                clip_around_match(line, column, max),
                "{line:?} {column} {max}"
            );
        }
    }
}

/// A hit near the end of a long line gets a window that spends the
/// whole budget, not a quarter-snippet lead and the short tail: the
/// window shifts left, ends on the line's own last char, and keeps the
/// match visible.
#[test]
fn a_window_near_the_line_end_fills_the_budget() {
    let line = format!("{}cursor:pointer}}", "a".repeat(230));
    let column = u32::try_from(line.find("cursor").expect("hit")).expect("column");
    for max in [40, 200] {
        let value = clip_around_match(&line, column, max);
        assert_eq!(value.chars().count(), max, "{value}");
        assert!(
            value.starts_with('…') && value.ends_with("cursor:pointer}"),
            "{value}"
        );
    }
    // A hit mid-line keeps its quarter-snippet lead.
    let mid = format!("{}needle{}", "a".repeat(300), "b".repeat(300));
    let value = clip_around_match(&mid, 300, 40);
    assert!(
        value.starts_with(&format!("…{}needle", "a".repeat(10))),
        "{value}"
    );
    assert!(value.ends_with("..."), "{value}");
    assert_eq!(value.chars().count(), 40, "{value}");
}

#[test]
fn untruncated_content_snippet_has_no_original_char_length() {
    let f = assemble(&[(1, "short line", Some(0))], 0, 500);
    assert_eq!(f.matches[0].original_chars, None);
}

#[test]
fn fitting_tail_matches_keep_the_complete_source_line() {
    for prefix in ["x", "é", "😀"] {
        let line = format!("{}needle", prefix.repeat(34));
        let column = u32::try_from(prefix.encode_utf16().count() * 34).unwrap();
        for context in [0, 1] {
            for limit in [40, 41] {
                let file = assemble(&[(1, &line, Some(column))], context, limit);
                let hit = &file.matches[0];
                assert_eq!(hit.value, line, "{prefix} context={context} limit={limit}");
                assert_eq!(hit.original_chars, None);
                assert_eq!(hit.original_bytes, None);
            }
            let clipped = assemble(&[(1, &line, Some(column))], context, 39);
            assert!(clipped.matches[0].value.contains("needle"));
            assert_eq!(clipped.matches[0].original_chars, Some(40));
            assert_eq!(clipped.matches[0].original_bytes, Some(line.len() as u32));
        }
    }
}

#[test]
fn complete_value_identity_is_independent_of_display_width() {
    let first = format!("needle {} first", "a".repeat(50));
    let second = format!("needle {} second", "a".repeat(50));
    let lines = [(1, first.as_str(), Some(0)), (2, second.as_str(), Some(0))];
    for context in [0, 1] {
        let narrow = assemble(&lines, context, 20);
        let whole = assemble(&lines, context, 500);
        for (clipped, complete) in narrow.matches.iter().zip(&whole.matches) {
            assert_ne!(clipped.value, complete.value);
            assert_eq!(clipped.value_digest, complete.value_digest);
        }
        if context == 0 {
            assert_eq!(narrow.matches[0].value, narrow.matches[1].value);
            assert_ne!(
                narrow.matches[0].value_digest,
                narrow.matches[1].value_digest
            );
        }
    }
    let padded = format!("  {first}  ");
    let duplicates = assemble(&[(1, &first, Some(0)), (2, &padded, Some(2))], 0, 20);
    assert_eq!(
        duplicates.matches[0].value_digest,
        duplicates.matches[1].value_digest
    );
}

#[test]
fn truncates_long_snippets() {
    let long = "a".repeat(600);
    let f = assemble(&[(1, &long, Some(0))], 0, 10);
    let val = &f.matches[0].value;
    assert!(val.ends_with("..."));
    assert!(val.chars().count() <= 10);
}

#[test]
fn groups_multiple_matches_per_file() {
    let f = assemble(&[(1, "line1", Some(0)), (5, "line2", Some(0))], 0, 500);
    assert_eq!(f.match_count, 2);
}

#[test]
fn preserves_unicode_content() {
    let f = assemble(&[(1, "café → naïve", Some(0))], 0, 500);
    assert_eq!(f.matches[0].value, "café → naïve");
}

#[test]
fn truncate_unicode_counts_chars_not_bytes() {
    // "café" is 4 chars but 5 bytes (é = 2 bytes)
    let s = "café world";
    let r = truncate_unicode(s, 4);
    // should truncate at "c" boundary before limit and add "..."
    // limit 4 → head at max_chars-3=1 chars + "..."
    assert!(r.ends_with("..."));
}

#[test]
fn truncate_unicode_zero_limit_returns_empty() {
    assert_eq!(truncate_unicode("hello", 0), "");
}

/// A limit too short for the `...` marker returns real chars, never
/// only dots.
#[test]
fn truncate_unicode_tiny_limits_return_real_chars() {
    assert_eq!(truncate_unicode("hello", 1), "h");
    assert_eq!(truncate_unicode("hello", 2), "he");
    assert_eq!(truncate_unicode("hello", 3), "hel");
    assert_eq!(truncate_unicode("héllo", 2), "hé");
}

/// A tiny limit shows the match's own chars, ending at the line's end.
#[test]
fn a_tiny_clip_shows_the_match() {
    assert_eq!(clip_around_match("xxxxx needle", 6, 3), "nee");
    assert_eq!(clip_around_match("xxxxx needle", 11, 3), "dle");
    assert_eq!(clip_around_match("needle xxxxx", 0, 2), "ne");
}

#[test]
fn truncate_unicode_tiny_limits_preserve_short_input() {
    assert_eq!(truncate_unicode("é", 1), "é");
    assert_eq!(truncate_unicode("é", 2), "é");
}
