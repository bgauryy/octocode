//! Assembles ripgrep-style match and context lines into per-file snippets.
use std::collections::HashMap;

use crate::types::{RipgrepFile, RipgrepMatch};

// ── intermediate state ────────────────────────────────────────────────────────

pub(crate) struct RawMatch {
    pub(crate) line_text: String,
    pub(crate) line_number: u32,
    pub(crate) column: u32,
}

pub(crate) struct FileEntry {
    pub(crate) raw_matches: Vec<RawMatch>,
    /// line_number → context text
    pub(crate) contexts: HashMap<u32, String>,
}

impl FileEntry {
    pub(crate) fn new() -> Self {
        Self {
            raw_matches: Vec::new(),
            contexts: HashMap::new(),
        }
    }
}

// ── core parsing logic ────────────────────────────────────────────────────────

/// Strips a single trailing `\r\n` or `\n` from `s` in place, matching
/// ripgrep's included newline in the `lines.text` field. Takes ownership so
/// callers moving an owned `String` truncate in place — no re-allocation,
/// vs. the borrow + `.to_owned()` it replaced (which copied into a fresh
/// buffer and dropped the original `String`'s heap allocation).
pub(crate) fn strip_trailing_newline(mut s: String) -> String {
    if s.ends_with('\n') {
        s.pop();
        if s.ends_with('\r') {
            s.pop();
        }
    }
    s
}

/// Chars kept after the match start when a snippet must be clipped.
const MATCH_TAIL_CHARS: usize = 20;

/// Char index for a UTF-16 `column` within `line` (clamped to the line).
fn utf16_to_char_index(line: &str, column: u32) -> usize {
    let mut units = 0usize;
    for (index, ch) in line.chars().enumerate() {
        if units >= column as usize {
            return index;
        }
        units += ch.len_utf16();
    }
    line.chars().count()
}

/// [`relevance::line_rank`] of a match at UTF-16 `column` of `line`.
fn line_rank_at(line: &str, column: u32) -> u32 {
    let byte = line
        .char_indices()
        .nth(utf16_to_char_index(line, column))
        .map_or(line.len(), |(byte, _)| byte);
    super::relevance::line_rank(line.as_bytes(), byte)
}

/// Whether [`clip_around_match`] cuts a window around the match rather
/// than keeping the line's head.
fn clips_to_window(line: &str, column: u32, max_chars: usize) -> bool {
    max_chars > 3
        && utf16_to_char_index(line, column) + MATCH_TAIL_CHARS.min(max_chars / 2) >= max_chars
}

/// Clip `line` to `max_chars`, keeping the match at UTF-16 `column` visible.
/// Lines whose match already fits the head are truncated from the start; otherwise
/// the window starts a quarter-snippet before the match and is marked with `…`.
fn clip_around_match(line: &str, column: u32, max_chars: usize) -> String {
    if !clips_to_window(line, column, max_chars) {
        return truncate_unicode(line, max_chars);
    }
    let match_char = utf16_to_char_index(line, column);
    let start_char = match_char.saturating_sub(max_chars / 4);
    let start_byte = line
        .char_indices()
        .nth(start_char)
        .map_or(line.len(), |(byte, _)| byte);
    format!("…{}", truncate_unicode(&line[start_byte..], max_chars - 1))
}

/// Truncates a string to at most `max_chars` Unicode scalar values, appending
/// `...` when truncated.
pub(crate) fn truncate_unicode(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if max_chars <= 3 {
        let mut chars = s.chars();
        if chars.by_ref().take(max_chars).count() < max_chars || chars.next().is_none() {
            return s.to_owned();
        }
        return ".".repeat(max_chars);
    }

    let head_chars = max_chars - 3;
    let mut head_byte_end = 0;
    for (char_idx, (byte_idx, _ch)) in s.char_indices().enumerate() {
        if char_idx == head_chars {
            head_byte_end = byte_idx;
        }
        if char_idx == max_chars {
            return format!("{}...", &s[..head_byte_end]);
        }
    }
    s.to_owned()
}

/// Assembles a single file's matches into the final `RipgrepFile`, joining each
/// match line with its surrounding `context_lines` and truncating the resulting
/// snippet to `max_snippet` chars.
pub(crate) fn assemble_file(
    path: String,
    entry: FileEntry,
    context_lines: u32,
    max_snippet: usize,
) -> RipgrepFile {
    if context_lines == 0 {
        // One row per hit line: a line that needs no clip becomes the value
        // itself instead of a copy.
        let matches = entry
            .raw_matches
            .into_iter()
            .map(|m| {
                let chars = m.line_text.chars().count();
                let rank = line_rank_at(&m.line_text, m.column);
                let value = if chars <= max_snippet
                    && !clips_to_window(&m.line_text, m.column, max_snippet)
                {
                    m.line_text
                } else {
                    clip_around_match(&m.line_text, m.column, max_snippet)
                };
                RipgrepMatch {
                    line: m.line_number,
                    column: m.column,
                    value,
                    count: None,
                    kind: None,
                    score_hint: None,
                    rank: Some(rank),
                    original_chars: (chars > max_snippet)
                        .then(|| u32::try_from(chars).unwrap_or(u32::MAX)),
                }
            })
            .collect::<Vec<_>>();
        return RipgrepFile {
            path,
            match_count: matches.len() as u32,
            matches,
            source: None,
        };
    }
    // Neighbouring match lines are context too (rg -C prints them); without this
    // lookup a snippet silently skipped them and joined non-adjacent lines.
    let match_lines: HashMap<u32, &str> = entry
        .raw_matches
        .iter()
        .map(|m| (m.line_number, m.line_text.as_str()))
        .collect();
    let neighbour = |line: u32| {
        entry
            .contexts
            .get(&line)
            .map(String::as_str)
            .or_else(|| match_lines.get(&line).copied())
    };
    let matches: Vec<RipgrepMatch> = entry
        .raw_matches
        .iter()
        .map(|m| {
            let (value, original_chars) = {
                // Contiguous context only: stop at the first line that is absent.
                // Join by line slot (not by buffer emptiness) so a blank
                // leading line keeps its place and line numbers stay aligned.
                let mut before: Vec<&str> = (1..=context_lines)
                    .map_while(|i| m.line_number.checked_sub(i).and_then(neighbour))
                    .collect();
                before.reverse();
                let after: Vec<&str> = (1..=context_lines)
                    .map_while(|i| m.line_number.checked_add(i).and_then(neighbour))
                    .collect();
                let join = |lead: &[&str]| {
                    let mut window = lead.to_vec();
                    window.push(&m.line_text);
                    window.extend(&after);
                    window.join("\n")
                };
                let joined = join(&before);
                let chars = joined.chars().count();
                let value = if chars <= max_snippet {
                    joined
                } else {
                    // Drop leading context (farthest first) until the match
                    // line fits, then spend the rest on trailing context.
                    let column = utf16_to_char_index(&m.line_text, m.column);
                    let budget = max_snippet.saturating_sub(MATCH_TAIL_CHARS);
                    let mut lead = &before[..];
                    let prefix = |lead: &[&str]| {
                        lead.iter()
                            .map(|line| line.chars().count() + 1)
                            .sum::<usize>()
                    };
                    while !lead.is_empty() && prefix(lead) + column >= budget {
                        lead = &lead[1..];
                    }
                    if prefix(lead) + column < budget {
                        truncate_unicode(&join(lead), max_snippet)
                    } else {
                        // Even the match line alone would push the match out.
                        clip_around_match(&m.line_text, m.column, max_snippet)
                    }
                };
                (
                    value,
                    (chars > max_snippet).then(|| u32::try_from(chars).unwrap_or(u32::MAX)),
                )
            };

            RipgrepMatch {
                line: m.line_number,
                column: m.column,
                value,
                count: None,
                kind: None,
                score_hint: None,
                rank: Some(line_rank_at(&m.line_text, m.column)),
                original_chars,
            }
        })
        .collect();

    let match_count = matches.len() as u32;
    RipgrepFile {
        path,
        match_count,
        matches,
        source: None,
    }
}

// ── unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Assembles `f.ts` from `(line_number, text, Some(column))` matches and
    /// `(line_number, text, None)` context lines.
    fn assemble(
        lines: &[(u32, &str, Option<u32>)],
        context_lines: u32,
        max_snippet: usize,
    ) -> RipgrepFile {
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
        assemble_file("f.ts".to_owned(), entry, context_lines, max_snippet)
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
    /// equals the clip of the line, including a line that fits but whose
    /// match sits in its tail (that one is clipped to a window).
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

    #[test]
    fn untruncated_content_snippet_has_no_original_char_length() {
        let f = assemble(&[(1, "short line", Some(0))], 0, 500);
        assert_eq!(f.matches[0].original_chars, None);
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

    #[test]
    fn truncate_unicode_tiny_limits_never_exceed_limit() {
        assert_eq!(truncate_unicode("hello", 1), ".");
        assert_eq!(truncate_unicode("hello", 2), "..");
        assert_eq!(truncate_unicode("hello", 3), "...");
    }

    #[test]
    fn truncate_unicode_tiny_limits_preserve_short_input() {
        assert_eq!(truncate_unicode("é", 1), "é");
        assert_eq!(truncate_unicode("é", 2), "é");
    }
}
