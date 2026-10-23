//! Assembles match and context lines into per-file snippets.
use crate::types::SearchContext;
use sha2::{Digest as _, Sha256};
use std::collections::HashMap;

use crate::types::{TextSearchFile, TextSearchMatch};

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
    pub(crate) prepared: Vec<TextSearchMatch>,
}

impl FileEntry {
    pub(crate) fn new() -> Self {
        Self {
            raw_matches: Vec::new(),
            contexts: HashMap::new(),
            prepared: Vec::new(),
        }
    }
}

// ── core parsing logic ────────────────────────────────────────────────────────

/// Strips a single trailing `\r\n` or `\n` from `s` in place, matching
/// the searcher's included newline in the `lines.text` field. Takes ownership so
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

/// [`relevance::line_rank`](super::relevance::line_rank) of a match at UTF-16 `column` of `line`.
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
/// A window near the line's end starts earlier instead, so it still spends
/// `max_chars` (the tail it ends on is the line's own, unmarked). A limit too
/// short for a marker shows the line's own chars from the match.
fn clip_around_match(line: &str, column: u32, max_chars: usize) -> String {
    let line_chars = line.chars().count();
    if line_chars <= max_chars {
        return line.to_owned();
    }
    if max_chars <= 3 {
        let start = utf16_to_char_index(line, column).min(line_chars - max_chars);
        return line.chars().skip(start).take(max_chars).collect();
    }
    if !clips_to_window(line, column, max_chars) {
        return truncate_unicode(line, max_chars);
    }
    let match_char = utf16_to_char_index(line, column);
    // `…` takes one char; the rest of the window is the line from its start.
    let start_char = match_char
        .saturating_sub(max_chars / 4)
        .min(line_chars - (max_chars - 1));
    let start_byte = line
        .char_indices()
        .nth(start_char)
        .map_or(line.len(), |(byte, _)| byte);
    format!("…{}", truncate_unicode(&line[start_byte..], max_chars - 1))
}

/// Truncates a string to at most `max_chars` Unicode scalar values, appending
/// `...` when truncated; a limit too short for the marker keeps the leading
/// chars themselves.
pub(crate) fn truncate_unicode(s: &str, max_chars: usize) -> String {
    if max_chars <= 3 {
        return s.chars().take(max_chars).collect();
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

/// Assembles a single file's matches into the final `TextSearchFile`, joining each
/// match line with its surrounding `context` and truncating the resulting
/// snippet to `max_snippet` chars.
pub(crate) fn assemble_file(
    path: String,
    entry: FileEntry,
    context: SearchContext,
    max_snippet: usize,
) -> TextSearchFile {
    if !entry.prepared.is_empty() {
        return TextSearchFile {
            path,
            match_count: entry.prepared.len() as u32,
            matches: entry.prepared,
            source: None,
        };
    }
    if context.is_empty() {
        // One row per hit line: a line that needs no clip becomes the value
        // itself instead of a copy.
        let matches = entry
            .raw_matches
            .into_iter()
            .map(|m| line_match(&m.line_text, m.line_number, m.column, max_snippet))
            .collect::<Vec<_>>();
        return TextSearchFile {
            path,
            match_count: matches.len() as u32,
            matches,
            source: None,
        };
    }
    // Neighbouring match lines are context too; without this
    // lookup a snippet silently skipped them and joined non-adjacent lines.
    let match_lines: HashMap<u32, &str> = entry
        .raw_matches
        .iter()
        .flat_map(|m| {
            m.line_text
                .split('\n')
                .enumerate()
                .map(move |(index, text)| (m.line_number.saturating_add(index as u32), text))
        })
        .collect();
    let neighbour = |line: u32| {
        entry
            .contexts
            .get(&line)
            .map(String::as_str)
            .or_else(|| match_lines.get(&line).copied())
    };
    let matches: Vec<TextSearchMatch> = entry
        .raw_matches
        .iter()
        .map(|m| {
            let last_line = last_line(&m.line_text, m.line_number);
            let (value, value_digest, original_chars, original_bytes) = {
                // Contiguous context only: stop at the first line that is absent.
                // Join by line slot (not by buffer emptiness) so a blank
                // leading line keeps its place and line numbers stay aligned.
                let mut before: Vec<&str> = (1..=context.before)
                    .map_while(|i| m.line_number.checked_sub(i).and_then(neighbour))
                    .collect();
                before.reverse();
                let end = last_line.unwrap_or(m.line_number);
                let after: Vec<&str> = (1..=context.after)
                    .map_while(|i| end.checked_add(i).and_then(neighbour))
                    .collect();
                let join = |lead: &[&str]| {
                    let mut window = lead.to_vec();
                    window.push(&m.line_text);
                    window.extend(&after);
                    window.join("\n")
                };
                let joined = join(&before);
                let (chars, joined_bytes) = (joined.chars().count(), joined.len());
                let value_digest = Sha256::digest(joined.trim().as_bytes()).into();
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
                let clipped = chars > max_snippet;
                (
                    value,
                    value_digest,
                    clipped.then(|| u32::try_from(chars).unwrap_or(u32::MAX)),
                    clipped.then(|| u32::try_from(joined_bytes).unwrap_or(u32::MAX)),
                )
            };

            TextSearchMatch {
                deferred: None,
                line: m.line_number,
                last_line,
                column: m.column,
                byte_offset: utf16_to_byte_offset(&m.line_text, m.column),
                value,
                value_digest,
                count: None,
                rank: Some(line_rank_at(&m.line_text, m.column)),
                original_chars,
                original_bytes,
            }
        })
        .collect();

    let match_count = matches.len() as u32;
    TextSearchFile {
        path,
        match_count,
        matches,
        source: None,
    }
}

/// Recreate the exact context-free snippet, rank and full-value identity.
pub fn line_match(
    line_text: &str,
    line_number: u32,
    column: u32,
    max_snippet: usize,
) -> TextSearchMatch {
    let chars = line_text.chars().count();
    let (clipped, bytes) = (chars > max_snippet, line_text.len());
    let rank = line_rank_at(line_text, column);
    let byte_offset = utf16_to_byte_offset(line_text, column);
    let value_digest = Sha256::digest(line_text.trim().as_bytes()).into();
    let value = if !clipped {
        line_text.to_owned()
    } else {
        clip_around_match(line_text, column, max_snippet)
    };
    TextSearchMatch {
        deferred: None,
        line: line_number,
        last_line: last_line(line_text, line_number),
        column,
        byte_offset,
        value,
        value_digest,
        count: None,
        rank: Some(rank),
        original_chars: clipped.then(|| u32::try_from(chars).unwrap_or(u32::MAX)),
        original_bytes: clipped.then(|| u32::try_from(bytes).unwrap_or(u32::MAX)),
    }
}

/// The last line of a match whose text spans several lines.
fn last_line(match_text: &str, line_number: u32) -> Option<u32> {
    let breaks = match_text.bytes().filter(|&byte| byte == b'\n').count() as u32;
    (breaks > 0).then(|| line_number.saturating_add(breaks))
}

/// Byte offset of UTF-16 `column` within `line` (clamped to the line): where
/// a byte-window read of the line starts at the match.
fn utf16_to_byte_offset(line: &str, column: u32) -> u32 {
    let mut units = 0usize;
    for (offset, ch) in line.char_indices() {
        if units >= column as usize {
            return offset as u32;
        }
        units += ch.len_utf16();
    }
    line.len() as u32
}

#[cfg(test)]
mod tests;
