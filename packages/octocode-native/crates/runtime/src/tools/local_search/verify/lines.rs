//! Bounded reads of the source lines a verification needs.

use super::*;

pub(in crate::tools::local_search) enum VerifyReadError {
    /// The source could not be opened or read.
    Io,
    Cancelled(String),
}

impl From<std::io::Error> for VerifyReadError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

/// One streamed pass over a source prefix for the clipped-secret check.
pub(in crate::tools::local_search) struct VerificationLines {
    /// Private-key block ranges over every line read.
    pub(in crate::tools::local_search) key_ranges: Vec<(u32, u32)>,
    /// Neighborhood lines (1-based) that fit the retained-byte budget.
    pub(in crate::tools::local_search) retained: std::collections::BTreeMap<usize, String>,
    pub(in crate::tools::local_search) retained_bytes: usize,
    pub(in crate::tools::local_search) lines_read: usize,
    /// A line too long to buffer mentions `PRIVATE KEY`, so key-block state
    /// is unknown and nothing read from this source can be trusted.
    pub(in crate::tools::local_search) unclassified: bool,
}

/// Stream up to `limit` lines of `reader`, tracking private-key blocks over
/// all of them and keeping only lines inside `windows` (1-based inclusive),
/// within [`MAX_VERIFY_RETAINED_BYTES`]. Any read failure is an error (a
/// short source just yields fewer lines).
pub(in crate::tools::local_search) fn read_verification_lines(
    reader: &mut impl std::io::BufRead,
    windows: &[(usize, usize)],
    limit: usize,
    cancel: &impl CancellationCheck,
) -> Result<VerificationLines, VerifyReadError> {
    let mut tracker = crate::security::KeyBlockTracker::default();
    let mut read = VerificationLines {
        key_ranges: Vec::new(),
        retained: std::collections::BTreeMap::new(),
        retained_bytes: 0,
        lines_read: 0,
        unclassified: false,
    };
    let mut buf = Vec::new();
    while read.lines_read < limit {
        let number = read.lines_read + 1;
        if number.is_multiple_of(VERIFY_CANCEL_EVERY) {
            cancel.check().map_err(VerifyReadError::Cancelled)?;
        }
        let wanted = windows.iter().any(|&(lo, hi)| lo <= number && number <= hi);
        let cap = if wanted {
            MAX_VERIFY_RETAINED_BYTES.saturating_sub(read.retained_bytes)
        } else {
            MAX_PROBE_LINE_BYTES
        };
        buf.clear();
        let Some(line) = read_bounded_line(reader, &mut buf, cap, cancel)? else {
            break;
        };
        read.lines_read = number;
        match line {
            BoundedLine::Whole => {
                let text = String::from_utf8_lossy(&buf);
                let text = text.trim_end_matches(['\n', '\r']);
                tracker.push(text);
                if wanted {
                    read.retained_bytes += text.len();
                    read.retained.insert(number, text.to_owned());
                }
            }
            BoundedLine::Overflow { mentions_key } => {
                // Every key boundary contains `PRIVATE KEY`; a long line
                // without it is an ordinary line for the block state.
                read.unclassified |= mentions_key;
                tracker.push("");
            }
        }
    }
    read.key_ranges = tracker.finish();
    Ok(read)
}

pub(in crate::tools::local_search) enum BoundedLine {
    /// The whole line (with its terminator) is in the buffer.
    Whole,
    /// The line exceeded the cap and was consumed without being kept.
    Overflow { mentions_key: bool },
}

/// Read one line into `buf` if it fits `cap` bytes; otherwise consume it in
/// chunks, noting whether it mentions `PRIVATE KEY`. `None` at end of file.
pub(in crate::tools::local_search) fn read_bounded_line(
    reader: &mut impl std::io::BufRead,
    buf: &mut Vec<u8>,
    cap: usize,
    cancel: &impl CancellationCheck,
) -> Result<Option<BoundedLine>, VerifyReadError> {
    const NEEDLE: &[u8] = b"PRIVATE KEY";
    let mut overflow = false;
    let mut mentions_key = false;
    let mut consumed = 0usize;
    let mut since_check = 0usize;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            break;
        }
        let (take, done) = match chunk.iter().position(|&byte| byte == b'\n') {
            Some(at) => (at + 1, true),
            None => (chunk.len(), false),
        };
        let part = &chunk[..take];
        if !overflow && buf.len() + take > cap {
            overflow = true;
        }
        if overflow {
            // Keep a needle-length tail so a mention split across chunks
            // is still seen.
            buf.extend_from_slice(part);
            mentions_key |= buf.windows(NEEDLE.len()).any(|w| w == NEEDLE);
            let keep = buf.len().min(NEEDLE.len() - 1);
            buf.drain(..buf.len() - keep);
        } else {
            buf.extend_from_slice(part);
        }
        reader.consume(take);
        consumed += take;
        since_check += take;
        if since_check >= VERIFY_CHUNK_BYTES {
            since_check = 0;
            cancel.check().map_err(VerifyReadError::Cancelled)?;
        }
        if done {
            break;
        }
    }
    if consumed == 0 {
        return Ok(None);
    }
    Ok(Some(if overflow {
        buf.clear();
        BoundedLine::Overflow { mentions_key }
    } else {
        BoundedLine::Whole
    }))
}

pub(in crate::tools::local_search) fn materialize(
    scope: &PageScope<'_, impl CancellationCheck>,
    file: &mut octocode_engine::types::TextSearchFile,
    rows: std::ops::Range<usize>,
) -> Result<(), ToolError> {
    visit_values(scope, file, rows, |_, _| Ok(()))
}

/// Reconstruct one value at a time, then release or consume it before reading
/// the next. Every read is policy-checked and hashes the complete source; no
/// result escapes when that digest differs from the search's bytes.
pub(super) fn visit_values(
    scope: &PageScope<'_, impl CancellationCheck>,
    file: &mut octocode_engine::types::TextSearchFile,
    rows: std::ops::Range<usize>,
    mut visit: impl FnMut(usize, &mut octocode_engine::types::TextSearchMatch) -> Result<(), ToolError>,
) -> Result<(), ToolError> {
    use std::io::BufRead;
    // One check per file: a row's visit is bounded work, and every worker
    // polling the shared cancel per row contended on its lock.
    scope.cancel.check().map_err(ToolError::cancelled)?;
    let mut wanted = Vec::new();
    for index in rows {
        let Some(hit) = file.matches.get_mut(index) else {
            continue;
        };
        if hit.deferred.is_some() && hit.value.is_empty() {
            wanted.push(index);
        } else {
            visit(index, hit)?;
        }
    }
    if wanted.is_empty() {
        return Ok(());
    }
    wanted.sort_unstable_by_key(|&i| file.matches[i].line);
    let path = scope.output_root.join(&file.path);
    validate_matched(scope.paths, &path)?;
    let source = file.source.ok_or_else(|| changed_since_scan(scope.query))?;
    let (input, _) =
        octocode_engine::text::open_regular(&path).map_err(|_| changed_since_scan(scope.query))?;
    let mut input = std::io::BufReader::new(CheckedRead {
        input,
        cancel: scope.cancel,
    });
    let mut digest = Sha256::new();
    let mut bytes = Vec::new();
    let mut line = 0u32;
    let mut at = 0;
    let mut size = 0u64;
    // `CheckedRead` polls the cancel on every read of the file, so the
    // lines served from its buffer need no check of their own.
    while at < wanted.len() {
        bytes.clear();
        // Match the engine's line-buffer ceiling, including a terminating newline.
        let read = std::io::Read::take(&mut input, 64 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| {
                scope
                    .cancel
                    .check()
                    .err()
                    .map_or_else(|| changed_since_scan(scope.query), ToolError::cancelled)
            })?;
        if read == 0 {
            break;
        }
        if read > 64 * 1024 * 1024 {
            return Err(changed_since_scan(scope.query));
        }
        digest.update(&bytes);
        size += read as u64;
        line = line.saturating_add(1);
        while let Some(&index) = wanted.get(at).filter(|&&i| file.matches[i].line == line) {
            let value_bytes = if line == 1 {
                &bytes[octocode_engine::text::bom_len(&bytes)..]
            } else {
                &bytes
            };
            let value_bytes = value_bytes
                .split(|byte| *byte == 0)
                .next()
                .unwrap_or_default();
            let text = String::from_utf8_lossy(value_bytes);
            let text = text
                .strip_suffix('\n')
                .map(|s| s.strip_suffix('\r').unwrap_or(s))
                .unwrap_or(&text);
            let hit = &mut file.matches[index];
            hit.value = octocode_engine::search::text_parser::line_match(
                text,
                line,
                hit.column,
                crate::tools::local_search::local_search_query::effective_match_content_length(scope.query)
                    as usize,
            )
            .value;
            visit(index, hit)?;
            at += 1;
        }
    }
    // The remaining source still binds the snapshot, but has no values to
    // reconstruct. A binary tail can be larger than the searcher's line cap.
    drop(bytes);
    let mut chunk = [0u8; 64 * 1024];
    loop {
        scope.cancel.check().map_err(ToolError::cancelled)?;
        let read = std::io::Read::read(&mut input, &mut chunk).map_err(|_| {
            scope
                .cancel
                .check()
                .err()
                .map_or_else(|| changed_since_scan(scope.query), ToolError::cancelled)
        })?;
        if read == 0 {
            break;
        }
        digest.update(&chunk[..read]);
        size += read as u64;
    }
    if at != wanted.len()
        || size != source.size
        || <[u8; 32]>::from(digest.finalize()) != source.digest
    {
        return Err(changed_since_scan(scope.query));
    }
    Ok(())
}

struct CheckedRead<'a, C> {
    input: std::fs::File,
    cancel: &'a C,
}

impl<C: CancellationCheck> std::io::Read for CheckedRead<'_, C> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        std::io::Read::read(&mut self.input, bytes)
    }
}
