//! Bounded line windows over sources above the in-memory ceiling.
//!
//! `localFetch` normally reads the whole source (≤ `MAX_SOURCE_BYTES`) so it
//! can hash it, count lines, and redact private-key blocks across the entire
//! file before cutting a window. A larger source (logs, dumps, generated
//! code) is instead streamed once: the whole-file SHA-256 and line count are
//! computed on the fly, only the requested line window is kept (byte-capped,
//! so even one multi-GB line cannot allocate unbounded; a match scan slides
//! a bounded text window over the source the same way), and every window
//! line inside a private-key block is replaced before the window reaches the
//! regular sanitizer, whichever side of the window the block's markers fall
//! on. What a source's structure says is secret is masked in the window too
//! ([`masks`]). The window then goes through the normal pipeline, and its positions,
//! totals, digest, and continuations are mapped back to the whole file. One long
//! line pages by bytes: a page reads from the pass's nearest place inside the line,
//! and only as far as its window.

use crate::tools::source_view::local_fetch_query;
mod edges;
mod held;
mod hits;
mod kept;
mod masks;
mod matches;
mod page;
mod proven;
mod ranges;
mod scan;
mod stream;
mod unjudged;
mod window;

use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use crate::tools::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadView};
use ranges::fetch_ranges;
use std::path::Path;
use window::{Opened, stream_window};

pub(super) use crate::tools::source_view::prepare::MAX_STREAM_SOURCE_BYTES;
/// Lines held by one streamed window; response pages are sized separately.
const WINDOW_LINES: usize = 5_000;
/// View bytes a byte page's window keeps behind the page: the page finds its
/// line's start and its secret-scan margin there, as in an in-memory read. Past
/// every secret the detectors can bound, and a secret that outruns it (a value over
/// this long, its name out of reach) is the stated limit.
const BYTE_LOOKBEHIND: usize = 128 * 1024;
#[cfg(test)]
use held::WINDOW_BYTES;
/// Bytes a byte page's window holds: the page with that margin on both sides.
const BYTE_WINDOW_BYTES: usize = 3 * BYTE_LOOKBEHIND;
/// A line past this is no key-block line, whatever it spells: a key's own lines are
/// short, and a line of this length is judged by its bytes alone.
const KEY_LINE_BYTES: usize = 1024 * 1024;
/// Line prefix kept to read a private-key block's named lines and armor headers.
const MARKER_PREFIX_BYTES: usize = 256;

#[cfg(test)]
thread_local! {
    /// Source bytes this thread's streamed reads took in.
    static SOURCE_BYTES_READ: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Count `bytes` read from a streamed source.
fn count_read(bytes: usize) {
    #[cfg(test)]
    SOURCE_BYTES_READ.with(|read| read.set(read.get() + bytes));
    #[cfg(not(test))]
    let _ = bytes;
}

/// Source bytes this thread's streamed reads took in so far.
#[cfg(test)]
fn source_bytes_read() -> usize {
    SOURCE_BYTES_READ.with(std::cell::Cell::get)
}

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
    /// The window holds masks of secret bytes ([`masks`]).
    masked: bool,
    /// Those masks hide every value, as no walk could place the secret ones.
    every_value: bool,
}

/// A repair for a request a streamed window cannot serve.
fn unsupported(
    q: &LocalFetchQuery,
    (len, limits): (u64, crate::security::SourceLimits),
    reason: &str,
    next: NextCalls,
) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::LargeSourceWindowOnly,
        format!(
            "This source is {}MB, above the {}MB whole-file read limit, so only bounded line windows are served: {reason}",
            len / (1024 * 1024),
            limits.whole_source / (1024 * 1024)
        ),
    );
    result.next = Some(next);
    result
}

fn line_chunk_query(
    q: &LocalFetchQuery,
    offset: usize,
) -> Result<LocalFetchQuery, serde_json::Error> {
    local_fetch_query::updated(
        q,
        serde_json::json!({"view":"content","matchString":null,"regexEngine":null,"caseMode":null,"wholeWord":null,"contextLines":null,"block":null,"ranges":null,"snapshot":null,"readProgress":{"unit":"lines","offset":offset}}),
    )
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
    let security = &unjudged::Unjudged {
        scan: security,
        path_gated: octocode_engine::security::WholeSourcePatterns::for_path(
            &path.to_string_lossy(),
        )
        .is_some(),
    };
    serve_source(q, path, source, meta, modified, share, security, cancel)
}

fn serve_source(
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
    let kept = kept::current(q, path, meta);
    let source = Opened {
        file: source,
        path,
        len,
        meta,
        proven: kept.as_deref().map(|kept| (&kept.proven, kept.total_lines)),
        limits: security.limits(),
    };
    if local_fetch_query::view(q) == ReadView::Content
        && local_fetch_query::block(q)
        && !local_fetch_query::match_strings(q).is_empty()
    {
        return block_needs_whole_file(q, len, source.limits);
    }
    if local_fetch_query::view(q) != ReadView::Content || local_fetch_query::block(q) {
        return unsupported(
            q,
            (len, source.limits),
            "Transformed views and declaration expansion need the whole file.",
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
    if !local_fetch_query::match_strings(q).is_empty() {
        return matches::fetch_matches(q, &source, modified, noted(true), security, cancel);
    }
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

/// A `block` match read parses the whole file to find each hit's
/// declaration: lead to the same match read without `block`, which streams.
fn block_needs_whole_file(
    q: &LocalFetchQuery,
    len: u64,
    limits: crate::security::SourceLimits,
) -> LocalFetchResult {
    let next = NextCalls {
        read: local_fetch_query::updated(q, serde_json::json!({"block":null}))
            .ok()
            .map(FetchCall::related),
        ..NextCalls::default()
    };
    unsupported(
        q,
        (len, limits),
        "block parses the whole file to find each hit's declaration; the same read without block serves the hits with their context lines.",
        next,
    )
}

/// Bounded line windows from the first line.
fn read_bounded(q: &LocalFetchQuery) -> NextCalls {
    NextCalls {
        read_bounded_lines: line_chunk_query(q, 0).ok().map(FetchCall::related),
        ..NextCalls::default()
    }
}

#[cfg(test)]
mod tests;
