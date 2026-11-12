//! A matched line too long for a row ([`RowWindow`]): one row per window of
//! it around its hits, each stating the byte it starts at, in place of the
//! row of its whole text. The windows are the source's own text; what each
//! shows is judged with its source like every other row.
use super::long_line::line_of;
use super::{CollectSink, SpanColumns};
use crate::search::relevance;
use crate::search::text_parser::RawMatch;
use crate::types::{RowWindow, TextSearchMatch};
use grep_matcher::Matcher;

/// One window of a line: its bytes in the line's text, and the raw byte of
/// the line its first hit starts at.
struct Window {
    text: std::ops::Range<usize>,
    first_hit: usize,
}

impl<M: Matcher> CollectSink<'_, M> {
    /// The window rows of `bytes`, one matched line at `line`: `false`, with
    /// no row kept, when the search shows its lines whole, the line is no
    /// longer than a row, it holds no enumerable hit, or its windows join
    /// into the whole line.
    pub(super) fn window_rows(&mut self, bytes: &[u8], line: u32) -> std::io::Result<bool> {
        let Some(RowWindow {
            longer_than,
            margin,
        }) = self.work.row_window
        else {
            return Ok(false);
        };
        let decoded = line_of(bytes);
        let text = decoded.text.as_ref();
        // One line only: a block of several (multiline mode) is shown whole.
        if text.len() <= longer_than || text.contains('\n') {
            return Ok(false);
        }
        let mut columns = SpanColumns::new(bytes, text);
        let mut windows: Vec<Window> = Vec::new();
        self.matcher
            .find_iter(bytes, |found| {
                if super::past_block_end(bytes, found) {
                    return true;
                }
                let start = columns.lossy(found.start()).min(text.len());
                let end = columns.lossy(found.end()).min(text.len());
                let from = text.floor_char_boundary(start.saturating_sub(margin));
                let to = text.ceil_char_boundary(end.saturating_add(margin).min(text.len()));
                match windows.last_mut() {
                    Some(before) if from <= before.text.end => {
                        before.text.end = before.text.end.max(to);
                    }
                    _ => windows.push(Window {
                        text: from..to,
                        first_hit: found.start(),
                    }),
                }
                true
            })
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        let whole = matches!(windows.as_slice(), [one] if one.text == (0..text.len()));
        if windows.is_empty() || whole {
            return Ok(false);
        }
        self.not_utf8 |= decoded.warning.is_some();
        for window in windows {
            let hit = text.floor_char_boundary(columns.lossy(window.first_hit).min(text.len()));
            let (_, column, byte_offset) = columns.position(hit);
            let rank = relevance::line_rank(bytes, window.first_hit);
            let window_start = Some(crate::num::saturating_u32(window.text.start));
            // Both ends sit on char boundaries (floor/ceil above).
            let value = text.get(window.text).unwrap_or_default().to_owned();
            if self.work.deferred_line {
                self.entry.prepared.push(TextSearchMatch {
                    deferred: None,
                    line,
                    first_line: line,
                    last_line: None,
                    column,
                    byte_offset,
                    value_digest: TextSearchMatch::digest_of(&value),
                    value,
                    count: None,
                    rank: Some(rank),
                    window_start,
                });
            } else {
                self.entry.raw_matches.push(RawMatch {
                    line_text: value,
                    line_number: line,
                    column,
                    byte_offset,
                    rank,
                    window_start,
                });
            }
        }
        Ok(true)
    }
}
