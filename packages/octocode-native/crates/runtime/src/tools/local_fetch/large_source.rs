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

use super::executor::process_fetched_content;
use super::types::*;
use crate::security::scan::ContentScan;
use crate::tools::cancel::CancellationCheck;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Largest source a window may be streamed from.
pub(super) const MAX_STREAM_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;
/// Lines one streamed window holds; pages inside it stay 16 KB.
const WINDOW_LINES: usize = 5_000;
/// Bytes one streamed window holds; a longer window is cut at a line end or,
/// for a single giant line, mid-line with a warning.
const WINDOW_BYTES: usize = 4 * 1024 * 1024;
/// Line prefix kept for private-key marker detection outside the window.
const MARKER_PREFIX_BYTES: usize = 256;
const KEY_PLACEHOLDER: &[u8] = b"[REDACTED-PRIVATEKEYFRAGMENT]";

struct Streamed {
    digest: String,
    total_lines: usize,
    window: Vec<u8>,
    /// First and last (1-based) lines actually held in `window`.
    first: usize,
    last: usize,
    key_lines: usize,
    clipped_line: Option<usize>,
}

fn is_marker(head: &[u8], edge: &str) -> bool {
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start();
    text.starts_with(edge) && text.contains("PRIVATE KEY")
}

/// The requested lines kept while a source streams past: byte-capped, with
/// every line inside a private-key block replaced, whichever side of the
/// window its markers fall on.
struct WindowLines {
    first: usize,
    last: usize,
    window: Vec<u8>,
    /// Offset of the current line in `window`.
    line_start: usize,
    /// The current line's leading bytes, for private-key marker detection.
    head: Vec<u8>,
    in_key: bool,
    held_last: usize,
    key_lines: usize,
    clipped_line: Option<usize>,
    full: bool,
}

impl WindowLines {
    fn new(first: usize, last: usize) -> Self {
        Self {
            first,
            last,
            window: Vec::new(),
            line_start: 0,
            head: Vec::new(),
            in_key: false,
            held_last: first.saturating_sub(1),
            key_lines: 0,
            clipped_line: None,
            full: false,
        }
    }

    /// Take one piece of `line` (up to and including its newline, if any).
    fn push(&mut self, piece: &[u8], newline: bool, line: usize) {
        if self.head.len() < MARKER_PREFIX_BYTES {
            let take = (MARKER_PREFIX_BYTES - self.head.len()).min(piece.len());
            self.head.extend_from_slice(&piece[..take]);
        }
        if !(self.first..=self.last).contains(&line) || self.full {
            return;
        }
        let room = WINDOW_BYTES.saturating_sub(self.window.len());
        if piece.len() <= room {
            self.window.extend_from_slice(piece);
            return;
        }
        self.window.extend_from_slice(&piece[..room]);
        if self.line_start == 0 {
            // One line larger than the whole window: keep its head and say so.
            self.clipped_line = Some(line);
            if !newline {
                self.window.push(b'\n');
            }
        } else {
            // Drop the partial line; the window ends before it.
            self.window.truncate(self.line_start);
        }
        self.full = true;
    }

    /// Close `line`: replace it when it sits in a private-key block, and
    /// track the block's markers.
    fn finish(&mut self, line: usize) {
        let begins = is_marker(&self.head, "-----BEGIN ");
        let ends = is_marker(&self.head, "-----END ");
        let inside = self.in_key || begins;
        let held = self.full && self.clipped_line != Some(line);
        if (self.first..=self.last).contains(&line) && !held {
            if inside {
                self.window.truncate(self.line_start);
                self.window.extend_from_slice(KEY_PLACEHOLDER);
                self.window.push(b'\n');
                self.key_lines += 1;
            }
            self.held_last = line;
        }
        if begins && !ends {
            self.in_key = true;
        } else if ends {
            self.in_key = false;
        }
        self.head.clear();
        if self.clipped_line == Some(line) {
            self.held_last = line;
        }
        self.line_start = self.window.len();
    }
}

/// Stream `path`: hash all bytes, count lines like `line_count`, and keep
/// lines `first..=last` (1-based) of the requested window.
fn stream(
    path: &Path,
    first: usize,
    last: usize,
    cancel: &impl CancellationCheck,
) -> Result<Streamed, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut lines = WindowLines::new(first, last);
    let mut line = 1usize;
    let mut pending = false; // bytes seen on the current (unterminated) line
    loop {
        cancel.check()?;
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        hasher.update(chunk);
        let mut rest = chunk;
        while !rest.is_empty() {
            let (piece, newline) = match rest.iter().position(|byte| *byte == b'\n') {
                Some(index) => (&rest[..=index], true),
                None => (rest, false),
            };
            rest = &rest[piece.len()..];
            pending = !newline;
            lines.push(piece, newline, line);
            if newline {
                lines.finish(line);
                line += 1;
            }
        }
    }
    if pending {
        lines.finish(line);
    }
    let total_lines = if pending { line } else { line - 1 };
    Ok(Streamed {
        digest: hex::encode(hasher.finalize()),
        total_lines,
        window: lines.window,
        first,
        last: lines.held_last.min(total_lines),
        key_lines: lines.key_lines,
        clipped_line: lines.clipped_line,
    })
}

/// A repair for a request a streamed window cannot serve.
fn unsupported(q: &LocalFetchQuery, len: u64, reason: &str, next: NextCalls) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        q.path.to_string(),
        "largeSourceWindowOnly",
        format!(
            "This source is {}MB, above the {}MB whole-file read limit, so only bounded line windows are served: {reason}",
            len / (1024 * 1024),
            super::executor::MAX_SOURCE_BYTES / (1024 * 1024)
        ),
    );
    result.source_bytes = usize::try_from(len).ok();
    result.is_partial = Some(true);
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
    query.context_bytes = None;
    query.clear_block_selectors();
    query.unit = Some(WindowUnit::Lines);
    query.offset = if offset == 0 {
        None
    } else {
        Some(wire_count(offset))
    };
    query.snapshot = None;
    query
}

fn continuation(query: LocalFetchQuery, reason: &str) -> Continuation {
    Continuation {
        query,
        reason: Some(reason.into()),
    }
}

/// Serve `q` from a source larger than the whole-file ceiling.
pub(super) fn fetch_window(
    q: &LocalFetchQuery,
    path: &Path,
    len: u64,
    modified: Option<String>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> LocalFetchResult {
    if !q.match_strings().is_empty() {
        return match_needs_whole_file(q, len);
    }
    if q.full_content == Some(true)
        || q.minify_mode() != MinifyMode::None
        || q.unit == Some(WindowUnit::Bytes)
        || (q.has_ranges() && q.start_line().is_none())
        || q.block()
    {
        return unsupported(
            q,
            len,
            "fullContent, minify, ranges, block, and byte chunks need the whole file. Read one line range or line windows.",
            read_bounded(q, "Read the file in bounded line windows."),
        );
    }
    let (first, requested_last, chunked) = match (q.start_line(), q.end_line()) {
        (Some(start), end) => (start, end.unwrap_or(start), false),
        _ => {
            let offset = q.offset().unwrap_or(0);
            (offset + 1, offset + q.window_length().unwrap_or(100), true)
        }
    };
    let last = requested_last.min(first + WINDOW_LINES - 1);
    let streamed = match stream(path, first, last, cancel) {
        Ok(streamed) => streamed,
        Err(error) => return LocalFetchResult::error(q.path.to_string(), "fileReadFailed", error),
    };
    if let Some(expected) = q.snapshot.as_deref()
        && **expected != streamed.digest
    {
        return restart_error(
            q,
            "staleSnapshot",
            crate::response::pages::STALE_SNAPSHOT_ERROR.into(),
            "Restart on the current file version.",
        );
    }
    let total = streamed.total_lines;
    if first > total.max(1) || (total == 0 && first > 1) {
        let mut result = restart_error(
            q,
            "invalidPagination",
            format!("Line {first} is past the end of the file ({total} lines)."),
            "Read from the first line.",
        );
        result.total_lines = Some(total);
        return result;
    }
    let held_last = streamed.last.max(streamed.first);
    let mut window_query = q.clone();
    window_query.set_line_span(1, held_last + 1 - streamed.first);
    window_query.snapshot = None;
    if chunked {
        // The chunk's file offset selected the window itself.
        window_query.offset = None;
        window_query.length = None;
        window_query.unit = None;
    }
    // An explicit line range keeps its offset: it pages *within* the
    // window (the in-window continuation below carries it).
    let mut result = process_fetched_content(
        &window_query,
        &streamed.window,
        path,
        &super::types::SourceFacts {
            modified,
            window: None,
        },
        security,
        cancel,
        regex,
    );
    if result.status == "error" {
        return result;
    }
    map_to_file(&mut result, &streamed, len);
    let more = if chunked {
        held_last < total
    } else {
        held_last < requested_last.min(total)
    };
    continue_window(
        q,
        &mut result,
        &streamed,
        more.then_some((chunked, requested_last)),
    );
    result
}

/// A match read needs the whole file: lead to localSearch, which streams
/// it, and to bounded line windows.
fn match_needs_whole_file(q: &LocalFetchQuery, len: u64) -> LocalFetchResult {
    let needles = q.match_strings();
    let is_regex = q.is_regex();
    let engine = if q.is_pcre2() { "pcre2" } else { "rust" };
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
            crate::contracts::tool_types::ReadCaseMode::Sensitive => "sensitive",
            crate::contracts::tool_types::ReadCaseMode::Smart => "smart",
            crate::contracts::tool_types::ReadCaseMode::Insensitive => "insensitive",
        });
    }
    let mut result = unsupported(
        q,
        len,
        "matchString needs the whole file. Find the line with localSearch (it streams files up to 512MB), then read it with ranges.",
        read_bounded(q, "Read the file from the start in bounded line windows."),
    );
    result.hints = vec![format!(
        "Run localSearch {search} to locate matching lines, then localFetch ranges around them."
    )];
    result
}

/// Bounded line windows from the first line.
fn read_bounded(q: &LocalFetchQuery, reason: &str) -> NextCalls {
    NextCalls {
        read_bounded_lines: Some(continuation(line_chunk_query(q, 0), reason)),
        ..NextCalls::default()
    }
}

/// An error row whose recovery restarts from the first line window.
fn restart_error(
    q: &LocalFetchQuery,
    code: &str,
    message: String,
    reason: &str,
) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(q.path.to_string(), code, message);
    result.next = Some(NextCalls {
        restart: Some(continuation(line_chunk_query(q, 0), reason)),
        ..NextCalls::default()
    });
    result
}

/// Map a window's positions, totals and digest back to the whole file.
fn map_to_file(result: &mut LocalFetchResult, streamed: &Streamed, len: u64) {
    let held_last = streamed.last.max(streamed.first);
    let shift = streamed.first - 1;
    result.start_line = result.start_line.map(|line| line + shift);
    result.end_line = result.end_line.map(|line| line + shift);
    for range in &mut result.source_line_ranges {
        range.start += shift;
        range.end += shift;
    }
    let total = streamed.total_lines;
    result.total_lines = Some(total);
    result.source_bytes = usize::try_from(len).ok();
    result.source_chars = None;
    result.source_sha256 = Some(streamed.digest.clone());
    result.warnings.insert(
        0,
        format!(
            "Large source ({}MB): served lines {}-{} of {total} from a streamed window.",
            len / (1024 * 1024),
            streamed.first,
            held_last
        ),
    );
    if streamed.key_lines > 0 {
        result.warnings.push(format!(
            "redactedContent: {} line(s) inside private-key blocks were replaced; this content is not verbatim source.",
            streamed.key_lines
        ));
    }
    if let Some(line) = streamed.clipped_line {
        result.warnings.push(format!(
            "Line {line} is longer than {}MB; only its first {}MB was read.",
            WINDOW_BYTES / (1024 * 1024),
            WINDOW_BYTES / (1024 * 1024)
        ));
        result.is_partial = Some(true);
    }
}

/// A page inside the window continues within the same absolute window;
/// after the window, `more` (chunked, requested last line) continues the
/// request with the next window.
fn continue_window(
    q: &LocalFetchQuery,
    result: &mut LocalFetchResult,
    streamed: &Streamed,
    more: Option<(bool, usize)>,
) {
    let held_last = streamed.last.max(streamed.first);
    let snapshot = streamed.digest.parse().ok();
    let inner = result
        .next
        .as_mut()
        .and_then(|next| next.r#continue.as_mut());
    if let Some(inner) = inner {
        inner.query.path = q.path.clone();
        inner.query.set_line_span(streamed.first, held_last);
        inner.query.unit = Some(WindowUnit::Lines);
        inner.query.snapshot = snapshot;
        return;
    }
    result.next = more.map(|(chunked, requested_last)| {
        let mut query = if chunked {
            line_chunk_query(q, held_last)
        } else {
            let mut query = q.clone();
            query.set_line_span(held_last + 1, requested_last);
            query
        };
        query.snapshot = snapshot;
        NextCalls {
            r#continue: Some(continuation(
                query,
                "Next streamed window of the same request.",
            )),
            ..NextCalls::default()
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::cancel::NeverCancel;

    fn temp(content: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("temp");
        std::io::Write::write_all(&mut file, content).expect("write");
        file
    }

    #[test]
    fn streams_hash_totals_and_window() {
        let file = temp(b"a\nb\nc\nd");
        let streamed = stream(file.path(), 2, 3, &NeverCancel).expect("stream");
        assert_eq!(streamed.total_lines, 4);
        assert_eq!(streamed.window, b"b\nc\n");
        assert_eq!((streamed.first, streamed.last), (2, 3));
        assert_eq!(streamed.digest, hex::encode(Sha256::digest(b"a\nb\nc\nd")));
    }

    #[test]
    fn key_block_lines_are_replaced_even_when_markers_are_outside_the_window() {
        let file = temp(b"x\n-----BEGIN RSA PRIVATE KEY-----\nSECRET1\nSECRET2\n-----END RSA PRIVATE KEY-----\ny\n");
        let streamed = stream(file.path(), 3, 4, &NeverCancel).expect("stream");
        let window = String::from_utf8(streamed.window).expect("utf8");
        assert!(!window.contains("SECRET"), "{window}");
        assert_eq!(streamed.key_lines, 2);
        let after = stream(file.path(), 6, 6, &NeverCancel).expect("stream");
        assert_eq!(after.window, b"y\n");
    }

    #[test]
    fn a_giant_single_line_is_clipped_not_buffered() {
        let big = vec![b'z'; WINDOW_BYTES + 10];
        let file = temp(&big);
        let streamed = stream(file.path(), 1, 1, &NeverCancel).expect("stream");
        assert_eq!(streamed.total_lines, 1);
        assert_eq!(streamed.clipped_line, Some(1));
        assert!(streamed.window.len() <= WINDOW_BYTES + 1);
    }
}
