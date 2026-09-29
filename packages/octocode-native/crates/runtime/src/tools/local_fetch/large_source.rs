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
    let mut line = 1usize;
    let mut line_start = 0usize; // offset of the current line in `window`
    let mut head: Vec<u8> = Vec::new();
    let mut in_key = false;
    let mut pending = false; // bytes seen on the current (unterminated) line
    let mut window = Vec::new();
    let mut held_last = first.saturating_sub(1);
    let mut key_lines = 0usize;
    let mut clipped_line = None;
    let mut window_full = false;
    let mut finish_line = |window: &mut Vec<u8>,
                           head: &mut Vec<u8>,
                           line_start: usize,
                           line: usize,
                           in_key: &mut bool,
                           window_full: bool,
                           held_last: &mut usize| {
        let begins = is_marker(head, "-----BEGIN ");
        let ends = is_marker(head, "-----END ");
        let inside = *in_key || begins;
        if (first..=last).contains(&line) && !window_full {
            if inside {
                window.truncate(line_start);
                window.extend_from_slice(KEY_PLACEHOLDER);
                window.push(b'\n');
                key_lines += 1;
            }
            *held_last = line;
        }
        if begins && !ends {
            *in_key = true;
        } else if ends {
            *in_key = false;
        }
        head.clear();
    };
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
            if head.len() < MARKER_PREFIX_BYTES {
                let take = (MARKER_PREFIX_BYTES - head.len()).min(piece.len());
                head.extend_from_slice(&piece[..take]);
            }
            let in_window = (first..=last).contains(&line) && !window_full;
            if in_window {
                let room = WINDOW_BYTES.saturating_sub(window.len());
                if piece.len() <= room {
                    window.extend_from_slice(piece);
                } else {
                    window.extend_from_slice(&piece[..room]);
                    if line_start == 0 {
                        // One line larger than the whole window: keep its
                        // head and say so.
                        clipped_line = Some(line);
                        if !newline {
                            window.push(b'\n');
                        }
                    } else {
                        // Drop the partial line; the window ends before it.
                        window.truncate(line_start);
                    }
                    window_full = true;
                }
            }
            if newline {
                finish_line(
                    &mut window,
                    &mut head,
                    line_start,
                    line,
                    &mut in_key,
                    window_full && clipped_line != Some(line),
                    &mut held_last,
                );
                if clipped_line == Some(line) {
                    held_last = line;
                }
                line += 1;
                line_start = window.len();
            }
        }
    }
    if pending {
        finish_line(
            &mut window,
            &mut head,
            line_start,
            line,
            &mut in_key,
            window_full && clipped_line != Some(line),
            &mut held_last,
        );
        if clipped_line == Some(line) {
            held_last = line;
        }
    }
    let total_lines = if pending { line } else { line - 1 };
    Ok(Streamed {
        digest: hex::encode(hasher.finalize()),
        total_lines,
        window,
        first,
        last: held_last.min(total_lines),
        key_lines,
        clipped_line,
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
    result.resolved_path = Some(q.path.to_string());
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
    query.match_string_is_regex = None;
    query.match_string_case_sensitive = None;
    query.context_lines = None;
    query.context_bytes = None;
    query.start_line = None;
    query.end_line = None;
    query.chunk_type = Some(ChunkType::Lines);
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
        tool: "localFetch".into(),
        query,
        confidence: "exact".into(),
        reason: Some(reason.into()),
    }
}

/// Serve `q` from a source larger than the whole-file ceiling.
#[allow(clippy::too_many_arguments)]
pub(super) fn fetch_window(
    q: &LocalFetchQuery,
    path: &Path,
    len: u64,
    modified: Option<String>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
    regex: &impl RegexMatch,
) -> LocalFetchResult {
    if let Some(needle) = q.match_string() {
        let mut search = serde_json::json!({
            "path": q.path.to_string(),
            "searchText": needle,
            "regex": if q.match_string_is_regex == Some(true) { "rust" } else { "literal" },
        });
        if q.match_string_case_sensitive == Some(true) {
            search["caseMode"] = serde_json::json!("sensitive");
        }
        let mut result = unsupported(
            q,
            len,
            "matchString needs the whole file. Find the line with localSearch (it streams files up to 512MB), then read it with startLine/endLine.",
            NextCalls {
                r#continue: None,
                read_bounded_lines: Some(continuation(
                    line_chunk_query(q, 0),
                    "Read the file from the start in bounded line windows.",
                )),
                restart: None,
            },
        );
        result.hints = vec![format!(
            "Run localSearch {search} to locate matching lines, then localFetch startLine/endLine around them."
        )];
        return result;
    }
    if q.full_content == Some(true)
        || q.minify_mode() != MinifyMode::None
        || q.chunk_type == Some(ChunkType::Bytes)
    {
        return unsupported(
            q,
            len,
            "fullContent, minify, and byte chunks need the whole file. Read line windows with startLine/endLine or line chunks.",
            NextCalls {
                r#continue: None,
                read_bounded_lines: Some(continuation(
                    line_chunk_query(q, 0),
                    "Read the file in bounded line windows.",
                )),
                restart: None,
            },
        );
    }
    let (first, requested_last, chunked) = match (q.start_line(), q.end_line()) {
        (Some(start), end) => (start, end.unwrap_or(start), false),
        _ => {
            let offset = q.offset().unwrap_or(0);
            (offset + 1, offset + q.chunk_size().unwrap_or(100), true)
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
        let mut result = LocalFetchResult::error(
            q.path.to_string(),
            "staleSnapshot",
            "The file changed since this continuation was issued; restart from the first page."
                .into(),
        );
        result.next = Some(NextCalls {
            r#continue: None,
            read_bounded_lines: None,
            restart: Some(continuation(
                line_chunk_query(q, 0),
                "Restart on the current file version.",
            )),
        });
        return result;
    }
    let total = streamed.total_lines;
    if first > total.max(1) || (total == 0 && first > 1) {
        let mut result = LocalFetchResult::error(
            q.path.to_string(),
            "invalidPagination",
            format!("Line {first} is past the end of the file ({total} lines)."),
        );
        result.total_lines = Some(total);
        result.next = Some(NextCalls {
            r#continue: None,
            read_bounded_lines: None,
            restart: Some(continuation(
                line_chunk_query(q, 0),
                "Read from the first line.",
            )),
        });
        return result;
    }
    let held_last = streamed.last.max(streamed.first);
    let window_lines = held_last + 1 - streamed.first;
    let mut window_query = q.clone();
    window_query.start_line = wire_positive(1);
    window_query.end_line = wire_positive(window_lines);
    window_query.snapshot = None;
    if chunked {
        // The chunk's file offset selected the window itself.
        window_query.offset = None;
        window_query.chunk_size = None;
        window_query.chunk_type = None;
    }
    // An explicit startLine/endLine keeps its offset: it pages *within* the
    // window (the in-window continuation below carries it).
    let mut result = process_fetched_content(
        &window_query,
        &streamed.window,
        path,
        modified,
        security,
        cancel,
        regex,
    );
    if result.status == "error" {
        return result;
    }
    let shift = streamed.first - 1;
    result.start_line = result.start_line.map(|line| line + shift);
    result.end_line = result.end_line.map(|line| line + shift);
    for range in &mut result.source_line_ranges {
        range.start += shift;
        range.end += shift;
    }
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
    // A page inside the window continues within the same absolute window;
    // after the window, the next window continues the request.
    let snapshot = streamed.digest.parse().ok();
    let inner = result
        .next
        .as_mut()
        .and_then(|next| next.r#continue.as_mut());
    if let Some(inner) = inner {
        inner.query.path = q.path.clone();
        inner.query.start_line = wire_positive(streamed.first);
        inner.query.end_line = wire_positive(held_last);
        inner.query.chunk_type = Some(ChunkType::Lines);
        inner.query.snapshot = snapshot;
    } else {
        let more = if chunked {
            held_last < total
        } else {
            held_last < requested_last.min(total)
        };
        result.next = more.then(|| {
            let mut query = if chunked {
                line_chunk_query(q, held_last)
            } else {
                let mut query = q.clone();
                query.start_line = wire_positive(held_last + 1);
                query
            };
            query.snapshot = snapshot;
            NextCalls {
                r#continue: Some(continuation(
                    query,
                    "Next streamed window of the same request.",
                )),
                read_bounded_lines: None,
                restart: None,
            }
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Never;
    impl CancellationCheck for Never {
        fn check(&self) -> Result<(), String> {
            Ok(())
        }
    }

    fn temp(content: &[u8]) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("temp");
        std::io::Write::write_all(&mut file, content).expect("write");
        file
    }

    #[test]
    fn streams_hash_totals_and_window() {
        let file = temp(b"a\nb\nc\nd");
        let streamed = stream(file.path(), 2, 3, &Never).expect("stream");
        assert_eq!(streamed.total_lines, 4);
        assert_eq!(streamed.window, b"b\nc\n");
        assert_eq!((streamed.first, streamed.last), (2, 3));
        assert_eq!(streamed.digest, hex::encode(Sha256::digest(b"a\nb\nc\nd")));
    }

    #[test]
    fn key_block_lines_are_replaced_even_when_markers_are_outside_the_window() {
        let file = temp(b"x\n-----BEGIN RSA PRIVATE KEY-----\nSECRET1\nSECRET2\n-----END RSA PRIVATE KEY-----\ny\n");
        let streamed = stream(file.path(), 3, 4, &Never).expect("stream");
        let window = String::from_utf8(streamed.window).expect("utf8");
        assert!(!window.contains("SECRET"), "{window}");
        assert_eq!(streamed.key_lines, 2);
        let after = stream(file.path(), 6, 6, &Never).expect("stream");
        assert_eq!(after.window, b"y\n");
    }

    #[test]
    fn a_giant_single_line_is_clipped_not_buffered() {
        let big = vec![b'z'; WINDOW_BYTES + 10];
        let file = temp(&big);
        let streamed = stream(file.path(), 1, 1, &Never).expect("stream");
        assert_eq!(streamed.total_lines, 1);
        assert_eq!(streamed.clipped_line, Some(1));
        assert!(streamed.window.len() <= WINDOW_BYTES + 1);
    }
}
