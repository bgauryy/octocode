//! Bounded line windows over sources above the in-memory ceiling.
//!
//! `localFetch` normally reads the whole source (≤ `MAX_SOURCE_BYTES`) so it
//! can hash it, count lines, and redact private-key blocks across the entire
//! file before cutting a window. A larger source (logs, dumps, generated
//! code) is instead streamed once: the whole-file SHA-256 and line count are
//! computed on the fly, only the requested line window is kept (byte-capped,
//! so even one multi-GB line cannot allocate unbounded), and every window
//! line inside a private-key block is replaced before the window reaches the
//! regular sanitizer, whichever side of the window the block's markers fall
//! on. The window then goes through the normal pipeline, and its positions,
//! totals, digest, and continuations are mapped back to the whole file.

mod held;
mod index;
mod page;
mod ranges;
mod stream;
mod window;

use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use crate::tools::source_view::*;
use octocode_contract_types::ErrorCode;
use ranges::fetch_ranges;
use std::path::Path;
use window::{Opened, stream_window};

/// Largest source a window may be streamed from.
pub(super) const MAX_STREAM_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;
/// Lines held by one streamed window; response pages are sized separately.
const WINDOW_LINES: usize = 5_000;
/// Lines the first bare read previews before continuing through bounded
/// windows (contract `localFetch.semanticPolicy.streamedHeadLines`).
const STREAMED_HEAD_LINES: usize = crate::id::local_fetch_policy::STREAMED_HEAD_LINES;
/// Bytes one streamed window holds; a longer window is cut at a line end or,
/// for a single giant line, mid-line: its byte pages start the window at
/// their own offset, so the rest of the line is the next window.
pub(super) const WINDOW_BYTES: usize = 4 * 1024 * 1024;
/// View bytes a byte page's window keeps behind the page: the page finds its
/// line's start and its secret-scan margin there, as in an in-memory read.
const BYTE_LOOKBEHIND: usize = 1024 * 1024;
/// Line prefix kept for private-key marker detection outside the window.
const MARKER_PREFIX_BYTES: usize = 256;

struct Streamed {
    digest: String,
    total_lines: usize,
    window: Vec<u8>,
    /// First and last (1-based) lines `window` holds bytes of.
    first: usize,
    last: usize,
    key_lines: usize,
    /// Where `window` starts in the requested lines' bytes, and how many
    /// bytes those lines have (key blocks replaced): the byte view a
    /// window of any size is a part of.
    origin: ByteOrigin,
    /// The source holds a NUL byte: binary, whatever its head showed.
    binary: bool,
}

/// A repair for a request a streamed window cannot serve.
fn unsupported(q: &LocalFetchQuery, len: u64, reason: &str, next: NextCalls) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        q.path.to_string(),
        ErrorCode::LargeSourceWindowOnly,
        format!(
            "This source is {}MB, above the {}MB whole-file read limit, so only bounded line windows are served: {reason}",
            len / (1024 * 1024),
            super::acquire::MAX_SOURCE_BYTES / (1024 * 1024)
        ),
    );
    result.source_bytes = usize::try_from(len).ok();
    result.next = Some(next);
    result
}

fn line_chunk_query(q: &LocalFetchQuery, offset: usize) -> LocalFetchQuery {
    let mut query = q.clone();
    query.full_content = None;
    query.minify = None;
    query.match_string = None;
    query.regex = None;
    query.case_mode = None;
    query.context_lines = None;
    local_fetch_query::clear_block_selectors(&mut query);
    query.unit = Some(WindowUnit::Lines);
    query.offset = if offset == 0 {
        None
    } else {
        Some(wire_count(offset))
    };
    query.snapshot = None;
    query
}

/// Serve `q` from a source larger than the whole-file ceiling, streamed
/// from `source`, the read's open handle on `path`.
pub(super) fn fetch_window(
    q: &LocalFetchQuery,
    path: &Path,
    source: &std::fs::File,
    meta: &std::fs::Metadata,
    modified: Option<String>,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let len = meta.len();
    // Taken before any byte is read: a write during the pass changes it.
    let stamp = crate::tools::source::cache_stamp(meta).filter(|_| {
        octocode_engine::stamp::SourceStamp::settled(meta, octocode_engine::stamp::settle_clock())
            .is_some()
    });
    let source = Opened {
        file: source,
        path,
        stamp,
        len,
    };
    if !local_fetch_query::match_strings(q).is_empty() {
        return match_needs_whole_file(q, len);
    }
    if q.full_content == Some(true)
        || local_fetch_query::minify_mode(q).is_some()
        || (q.unit == Some(WindowUnit::Bytes) && local_fetch_query::start_line(q).is_none())
        || local_fetch_query::block(q)
    {
        return unsupported(
            q,
            len,
            "fullContent, minify, block, and whole-source byte chunks need the whole file. Read line windows, or byte pages within one line range.",
            read_bounded(q),
        );
    }
    // A stream's first page carries its note: the page leaves it room.
    let noted = |noted: bool| {
        let note = octocode_engine::json_size::string_body_units(&streamed_warning(len)) + 4;
        share.map(|share| {
            if noted {
                share.saturating_sub(note).max(1)
            } else {
                share
            }
        })
    };
    if local_fetch_query::has_ranges(q) && local_fetch_query::start_line(q).is_none() {
        return fetch_ranges(q, &source, modified, noted(true), security, cancel);
    }
    let first_page = local_fetch_query::offset(q).unwrap_or(0) == 0;
    let mut result = stream_window(q, &source, modified, noted(first_page), security, cancel);
    // The note is the same on every page: the read's first page states it.
    if result.status != ReadStatus::Error && local_fetch_query::offset(q).unwrap_or(0) == 0 {
        result.warnings.insert(0, streamed_warning(len));
    }
    result
}

/// The note a streamed read's first page carries: the size past the
/// whole-file ceiling. The served lines are the row's own line anchors.
fn streamed_warning(len: u64) -> String {
    format!(
        "Large source ({}MB): served from streamed line windows.",
        len / (1024 * 1024)
    )
}

/// A match read needs the whole file: lead to localSearch, which streams
/// it, and to bounded line windows.
fn match_needs_whole_file(q: &LocalFetchQuery, len: u64) -> LocalFetchResult {
    let needles = local_fetch_query::match_strings(q);
    let is_regex = local_fetch_query::is_regex(q);
    let engine = if local_fetch_query::is_pcre2(q) {
        "pcre2"
    } else {
        "rust"
    };
    // A list matches any entry: one alternation for localSearch.
    let (needle, is_regex) = match needles.as_slice() {
        [one] => ((*one).to_owned(), is_regex),
        many => (
            many.iter()
                .map(|n| {
                    if is_regex {
                        (*n).to_owned()
                    } else {
                        ::regex::escape(n)
                    }
                })
                .collect::<Vec<_>>()
                .join("|"),
            true,
        ),
    };
    let mut search = serde_json::json!({
        "path": q.path.to_string(),
        "matchString": needle,
        "regex": if is_regex { engine } else { "literal" },
    });
    if let Some(mode) = q.case_mode {
        search["caseMode"] = serde_json::json!(match mode {
            octocode_contract_types::ReadCaseMode::Sensitive => "sensitive",
            octocode_contract_types::ReadCaseMode::Smart => "smart",
            octocode_contract_types::ReadCaseMode::Insensitive => "insensitive",
        });
    }
    if local_fetch_query::whole_word(q) {
        search["wholeWord"] = serde_json::json!(true);
    }
    let mut next = read_bounded(q);
    next.text_search = Some(
        crate::response::result::Continuation::new(crate::id::ToolId::LocalSearch, search).build(),
    );
    unsupported(q, len, "matchString needs the whole file.", next)
}

/// Bounded line windows from the first line.
fn read_bounded(q: &LocalFetchQuery) -> NextCalls {
    NextCalls {
        read_bounded_lines: Some(FetchCall {
            query: line_chunk_query(q, 0),
        }),
        ..NextCalls::default()
    }
}

#[cfg(test)]
mod tests;
