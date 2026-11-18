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
//! on. The window then goes through the normal pipeline, and its positions,
//! totals, digest, and continuations are mapped back to the whole file. One long
//! line pages by bytes: a page reads from the pass's nearest place inside the line,
//! and only as far as its window.

use crate::source_view::local_fetch_query;
mod edges;
mod edit_map;
mod error;
mod held;
mod hits;
mod kept;
mod matches;
mod page;
mod proven;
mod ranges;
mod scan;
mod sealed;
pub mod searched;
mod stream;
mod walk;
mod window;

use crate::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadView};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::security::scan::ContentScan;
use ranges::fetch_ranges;
use std::path::Path;
use window::{Opened, stream_window};

pub use crate::source_view::prepare::MAX_STREAM_SOURCE_BYTES;
pub use searched::{ChangedLines, Patched, changed_lines};
/// Lines held by one streamed window; response pages are sized separately.
const WINDOW_LINES: usize = 5_000;
#[cfg(test)]
use held::WINDOW_BYTES;
/// Fewest source bytes a byte page's window holds from its offset on
/// ([`byte_window_bytes`]). The window is sealed ([`sealed`]), so it needs no
/// margin for a secret scan; a redaction its end cuts shows whole.
const BYTE_WINDOW_BYTES: usize = 384 * 1024;

/// Source bytes a byte page's window holds from its offset on under the
/// response `share`: more than the page takes (its room never takes more
/// than three bytes for an escaped unit), and never under
/// [`BYTE_WINDOW_BYTES`].
fn byte_window_bytes(share: Option<usize>) -> usize {
    BYTE_WINDOW_BYTES.max(3 * crate::source_view::pagination::hit_page_room(share))
}
/// A line past this is no key-block line, whatever it spells: a key's own lines are
/// short, and a line of this length is judged by its bytes alone.
const KEY_LINE_BYTES: usize = 1024 * 1024;
/// Line prefix kept to read a private-key block's named lines and armor headers.
const MARKER_PREFIX_BYTES: usize = octocode_runtime_base::security::KEY_LINE_HEAD_BYTES;

#[cfg(test)]
thread_local! {
    /// Source bytes this thread's streamed reads took in.
    static SOURCE_BYTES_READ: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The budget a source's recorded edits are held against: the process memory
/// budget ([`octocode_engine::memory::MEMORY`]).
fn memory() -> &'static octocode_io::memo::MemoryBudget {
    &octocode_engine::memory::MEMORY
}

/// Count `bytes` read from a streamed source.
fn count_read(bytes: usize) {
    #[cfg(test)]
    {
        SOURCE_BYTES_READ.with(|read| read.set(read.get() + bytes));
    }
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
}

/// A repair for a request a streamed window cannot serve.
fn unsupported(
    q: &LocalFetchQuery,
    (len, limits): (u64, octocode_runtime_base::security::SourceLimits),
    reason: &str,
    next: NextCalls,
) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        ErrorCode::LargeSourceWindowOnly,
        format!(
            "This source is {len} bytes ({}), above the {}-byte ({}) whole-file read limit, so only bounded line windows are served: {reason}",
            mib(len),
            limits.whole_source,
            mib(limits.whole_source_bytes())
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
/// from `source`, the read's open handle on `path`. A later page of the read
/// leaves its read-wide facts to the first ([`LocalFetchResult::continued`]).
pub fn fetch_window(
    q: &LocalFetchQuery,
    path: &Path,
    source: &std::fs::File,
    meta: &std::fs::Metadata,
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let mut result = window_page(q, path, (source, meta), share, security, cancel);
    result.continued = local_fetch_query::read_progress(q).is_some()
        && result
            .pagination
            .as_ref()
            .is_some_and(|page| page.offset > 0);
    result
}

/// [`fetch_window`]'s page of `q`.
fn window_page(
    q: &LocalFetchQuery,
    path: &Path,
    (source, meta): (&std::fs::File, &std::fs::Metadata),
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let len = meta.len();
    let kept = kept::current(q, path, meta, security);
    let source = Opened {
        file: source,
        path,
        len,
        meta,
        proven: kept.as_deref().map(|kept| (&kept.proven, kept.total_lines)),
        limits: security.limits(),
        scan: security,
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
        return matches::fetch_matches(q, &source, noted(true), security, cancel);
    }
    if local_fetch_query::has_ranges(q) && local_fetch_query::start_line(q).is_none() {
        return fetch_ranges(q, &source, noted(true), security, cancel);
    }
    let first_page = local_fetch_query::offset(q).unwrap_or(0) == 0;
    let mut result = stream_window(q, &source, noted(first_page), security, cancel);
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
        "Large source ({len} bytes, {}): served from streamed line windows.",
        mib(len)
    )
}

/// `bytes` in MiB to one decimal, rounded down: shown beside the exact bytes,
/// which keep a comparison with a limit true however close they are.
fn mib(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    let tenths = bytes.saturating_mul(10) / MIB;
    format!("{}.{} MiB", tenths / 10, tenths % 10)
}

/// What a read of a source past the whole-file ceiling serves of some of its
/// lines ([`served_lines`]).
pub struct ServedLines {
    /// The digest of every byte of the source, as hex.
    pub digest: String,
    /// How many lines the source has.
    pub total_lines: usize,
    /// Each wanted span's lines, in the order asked; `None` for a span no
    /// window held whole (a line past the window, or past the source's end).
    pub spans: Vec<Option<Vec<String>>>,
}

/// The lines of each of `wants` (1-based, inclusive) of `source`, a file
/// larger than a source judged whole, as a read of exactly those lines
/// serves them: cut from the one streamed judgment of the whole source
/// ([`sealed`], [`masks`]), then through the read's own pipeline. Another tool that
/// shows lines of such a source shows these, so its row is the read's line.
/// `known` names the digest of bytes an earlier pass may still hold
/// ([`kept`]): their windows are then read without streaming the source again.
pub fn served_lines(
    path: &Path,
    source: &std::fs::File,
    meta: &std::fs::Metadata,
    wants: &[(usize, usize)],
    known: Option<&str>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<ServedLines, String> {
    let kept = known.and_then(|digest| kept::of_digest(digest, path, meta, security));
    let opened = Opened {
        file: source,
        path,
        len: meta.len(),
        meta,
        proven: kept.as_deref().map(|kept| (&kept.proven, kept.total_lines)),
        limits: security.limits(),
        scan: security,
    };
    let windows = wants
        .iter()
        .map(|&(first, last)| held::Want {
            first,
            last,
            from: 0,
            room: opened.limits.window,
        })
        .collect::<Vec<_>>();
    let streamed = opened
        .windows(&windows, cancel)
        .map_err(|error| error.message().to_owned())?;
    let (Some(first), true) = (streamed.first(), streamed.len() == wants.len()) else {
        return Err("no window streamed".to_owned());
    };
    if first.binary {
        return Err("binary content".to_owned());
    }
    let (digest, total_lines) = (first.digest.clone(), first.total_lines);
    let mut spans = Vec::with_capacity(wants.len());
    for (window, &(first, last)) in streamed.iter().zip(wants) {
        cancel.check()?;
        let last = last.min(total_lines);
        // The window holds every wanted line from its first byte.
        if window.first != first || window.last < last || window.origin.base != 0 || last < first {
            spans.push(None);
            continue;
        }
        // The steps a read's own pipeline takes of the window it serves.
        let prepared =
            crate::source_view::prepare::prepared(&window.window, true, security, cancel)?;
        let redaction = prepared
            .redaction(path, security)
            .map_err(|(_, message)| message)?;
        let text = prepared.sealed(redaction).text;
        let lines = text
            .split('\n')
            .take(last + 1 - first)
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
            .collect::<Vec<_>>();
        spans.push((lines.len() == last + 1 - first).then_some(lines));
    }
    Ok(ServedLines {
        digest,
        total_lines,
        spans,
    })
}

/// The redactions turning `raw` into `clean`, a line-preserving redaction of
/// it: each placeholder with the source bytes it stands for.
pub fn redactions(raw: &str, clean: &str) -> Vec<(std::ops::Range<usize>, String)> {
    sealed::align(raw, clean)
}

/// A `block` match read parses the whole file to find each hit's
/// declaration: lead to the same match read without `block`, which streams.
fn block_needs_whole_file(
    q: &LocalFetchQuery,
    len: u64,
    limits: octocode_runtime_base::security::SourceLimits,
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
