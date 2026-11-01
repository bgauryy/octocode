//! Assembles match and context lines into per-file snippets.
use crate::num::saturating_u32;
use crate::types::SearchContext;
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

/// Assemble each match with every requested contiguous context line.
pub(crate) fn assemble_file(
    path: String,
    entry: FileEntry,
    context: SearchContext,
) -> TextSearchFile {
    if !entry.prepared.is_empty() {
        return TextSearchFile {
            path,
            match_count: saturating_u32(entry.prepared.len()),
            matches: entry.prepared,
            source: None,
            term_counts: Vec::new(),
        };
    }
    if context.is_empty() {
        // One complete value per hit line.
        let matches = entry
            .raw_matches
            .into_iter()
            .map(|m| line_match(&m.line_text, m.line_number, m.column))
            .collect::<Vec<_>>();
        return TextSearchFile {
            path,
            match_count: saturating_u32(matches.len()),
            matches,
            source: None,
            term_counts: Vec::new(),
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
                .map(move |(index, text)| {
                    (m.line_number.saturating_add(saturating_u32(index)), text)
                })
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
            // Stop at an absent context line; blank lines retain their slots.
            let mut before: Vec<&str> = (1..=context.before)
                .map_while(|i| m.line_number.checked_sub(i).and_then(neighbour))
                .collect();
            before.reverse();
            let end = last_line.unwrap_or(m.line_number);
            let after = (1..=context.after).map_while(|i| end.checked_add(i).and_then(neighbour));
            let value = before
                .into_iter()
                .chain(std::iter::once(m.line_text.as_str()))
                .chain(after)
                .collect::<Vec<_>>()
                .join("\n");
            let value_digest = TextSearchMatch::digest_of(&value);

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
            }
        })
        .collect();

    let match_count = saturating_u32(matches.len());
    TextSearchFile {
        path,
        match_count,
        matches,
        source: None,
        term_counts: Vec::new(),
    }
}

/// Recreate the exact context-free snippet, rank and full-value identity.
pub fn line_match(line_text: &str, line_number: u32, column: u32) -> TextSearchMatch {
    let rank = line_rank_at(line_text, column);
    let byte_offset = utf16_to_byte_offset(line_text, column);
    let value_digest = TextSearchMatch::digest_of(line_text);
    TextSearchMatch {
        deferred: None,
        line: line_number,
        last_line: last_line(line_text, line_number),
        column,
        byte_offset,
        value: line_text.to_owned(),
        value_digest,
        count: None,
        rank: Some(rank),
    }
}

/// The last line of a match whose text spans several lines.
fn last_line(match_text: &str, line_number: u32) -> Option<u32> {
    let breaks = saturating_u32(match_text.bytes().filter(|&byte| byte == b'\n').count());
    (breaks > 0).then(|| line_number.saturating_add(breaks))
}

/// Byte offset of UTF-16 `column` within `line` (clamped to the line): where
/// a byte-window read of the line starts at the match.
fn utf16_to_byte_offset(line: &str, column: u32) -> u32 {
    let mut units = 0usize;
    for (offset, ch) in line.char_indices() {
        if units >= column as usize {
            return saturating_u32(offset);
        }
        units += ch.len_utf16();
    }
    saturating_u32(line.len())
}

#[cfg(test)]
mod tests;
