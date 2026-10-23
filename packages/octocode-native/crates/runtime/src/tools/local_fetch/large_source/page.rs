//! A streamed window's page mapped back to the whole file: positions,
//! totals, digest, out-of-range pages and continuations.

use super::window::WindowSpan;
use super::{Streamed, WINDOW_LINES, line_chunk_query};
use crate::num::usize_of;
use crate::tools::source_view::*;

/// An offset past the end of the read's `read_total` `unit`s (lines, or the
/// bytes of its lines): nothing returned, in the shape an in-memory read
/// gives the same offset ([`past_end`]), with the file's
/// own facts.
pub(super) fn past_end_row(
    q: &LocalFetchQuery,
    streamed: &Streamed,
    unit: WindowUnit,
    read_total: usize,
    len: u64,
) -> LocalFetchResult {
    let next = past_end(q);
    let offset = local_fetch_query::offset(q).unwrap_or(0);
    let (total_lines, total_bytes) = match unit {
        WindowUnit::Lines => (read_total, usize_of(len)),
        WindowUnit::Bytes => (streamed.last + 1 - streamed.first, read_total),
    };
    LocalFetchResult {
        path: q.path.to_string(),
        source_sha256: Some(streamed.digest.clone()),
        content: Some(String::new()),
        total_lines: Some(streamed.total_lines),
        source_bytes: usize::try_from(len).ok(),
        returned_chars: Some(0),
        returned_bytes: Some(0),
        returned_lines: Some(0),
        pagination: Some(Pagination {
            unit,
            offset,
            length: 0,
            window: local_fetch_query::window_length(q).unwrap_or(WINDOW_LINES),
            total_lines,
            total_bytes,
            has_more: false,
            next_offset: None,
            resume_line: None,
        }),
        out_of_range: true,
        next: Some(next),
        ..LocalFetchResult::blank(ReadStatus::Success)
    }
}

/// Map a window's positions, totals and digest back to the whole file.
pub(super) fn map_to_file(result: &mut LocalFetchResult, streamed: &Streamed, len: u64) {
    let shift = streamed.first - 1;
    for range in &mut result.source_line_ranges {
        range.start += shift;
        range.end += shift;
    }
    result.start_line = result.source_line_ranges.first().map(|range| range.start);
    result.end_line = result.source_line_ranges.last().map(|range| range.end);
    let total = streamed.total_lines;
    result.total_lines = Some(total);
    result.source_bytes = usize::try_from(len).ok();
    result.source_chars = None;
    result.source_sha256 = Some(streamed.digest.clone());
    if streamed.key_lines > 0 {
        result.warnings.push(format!(
            "redactedContent: {} line(s) inside private-key blocks were replaced; this content is not verbatim source.",
            streamed.key_lines
        ));
    }
}

/// The read's next page. A line page continues the read itself at
/// `next_offset` (counted in the read's own lines: the file's for a
/// `chunked` read, the range's otherwise), so the next window starts where
/// the page ended and the offset never resets at a window boundary. A byte
/// page continues in the bytes of the lines its window was cut from (its
/// offset counts from their start, as an in-memory read's does).
pub(super) fn continue_window(
    q: &LocalFetchQuery,
    result: &mut LocalFetchResult,
    streamed: &Streamed,
    span: WindowSpan,
    next_offset: Option<usize>,
) {
    let chunked = span.chunked;
    let snapshot = streamed.digest.parse().ok();
    let inner = result
        .next
        .as_mut()
        .and_then(|next| next.continue_window.as_mut());
    if let Some(inner) = inner
        && inner.query.unit == Some(WindowUnit::Bytes)
    {
        inner.query.path = q.path.clone();
        local_fetch_query::set_line_span(
            &mut inner.query,
            span.first,
            span.last.min(streamed.total_lines),
        );
        inner.query.snapshot = snapshot;
        return;
    }
    let read = next_offset.map(|offset| {
        let mut query = if chunked {
            line_chunk_query(q, offset)
        } else {
            let mut query = q.clone();
            if query.unit == Some(WindowUnit::Bytes) {
                // The long line is done: line pages resume after it.
                query.unit = None;
                query.length = None;
            }
            query.offset = Some(wire_count(offset));
            query
        };
        query.snapshot = snapshot;
        FetchCall { query }
    });
    match (result.next.as_mut(), read) {
        (Some(next), read) => next.continue_window = read,
        (None, Some(read)) => {
            result.next = Some(NextCalls {
                continue_window: Some(read),
                ..NextCalls::default()
            });
        }
        (None, None) => {}
    }
}

/// The first and last file lines a page served.
pub(super) fn served_span(result: &LocalFetchResult) -> Option<(usize, usize)> {
    result
        .source_line_ranges
        .first()
        .map(|range| range.start)
        .zip(result.source_line_ranges.last().map(|range| range.end))
        .or(result.start_line.zip(result.end_line))
}
