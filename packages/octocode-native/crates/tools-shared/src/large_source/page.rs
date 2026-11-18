//! A streamed window's page mapped back to the whole file: positions,
//! totals, digest, out-of-range pages and continuations.

use super::window::WindowSpan;
use super::{Streamed, WINDOW_LINES, line_chunk_query};
use crate::source_view::local_fetch_query;
use crate::source_view::*;
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit};
use octocode_engine::num::saturating_usize;

/// An offset past the end of the read's `read_total` `unit`s (lines, or the
/// bytes of its lines): nothing returned, in the shape an in-memory read
/// gives the same offset ([`past_end`]), with the file's
/// own facts.
pub(super) fn past_end_row(
    q: &LocalFetchQuery,
    streamed: &Streamed,
    unit: ReadProgressUnit,
    read_total: usize,
    len: u64,
) -> LocalFetchResult {
    let next = past_end(q);
    let offset = local_fetch_query::offset(q).unwrap_or(0);
    let (total_lines, total_bytes) = match unit {
        ReadProgressUnit::Lines => (read_total, saturating_usize(len)),
        ReadProgressUnit::Bytes => (streamed.last + 1 - streamed.first, read_total),
    };
    LocalFetchResult {
        path: local_fetch_query::path(q).to_owned(),
        source_sha256: Some(streamed.digest.clone()),
        content: Some(String::new()),
        total_lines: Some(streamed.total_lines),
        returned_bytes: Some(0),
        returned_lines: Some(0),
        pagination: Some(Pagination {
            unit,
            offset,
            length: 0,
            window: WINDOW_LINES,
            total_lines,
            total_bytes,
            has_more: false,
            next_offset: None,
            resume_line: None,
            placed: None,
        }),
        out_of_range: true,
        next: Some(next),
        ..LocalFetchResult::blank(ReadStatus::Success)
    }
}

/// Map a window's positions, totals and digest back to the whole file.
pub(super) fn map_to_file(result: &mut LocalFetchResult, streamed: &Streamed) {
    let shift = streamed.first - 1;
    for range in &mut result.source_line_ranges {
        range.start += shift;
        range.end += shift;
    }
    result.start_line = result.source_line_ranges.first().map(|range| range.start);
    result.end_line = result.source_line_ranges.last().map(|range| range.end);
    let total = streamed.total_lines;
    result.total_lines = Some(total);
    result.source_sha256 = Some(streamed.digest.clone());
    // The note stands on the page that shows the key lines' placeholders,
    // counting those it shows: a page of the window's other lines shows none.
    let placeholder = octocode_runtime_base::security::key_fragment_placeholder();
    let shown = result.content.as_deref().map_or(0, |content| {
        content
            .lines()
            .filter(|line| line.contains(&placeholder))
            .count()
    });
    if streamed.key_lines > 0 && shown > 0 {
        result
            .warnings
            .push(octocode_runtime_base::security::redacted_mark());
    }
}

/// Add a joined part's `warning` to the row's `warnings`, once.
pub(super) fn join_warning(warnings: &mut Vec<String>, warning: String) {
    if !warnings.contains(&warning) {
        warnings.push(warning);
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
) -> Result<(), String> {
    let chunked = span.chunked;
    let snapshot = streamed.digest.as_str();
    let inner = result
        .next
        .as_mut()
        .and_then(|next| next.continue_window.as_mut());
    if let Some(inner) = inner
        && local_fetch_query::unit(&inner.query) == ReadProgressUnit::Bytes
    {
        local_fetch_query::set_path(&mut inner.query, local_fetch_query::path(q))
            .map_err(|error| error.to_string())?;
        // The byte pages count from the window's first line and run to the
        // read's last line (the file's for a chunk, the range's otherwise), not
        // the window's: the line walk they hand back to goes on to that line.
        let read_last = if chunked {
            streamed.total_lines
        } else {
            span.requested_last.min(streamed.total_lines)
        };
        local_fetch_query::set_line_span(&mut inner.query, span.first, read_last)
            .map_err(|error| error.to_string())?;
        local_fetch_query::set_snapshot(&mut inner.query, Some(snapshot))
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let read = next_offset
        .map(|offset| {
            let mut query = if chunked {
                line_chunk_query(q, offset).map_err(|error| error.to_string())?
            } else {
                q.clone()
            };
            local_fetch_query::set_progress(
                &mut query,
                Some(octocode_contract_types::ReadProgress {
                    unit: ReadProgressUnit::Lines,
                    offset: wire_count(offset),
                }),
            );
            local_fetch_query::set_snapshot(&mut query, Some(snapshot))
                .map_err(|error| error.to_string())?;
            Ok::<_, String>(FetchCall::page(query))
        })
        .transpose()?;
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
    Ok(())
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
