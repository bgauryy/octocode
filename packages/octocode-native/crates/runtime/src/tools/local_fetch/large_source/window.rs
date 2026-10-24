//! One streamed window of a read: its span, the pass that holds it, and the
//! page served from it.

use super::held::Want;
use super::index::read_windows;
use super::page::{continue_window, map_to_file, past_end_row, served_span};
use super::{BYTE_LOOKBEHIND, Streamed, WINDOW_LINES};
use crate::execution::cancel::CancellationCheck;
use crate::num::usize_of;
use crate::security::scan::ContentScan;
use crate::tools::source_view::prepare::extraction_error;
use crate::tools::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadProgressUnit};
use std::path::Path;

/// The file lines one streamed window of `q` holds.
#[derive(Clone, Copy)]
pub(super) struct WindowSpan {
    pub(super) first: usize,
    /// The window's last line: the request's, capped at [`WINDOW_LINES`]
    /// unless the window is a byte page, which pages the range's bytes.
    pub(super) last: usize,
    /// The request's last line.
    pub(super) requested_last: usize,
    /// The line a line `offset` of the read counts from: the range's first
    /// line, or line 1 for a chunk.
    pub(super) origin: usize,
    /// A line chunk (offset + length, no `ranges`) rather than a range.
    pub(super) chunked: bool,
    /// The range's bytes the window skips: a byte page's offset less its
    /// lookbehind ([`BYTE_LOOKBEHIND`]).
    pub(super) from: usize,
    /// A byte page of a range: it reads at its byte offset, however many
    /// lines (or bytes of one line) come before it.
    pub(super) byte_page: bool,
}

impl WindowSpan {
    pub(super) fn want(&self) -> Want {
        Want {
            first: self.first,
            last: self.last,
            from: self.from,
            byte_page: self.byte_page,
        }
    }
}

pub(super) fn window_span(q: &LocalFetchQuery) -> WindowSpan {
    let (first, requested_last, chunked) = match (
        local_fetch_query::start_line(q),
        local_fetch_query::end_line(q),
    ) {
        // A range pages in its own lines: a line offset starts the window
        // there, so every window holds a full page past the offset (a byte
        // offset pages inside the range's first line instead).
        (Some(start), end) => {
            let skip = match local_fetch_query::unit(q) {
                ReadProgressUnit::Bytes => 0,
                _ => local_fetch_query::offset(q).unwrap_or(0),
            };
            (start + skip, end.unwrap_or(start), false)
        }
        _ => {
            let offset = local_fetch_query::offset(q).unwrap_or(0);
            (offset + 1, offset + WINDOW_LINES, true)
        }
    };
    let byte_page = !chunked && local_fetch_query::unit(q) == ReadProgressUnit::Bytes;
    WindowSpan {
        first,
        last: if byte_page {
            requested_last
        } else {
            requested_last.min(first + WINDOW_LINES - 1)
        },
        from: if byte_page {
            local_fetch_query::offset(q)
                .unwrap_or(0)
                .saturating_sub(BYTE_LOOKBEHIND)
        } else {
            0
        },
        requested_last,
        origin: if chunked {
            1
        } else {
            local_fetch_query::start_line(q).unwrap_or(first)
        },
        chunked,
        byte_page,
    }
}

/// The streamed source a read opened: its handle, canonical path, settled
/// stamp (see [`read_windows`]) and length.
pub(super) struct Opened<'a> {
    pub(super) file: &'a std::fs::File,
    pub(super) path: &'a Path,
    pub(super) stamp: Option<octocode_engine::stamp::SourceStamp>,
    pub(super) len: u64,
}

impl Opened<'_> {
    pub(super) fn windows(
        &self,
        wants: &[Want],
        cancel: &impl CancellationCheck,
    ) -> Result<Vec<Streamed>, String> {
        read_windows(self.file, self.path, self.stamp, wants, cancel)
    }
}

/// One streamed line window (a range or a chunk).
pub(super) fn stream_window(
    q: &LocalFetchQuery,
    source: &Opened<'_>,
    modified: Option<String>,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let span = window_span(q);
    // One span streams exactly one window.
    let streamed = source
        .windows(&[span.want()], cancel)
        .and_then(|mut windows| windows.pop().ok_or_else(|| "no window streamed".to_owned()));
    match streamed {
        Ok(streamed) => serve_window(q, streamed, span, source, modified, share, security, cancel),
        Err(error) => LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::FileReadFailed,
            error,
        ),
    }
}

/// The page of one streamed window, mapped back to the whole file.
pub(super) fn serve_window(
    q: &LocalFetchQuery,
    streamed: Streamed,
    span: WindowSpan,
    source: &Opened<'_>,
    modified: Option<String>,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let (path, len) = (source.path, source.len);
    let WindowSpan {
        first,
        requested_last,
        origin,
        chunked,
        byte_page,
        ..
    } = span;
    if let Some(expected) = local_fetch_query::snapshot(q)
        && expected != streamed.digest
    {
        return stale_snapshot(q);
    }
    if streamed.binary {
        return super::super::acquire::refuse_binary(q, local_fetch_query::path(q));
    }
    let total = streamed.total_lines;
    if !chunked && origin > total {
        // A range starting past the file end, as an in-memory read refuses it.
        return extraction_error(
            q,
            ExtractionError::new(
                ErrorCode::InvalidInput,
                format!("Requested lines start past the file end ({total} lines)"),
            ),
            total,
        );
    }
    // A line offset past the read's last line (the range's, or the file's
    // for a chunk) is the in-memory read's out-of-range page.
    let read_total = if chunked {
        total
    } else {
        requested_last.min(total) + 1 - origin
    };
    if (!chunked && first > requested_last.min(total))
        || (chunked && (first > total.max(1) || (total == 0 && first > 1)))
    {
        return past_end_row(q, &streamed, ReadProgressUnit::Lines, read_total, len);
    }
    // A byte offset past the range's last byte, which no window holds.
    let offset = local_fetch_query::offset(q).unwrap_or(0);
    if byte_page && offset > 0 && offset >= streamed.origin.total {
        return past_end_row(
            q,
            &streamed,
            ReadProgressUnit::Bytes,
            streamed.origin.total,
            len,
        );
    }
    let held_last = streamed.last.max(streamed.first);
    let progress = if !chunked && local_fetch_query::unit(q) == ReadProgressUnit::Bytes {
        local_fetch_query::read_progress(q).cloned()
    } else {
        None
    };
    let window_query = match local_fetch_query::updated(
        q,
        serde_json::json!({"classification":null,"selection":null,"snapshot":null,"ranges":[{"startLine":1,"endLine":held_last+1-streamed.first}],"readProgress":progress}),
    ) {
        Ok(query) => query,
        Err(error) => {
            return LocalFetchResult::error(
                local_fetch_query::path(q).to_owned(),
                ErrorCode::ExecutionFailed,
                error.to_string(),
            );
        }
    };
    let serve = |window_query: &LocalFetchQuery| {
        let mut result = process_fetched_content(
            window_query,
            &streamed.window,
            path,
            &SourceFacts {
                modified: modified.clone(),
                window: share,
                beside: 0,
                interior: streamed.first > 1 || streamed.origin.base > 0,
                byte_origin: Some(streamed.origin),
            },
            security,
            cancel,
        );
        if result.status == ReadStatus::Error {
            return result;
        }
        map_to_file(&mut result, &streamed, len);
        if !chunked && requested_last > total {
            // The in-memory read states the same clamp.
            result.warnings.push(format!(
                "Range {first}-{requested_last} adjusted to end at {total} (file end)."
            ));
        }
        // The read's last line: the file's for a chunk, the range's otherwise.
        let last = if chunked {
            total
        } else {
            requested_last.min(total)
        };
        // The lines the page returned, which the window's page budget may have
        // cut short of the window it held.
        let served = served_span(&result).unwrap_or((streamed.first, held_last));
        if let Err(error) = continue_window(
            q,
            &mut result,
            &streamed,
            span,
            (served.1 < last).then_some(served.1 + 1 - origin),
        ) {
            return LocalFetchResult::error(
                local_fetch_query::path(q).to_owned(),
                ErrorCode::ExecutionFailed,
                error,
            );
        }
        // A line page pages the read's own lines (the file's for a chunk, the
        // range's otherwise) whatever window held it, as an in-memory read's
        // pagination does.
        if let Some(page) = result.pagination.as_mut()
            && page.unit == ReadProgressUnit::Lines
        {
            let (start, end) = served;
            page.offset = start - origin;
            page.length = end + 1 - start;
            page.total_lines = last + 1 - origin;
            if chunked {
                page.total_bytes = usize_of(len);
            }
            page.has_more = end < last;
            page.next_offset = page.has_more.then_some(end + 1 - origin);
            page.resume_line = None;
        }
        // A byte page pages the bytes of the lines its window was cut from.
        if let Some(page) = result.pagination.as_mut()
            && page.unit == ReadProgressUnit::Bytes
        {
            page.total_lines = span.last.min(total) + 1 - span.first;
        }
        result
    };
    serve(&window_query)
}
