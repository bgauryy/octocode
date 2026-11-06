//! The one window cutter: every tool that renders a line hands its sealed
//! (redacted, then reduced) text here, and only here is it cut into windows
//! that fit a page. A window is measured as it serializes in a JSON string
//! (escapes included) in UTF-16 units, the unit of `responseChars`; it ends
//! after the last line end that fits, and inside a line only when that line
//! alone outgrows the room. A cut never falls inside a redaction marker or a
//! character, so a secret is never shown or split by paging.
//!
//! The text is always whole in memory: the row pager's `rowPart.text`
//! strings, a localFetch view or byte page ([`window_end`], with cursors
//! checked by [`is_clean_cut`]), a ghGetHistoryItem body ([`clean_cut`]) and
//! an lspSearch declaration window ([`window_at`]). A streamed localFetch
//! source is sealed window by window before it gets here, so its byte
//! offsets count the sealed view as every other read's do.

use octocode_engine::security::{CONTENT_REDACTION_PREFIX, REDACTION_PREFIX};
use std::ops::Range;

/// UTF-16 units `ch` takes inside a serialized JSON string.
pub(crate) fn escaped_units(ch: char) -> usize {
    match ch {
        '"' | '\\' | '\n' | '\r' | '\t' | '\u{08}' | '\u{0c}' => 2,
        ch if (ch as u32) < 0x20 => 6,
        ch => ch.len_utf16(),
    }
}

/// UTF-16 units `text` takes inside a serialized JSON string (no quotes).
pub(crate) fn escaped_len(text: &str) -> usize {
    text.chars().map(escaped_units).sum()
}

/// Where a redaction marker stands relative to a cut.
enum Marker {
    /// No marker covers the cut.
    None,
    /// The cut falls inside the marker at this byte range.
    Around(Range<usize>),
}

/// The redaction marker covering byte `at` of `text`, looking no further back
/// than `start` (an earlier cut never fell inside one). A marker is
/// `[REDACTED…]` or `[CONTENT-REDACTED…]` on one line, so the scan stops at
/// the line, never past it.
fn marker_around(text: &str, start: usize, at: usize) -> Marker {
    // The caller's cuts are character boundaries; marker offsets below
    // come from matching single ASCII delimiters within those cuts.
    let (before, after) = text.split_at(at);
    let head = before.split_at(start).1;
    let Some(open) = head.rfind(['[', ']', '\n']) else {
        return Marker::None;
    };
    if head.as_bytes()[open] != b'[' {
        return Marker::None;
    }
    let open = start + open;
    let rest = text.split_at(open).1;
    if !(rest.starts_with(REDACTION_PREFIX) || rest.starts_with(CONTENT_REDACTION_PREFIX)) {
        return Marker::None;
    }
    match after.find(['[', ']', '\n']) {
        Some(close) if text.as_bytes()[at + close] == b']' => Marker::Around(open..at + close + 1),
        _ => Marker::None,
    }
}

/// Whether `at` separates the two ASCII bytes of one CRLF line ending.
fn splits_crlf(text: &str, at: usize) -> bool {
    at.checked_sub(1)
        .and_then(|before| text.as_bytes().get(before..=at))
        .is_some_and(|pair| pair == b"\r\n")
}

/// Where a cut at byte `at` of `text` really falls, whatever unit the caller
/// counts in: `at` moved back to its character's first byte, before the
/// `\r\n` pair it would split, and to the start of the redaction marker it
/// would fall inside. It depends on `at` and the text alone, so one window's
/// end and the next one's start, cut at the same nominal edge, are the same
/// byte.
#[must_use]
pub(crate) fn clean_cut(text: &str, at: usize) -> usize {
    let mut at = text.floor_char_boundary(at.min(text.len()));
    if splits_crlf(text, at) {
        at -= 1;
    }
    match marker_around(text, 0, at) {
        Marker::Around(marker) => marker.start,
        Marker::None => at,
    }
}

/// Whether byte `at` is a cut [`clean_cut`] leaves where it is: a cursor
/// naming any other byte was not emitted by a window of `text`.
#[must_use]
pub(crate) fn is_clean_cut(text: &str, at: usize) -> bool {
    at <= text.len() && clean_cut(text, at) == at
}

/// The end byte of the window of `text` that starts at byte `start` and fits
/// `room` escaped UTF-16 units: after the last `\n` that fits, or, when the
/// line at `start` alone outgrows the room, at the last character that fits.
/// A cut that would fall inside a redaction marker moves before it, or past
/// it when the marker starts the window (an indivisible marker longer than
/// the room stays whole), so every window holds at least one character or
/// one whole marker and a walk always advances.
#[must_use]
pub(crate) fn window_end(text: &str, start: usize, room: usize) -> usize {
    let mut used = 0;
    let mut end = start;
    let mut line_end = None;
    // `start` is zero or the previous window's character boundary.
    for (offset, ch) in text.split_at(start).1.char_indices() {
        let units = escaped_units(ch);
        if used + units > room && end > start {
            break;
        }
        used += units;
        end = start + offset + ch.len_utf8();
        if ch == '\n' {
            line_end = Some(end);
        }
        if used >= room {
            break;
        }
    }
    if end >= text.len() {
        return text.len();
    }
    if let Some(line_end) = line_end {
        return line_end;
    }
    // Never between `\r` and its `\n`: the pair is one line end.
    if splits_crlf(text, end) {
        if end - 1 > start {
            end -= 1;
        } else {
            end += 1;
        }
    }
    match marker_around(text, start, end) {
        Marker::Around(marker) if marker.start > start => marker.start,
        Marker::Around(marker) => marker.end,
        Marker::None => end,
    }
}

/// The window of `text` that starts at byte `at` (a declaration inside a
/// line too long for one page) and fits `room` escaped UTF-16 units: it starts at
/// [`clean_cut`] of `at` (at the hit, or just before it when `at` falls
/// inside a character, a CRLF pair or a redaction marker) and ends at
/// [`window_end`]. Both edges are clean cuts, so a byte cursor naming either
/// is accepted by a reader that refuses any other (`is_clean_cut`), and a
/// walk on from the end reaches the rest of `text` exactly once.
#[must_use]
pub(crate) fn window_at(text: &str, at: usize, room: usize) -> Range<usize> {
    let start = clean_cut(text, at);
    start..window_end(text, start, room)
}

#[cfg(test)]
mod tests;
