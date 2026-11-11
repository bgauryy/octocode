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
    /// The hit's byte in its source line (not in `line_text`, which may be
    /// only a window of a long line).
    pub(crate) byte_offset: u32,
    /// [`relevance::line_rank`](super::relevance::line_rank) of the hit,
    /// taken on its whole source line.
    pub(crate) rank: u32,
    /// [`TextSearchMatch::window_start`]: `line_text` is a window of its
    /// line, and joins no context.
    pub(crate) window_start: Option<u32>,
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
            .map(|m| TextSearchMatch {
                deferred: None,
                line: m.line_number,
                first_line: m.line_number,
                last_line: last_line(&m.line_text, m.line_number),
                column: m.column,
                byte_offset: m.byte_offset,
                value_digest: TextSearchMatch::digest_of(&m.line_text),
                value: m.line_text,
                count: None,
                rank: Some(m.rank),
                window_start: m.window_start,
            })
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
    // A window is not its line: a long matched line is no neighbour's context.
    let match_lines: HashMap<u32, &str> = entry
        .raw_matches
        .iter()
        .filter(|m| m.window_start.is_none())
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
            // A window of a long line is its row: the lines around it are
            // context of the line, not of the window.
            let context = if m.window_start.is_some() {
                SearchContext::default()
            } else {
                context
            };
            // Stop at an absent context line; blank lines retain their slots.
            let mut before: Vec<&str> = (1..=context.before)
                .map_while(|i| m.line_number.checked_sub(i).and_then(neighbour))
                .collect();
            before.reverse();
            // The line the value starts at: as many lines before its match as
            // were joined, which stops at an absent neighbour.
            let first_line = m.line_number - saturating_u32(before.len());
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
                first_line,
                last_line,
                column: m.column,
                byte_offset: m.byte_offset,
                value,
                value_digest,
                count: None,
                rank: Some(m.rank),
                window_start: m.window_start,
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

/// Recreate the exact context-free snippet, rank and full-value identity of
/// the whole line `line_text`, whose hit starts at byte `byte_offset` (UTF-16
/// `column`): the sink knows both, so neither is scanned for again.
pub fn line_match(
    line_text: &str,
    line_number: u32,
    column: u32,
    byte_offset: u32,
) -> TextSearchMatch {
    let rank = super::relevance::line_rank(line_text.as_bytes(), byte_offset as usize);
    let value_digest = TextSearchMatch::digest_of(line_text);
    TextSearchMatch {
        deferred: None,
        line: line_number,
        first_line: line_number,
        last_line: last_line(line_text, line_number),
        column,
        byte_offset,
        value: line_text.to_owned(),
        value_digest,
        count: None,
        rank: Some(rank),
        window_start: None,
    }
}

/// The last line of a match whose text spans several lines.
fn last_line(match_text: &str, line_number: u32) -> Option<u32> {
    let breaks = saturating_u32(match_text.bytes().filter(|&byte| byte == b'\n').count());
    (breaks > 0).then(|| line_number.saturating_add(breaks))
}

#[cfg(test)]
mod tests;
