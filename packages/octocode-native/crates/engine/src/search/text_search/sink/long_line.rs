//! A matched line past [`LONG_LINE_BYTES`]: a row shows a window of it around its hit,
//! and the line is judged whole for secrets, so only a secret's own bytes are hidden.
//! Its text is borrowed where it is UTF-8, so the line is held once.
use super::CollectSink;
use crate::num::saturating_u32;
use crate::search::relevance;
use crate::security::{LONG_LINE_BYTES, LongLine};
use crate::text::utf8_offsets::byte_to_utf16_offset;
use crate::text::{Decoded, interior_offset};
use crate::types::TextSearchMatch;
use grep_matcher::Matcher;

/// Whether `bytes`, one matched block, is one line past the bound.
pub(super) fn is_long(bytes: &[u8]) -> bool {
    bytes.len() > LONG_LINE_BYTES && !bytes[..bytes.len() - 1].contains(&b'\n')
}

/// `bytes` without its line break, as text: its own bytes when they are UTF-8.
pub(super) fn line_of(bytes: &[u8]) -> Decoded<'_> {
    let line = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    Decoded::interior(line.strip_suffix(b"\r").unwrap_or(line))
}

impl<M: Matcher> CollectSink<'_, M> {
    fn judged(&mut self, text: &str) -> LongLine {
        let path = self.source.map(std::path::Path::to_string_lossy);
        let judged = LongLine::judged(text, path.as_deref());
        self.every_value |= judged.every_value();
        judged
    }

    /// The first hit's row of this long line, in place of the row of its text.
    pub(super) fn long_line_row(
        &mut self,
        bytes: &[u8],
        line: u32,
        first_start: Option<usize>,
    ) -> std::io::Result<()> {
        let failed = |error: M::Error| std::io::Error::other(error.to_string());
        let decoded = line_of(bytes);
        self.not_utf8 |= decoded.warning.is_some();
        let text = decoded.text.as_ref();
        let start = first_start.unwrap_or(0);
        let end = self
            .matcher
            .find_at(bytes, start)
            .map_err(failed)?
            .map_or(start, |found| found.end());
        let hit = text.floor_char_boundary(interior_offset(bytes, start).min(text.len()))
            ..text.ceil_char_boundary(interior_offset(bytes, end).min(text.len()));
        let column = saturating_u32(byte_to_utf16_offset(text, hit.start));
        let byte_offset = saturating_u32(hit.start);
        let value = self.judged(text).window(text, hit);
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
                rank: Some(relevance::line_rank(bytes, start)),
                window_start: None,
            });
        } else {
            self.entry
                .raw_matches
                .push(crate::search::text_parser::RawMatch {
                    line_text: value,
                    line_number: line,
                    column,
                    byte_offset,
                    rank: relevance::line_rank(bytes, start),
                    window_start: None,
                });
        }
        Ok(())
    }

    /// A context line past the bound, as a row shows it: its head.
    pub(super) fn long_line_context(&mut self, bytes: &[u8]) -> String {
        let decoded = line_of(bytes);
        self.not_utf8 |= decoded.warning.is_some();
        let text = decoded.text.as_ref();
        self.judged(text).window(text, 0..0)
    }

    /// One row per submatch of this long line, each with only a secret's own bytes
    /// hidden: how many submatches it holds.
    pub(super) fn collect_long_spans(
        &mut self,
        bytes: &[u8],
        line: u32,
        first_start: &mut Option<usize>,
    ) -> std::io::Result<u32> {
        let decoded = line_of(bytes);
        self.not_utf8 |= decoded.warning.is_some();
        let text = decoded.text.as_ref();
        let judged = self.judged(text);
        let mut count = 0u32;
        let mut columns = super::SpanColumns::new(bytes, text);
        let (om, charge, stop) = (&mut self.om_matches, &mut self.charge, self.stop);
        self.matcher
            .find_iter(bytes, |found| {
                count = count.saturating_add(1);
                first_start.get_or_insert(found.start());
                let start = columns.lossy(found.start()).min(text.len());
                let end = columns.lossy(found.end()).min(text.len());
                let shown = text.floor_char_boundary(start)..text.ceil_char_boundary(end);
                let (_, column, byte_offset) = columns.position(start);
                let value = judged.cut(text, shown);
                let span = TextSearchMatch {
                    deferred: None,
                    line,
                    first_line: line,
                    last_line: None,
                    column,
                    byte_offset,
                    value_digest: TextSearchMatch::digest_of(&value),
                    value,
                    count: None,
                    rank: None,
                    window_start: None,
                };
                super::keep_span((&mut *om, &mut *charge, stop), span)
            })
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        Ok(count)
    }
}
