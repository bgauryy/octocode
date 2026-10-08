//! Secret safety of shown values: redaction of the scan, the re-read that
//! checks clipped values against their source bytes, and bounded line reads.

use super::leads::ENCLOSING_MAX_FILES;
use super::types::*;
use super::{cursor::*, layout::*};
use crate::security::ContentSecurity;
use crate::tools::ast_search::MAX_PARSE_SOURCE_BYTES;
use crate::tools::cancel::CancellationCheck;
use crate::tools::result::ToolError;
use sha2::{Digest, Sha256};

/// Upper bound (bytes) on a file re-read for the private-key block scan.
/// Matches the secret scanner's own content cap; larger files fall back to the
/// per-match window sanitizer rather than pay an unbounded read.
pub(super) const MAX_KEY_SCAN_BYTES: u64 = 10 * 1024 * 1024;

/// What a stored scan's file must still hash to (see `expected_digest`).
pub(super) type ExpectedDigest<'a> =
    dyn Fn(&std::path::Path) -> Option<Option<super::manifest::Digest>> + 'a;

/// (path, line, column) of every match whose value secret redaction
/// changed: the returned text is then not verbatim source, and the row says
/// so via a `redactedMatches` warning.
pub(super) type Redacted = std::collections::HashSet<(String, u32, u32)>;

/// What redacting a fresh scan changed, so a later page of the same stored
/// scan replays it instead of sanitizing every value again. Sanitizing is a
/// pure function of a value and its source path; the only file input is a
/// key-block scan, so the record keeps the digest of each such read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Redaction {
    /// The root the scan's paths were made relative to.
    pub(super) output_root: std::path::PathBuf,
    /// Files whose snippets looked like key material, by scan index, with
    /// the digest of the bytes their key-block scan read (`None`: no read).
    pub(super) key_files: Vec<(usize, Option<super::manifest::Digest>)>,
    /// Values the redaction replaced: file index, match index, new value.
    pub(super) changes: Vec<(usize, usize, String)>,
}

impl Redaction {
    /// Bytes the record adds to a stored scan.
    pub(super) fn weight(&self) -> usize {
        self.output_root.as_os_str().len()
            + self.key_files.len() * 40
            + self
                .changes
                .iter()
                .map(|(_, _, value)| value.len() + 16)
                .sum::<usize>()
    }
}

/// Make paths relative to `output_root` and redact secrets in every value.
/// A match on an interior base64 body line of a private key would leak the
/// key even though the match view holds no BEGIN/END marker (the anchored
/// built-in patterns need a complete block). Only when a snippet actually
/// looks like key material is the full file scanned for private-key block
/// ranges; matches whose window intersects a block are redacted. Innocent
/// base64 triggers a scan that finds no block and redacts nothing. A file
/// is read only after it passes the read policy.
///
/// `stored` is the redaction a fresh page made of this stored scan: it is
/// replayed when every key-block scan reads the same bytes again (see
/// [`replay_redaction`]); otherwise every value is redacted afresh. Returns
/// the redacted rows and the record of this redaction.
#[allow(clippy::too_many_arguments)]
pub(super) fn redact_scan(
    query: &LocalSearchQuery,
    parsed: &mut octocode_engine::types::TextSearchResult,
    output_root: &std::path::Path,
    expected_digest: &ExpectedDigest<'_>,
    paths: &crate::policy::path::PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
    stored: Option<&Redaction>,
) -> Result<(Redacted, Redaction), ToolError> {
    if let Some(stored) = stored.filter(|stored| stored.output_root == output_root) {
        cancel.check().map_err(ToolError::cancelled)?;
        if let Some(redacted) =
            replay_redaction(query, parsed, output_root, expected_digest, paths, stored)?
        {
            return Ok((redacted, stored.clone()));
        }
    }
    let mut redacted = Redacted::new();
    let mut record = Redaction {
        output_root: output_root.to_path_buf(),
        ..Redaction::default()
    };
    for (file_index, file) in parsed.files.iter_mut().enumerate() {
        if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
            file.path = relative.to_string_lossy().into_owned();
        }
        let source_path = output_root.join(&file.path);
        let key_ranges = if file
            .matches
            .iter()
            .any(|m| crate::security::snippet_may_hold_key_material(&m.value))
        {
            validate_matched(paths, &source_path)?;
            let bytes = read_key_scan_bytes(&source_path);
            let digest = bytes
                .as_ref()
                .map(|bytes| <[u8; 32]>::from(Sha256::digest(bytes)));
            record.key_files.push((file_index, digest));
            if let (Some(digest), Some(expected)) = (digest, expected_digest(&source_path))
                && expected != Some(digest)
            {
                return Err(changed_since_scan(query));
            }
            bytes
                .map(|bytes| {
                    crate::security::private_key_block_line_ranges(&String::from_utf8_lossy(&bytes))
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for (match_index, matched) in file.matches.iter_mut().enumerate() {
            cancel.check().map_err(ToolError::cancelled)?;
            let changed = if !key_ranges.is_empty()
                && crate::security::match_window_intersects_key_block(
                    matched.line,
                    &matched.value,
                    &key_ranges,
                ) {
                matched.value = crate::security::key_fragment_placeholder();
                true
            } else {
                let sanitized = security.sanitize_text(&matched.value, Some(&source_path));
                let changed = sanitized.content != matched.value;
                matched.value = sanitized.content;
                changed
            };
            if changed {
                redacted.insert((file.path.clone(), matched.line, matched.column));
                record
                    .changes
                    .push((file_index, match_index, matched.value.clone()));
            }
        }
    }
    Ok((redacted, record))
}

/// The bytes a key-block scan reads: the whole file within
/// [`MAX_KEY_SCAN_BYTES`], or `None`.
fn read_key_scan_bytes(source: &std::path::Path) -> Option<Vec<u8>> {
    std::fs::metadata(source)
        .ok()
        .filter(|meta| meta.len() <= MAX_KEY_SCAN_BYTES)
        .and_then(|_| std::fs::read(source).ok())
}

/// Replay `stored` on the stored scan it was made from. Its key-block scans
/// read each file again, under the read policy and bound to the stored
/// digest exactly as a fresh redaction is (a changed file restarts the
/// snapshot). When each read hashes to the bytes the record was made from,
/// the key-block ranges, and so every replaced value, are the ones a fresh
/// redaction computes; `None` (no read, other bytes) redacts afresh.
fn replay_redaction(
    query: &LocalSearchQuery,
    parsed: &mut octocode_engine::types::TextSearchResult,
    output_root: &std::path::Path,
    expected_digest: &ExpectedDigest<'_>,
    paths: &crate::policy::path::PathPolicy,
    stored: &Redaction,
) -> Result<Option<Redacted>, ToolError> {
    for &(index, made_from) in &stored.key_files {
        let Some(file) = parsed.files.get(index) else {
            return Ok(None);
        };
        let source_path = output_root.join(&file.path);
        validate_matched(paths, &source_path)?;
        let Some(bytes) = read_key_scan_bytes(&source_path) else {
            return Ok(None);
        };
        let digest = <[u8; 32]>::from(Sha256::digest(&bytes));
        if let Some(expected) = expected_digest(&source_path)
            && expected != Some(digest)
        {
            return Err(changed_since_scan(query));
        }
        if made_from != Some(digest) {
            return Ok(None);
        }
    }
    if stored.changes.iter().any(|&(file, row, _)| {
        parsed
            .files
            .get(file)
            .is_none_or(|file| row >= file.matches.len())
    }) {
        return Ok(None);
    }
    for file in &mut parsed.files {
        if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
            file.path = relative.to_string_lossy().into_owned();
        }
    }
    let mut redacted = Redacted::new();
    for (file, row, value) in &stored.changes {
        let file = &mut parsed.files[*file];
        let matched = &mut file.matches[*row];
        matched.value.clone_from(value);
        redacted.insert((file.path.clone(), matched.line, matched.column));
    }
    Ok(Some(redacted))
}

/// A matched file must pass the read policy before a page reads it or shows
/// its rows.
pub(super) fn validate_matched(
    paths: &crate::policy::path::PathPolicy,
    source: &std::path::Path,
) -> Result<(), ToolError> {
    paths.validate_read(source).map(drop).map_err(|error| {
        ToolError::new(
            error.local_error_code("fileAccessFailed"),
            "Search encountered a path denied by the active path policy",
        )
    })
}

/// Outlines of the page's first shown files, keyed by source path, parsed
/// from the bytes their secret check read (`None`: no outline).
pub(super) type Outlines =
    std::collections::HashMap<std::path::PathBuf, Option<super::enclosing::Outline>>;

/// Re-read the shown files. A page from a stored scan shows each of its
/// files only while that file still hashes to the stored bytes; text views
/// prove it in the secret check, which reads the same bytes, and the first
/// [`ENCLOSING_MAX_FILES`] files with rows are outlined from that read into
/// `outlines`. Returns whether some values were redacted because their
/// source could not be re-read.
#[allow(clippy::too_many_arguments)]
pub(super) fn verify_shown(
    query: &LocalSearchQuery,
    parsed: &mut octocode_engine::types::TextSearchResult,
    layout: &Layout,
    output_root: &std::path::Path,
    expected_digest: &ExpectedDigest<'_>,
    redacted: &mut Redacted,
    outlines: &mut Outlines,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
) -> Result<bool, ToolError> {
    if layout.list {
        for (index, _) in &layout.shown {
            let source = output_root.join(&parsed.files[*index].path);
            if let Some(expected) = expected_digest(&source)
                && (expected.is_none() || expected != super::manifest::digest_file(&source).ok())
            {
                return Err(changed_since_scan(query));
            }
        }
        return Ok(false);
    }
    let mut unverified = false;
    let mut files_with_rows = 0usize;
    // Outlines are parsed after the checks, in parallel: one tree-sitter
    // parse per shown file was the page's slowest serial step.
    let mut to_outline: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
    for (index, rows) in &layout.shown {
        let file = &mut parsed.files[*index];
        cancel.check().map_err(ToolError::cancelled)?;
        let before = file
            .matches
            .iter()
            .map(|matched| matched.value.clone())
            .collect::<Vec<_>>();
        let source = output_root.join(&file.path);
        // The page's first files are outlined for enclosing names: read
        // each once, for both its secret check and its outline.
        let shows_rows = rows.start.min(file.matches.len()) < rows.end.min(file.matches.len());
        let outlined = shows_rows && files_with_rows < ENCLOSING_MAX_FILES;
        files_with_rows += usize::from(shows_rows);
        let bytes = outlined
            .then(|| crate::tools::source::read_bounded(&source, MAX_PARSE_SOURCE_BYTES).ok())
            .flatten();
        let match_only = query.result_view == LocalSearchQueryResultView::MatchOnly;
        let expected = expected_digest(&source);
        let verification = match &bytes {
            Some(bytes) => guard_read(
                file,
                &source,
                || Ok(bytes.as_slice()),
                expected,
                rows.clone(),
                security,
                match_only,
                cancel,
            ),
            None => guard_clipped_secrets(
                file,
                &source,
                expected,
                rows.clone(),
                security,
                match_only,
                cancel,
            ),
        };
        match verification.map_err(ToolError::cancelled)? {
            Verification::Verified => {}
            Verification::Unverified => unverified = true,
            Verification::Changed => return Err(changed_since_scan(query)),
        }
        if let Some(bytes) = bytes {
            to_outline.push((source, bytes));
        }
        for (matched, before) in file.matches.iter().zip(before) {
            if matched.value != before {
                redacted.insert((file.path.clone(), matched.line, matched.column));
            }
        }
    }
    outlines.extend(parallel_outlines(to_outline, security));
    Ok(unverified)
}

/// The outline of each read file, parsed on scoped threads; order-free.
fn parallel_outlines(
    files: Vec<(std::path::PathBuf, Vec<u8>)>,
    security: &ContentSecurity,
) -> Vec<(std::path::PathBuf, Option<super::enclosing::Outline>)> {
    if files.len() < 2 {
        return files
            .into_iter()
            .map(|(source, bytes)| {
                let outline = super::leads::outline_of(&bytes, &source, security);
                (source, outline)
            })
            .collect();
    }
    let workers = std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .min(files.len());
    let per_worker = files.len().div_ceil(workers);
    let mut files = files;
    std::thread::scope(|scope| {
        let mut tasks = Vec::with_capacity(workers);
        while !files.is_empty() {
            let chunk: Vec<_> = files.drain(..per_worker.min(files.len())).collect();
            tasks.push(scope.spawn(move || {
                chunk
                    .into_iter()
                    .map(|(source, bytes)| {
                        let outline = super::leads::outline_of(&bytes, &source, security);
                        (source, outline)
                    })
                    .collect::<Vec<_>>()
            }));
        }
        // A worker that panicked leaves its files without enclosing names.
        tasks
            .into_iter()
            .filter_map(|task| task.join().ok())
            .flatten()
            .collect()
    })
}

/// Remove `…` window markers and `...` truncation suffixes from a value line.
pub(super) fn strip_clip_markers(line: &str) -> &str {
    let line = line.strip_prefix('…').unwrap_or(line);
    let line = line.strip_suffix("...").unwrap_or(line);
    line.strip_suffix('…').unwrap_or(line)
}

/// Security: the engine clips values (matchOnly spans, matchContentLength,
/// long-line windows) *before* sanitization, so a clipped
/// secret no longer matches any secret pattern and leaks verbatim. For each
/// shown match, sanitize the full source lines the value was cut from; a value
/// line not literally present in that sanitized text overlapped a redaction and
/// is replaced (placeholder for spans, the sanitized match line otherwise).
/// Private-key blocks are detected from the whole file prefix, streamed once
/// without retaining it; only the shown matches' neighborhoods are kept.
///
/// Fails closed: when the source cannot be re-read, no longer holds a shown
/// line, or its neighborhood exceeds the retained-byte budget, the value is
/// replaced by a placeholder and the result is `Unverified`. With `expected`
/// (values from a stored scan), the whole source is hashed while it is read;
/// bytes that differ from the stored digest, or a missing digest, return
/// `Changed` and the values must not be shown. Cancellation stops the read
/// and returns the reason.
pub(super) fn guard_clipped_secrets(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    expected: Option<Option<super::manifest::Digest>>,
    shown: std::ops::Range<usize>,
    security: &ContentSecurity,
    match_only: bool,
    cancel: &impl CancellationCheck,
) -> Result<Verification, String> {
    guard_read(
        file,
        source,
        || std::fs::File::open(source),
        expected,
        shown,
        security,
        match_only,
        cancel,
    )
}

/// [`guard_clipped_secrets`] over the reader `open` returns, opened only
/// when a shown value needs its source lines.
#[allow(clippy::too_many_arguments)]
pub(super) fn guard_read<R: std::io::Read>(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    open: impl FnOnce() -> std::io::Result<R>,
    expected: Option<Option<super::manifest::Digest>>,
    shown: std::ops::Range<usize>,
    security: &ContentSecurity,
    match_only: bool,
    cancel: &impl CancellationCheck,
) -> Result<Verification, String> {
    if expected == Some(None) {
        return Ok(Verification::Changed);
    }
    let end = shown.end.min(file.matches.len());
    let start = shown.start.min(end);
    let shown = &mut file.matches[start..end];
    // 1-based inclusive source lines each shown value needs re-sanitized.
    let windows = shown
        .iter()
        .map(|m| {
            let span = m.value.lines().count().max(1);
            let line = m.line as usize;
            (line.saturating_sub(span).max(1), line + span)
        })
        .collect::<Vec<_>>();
    let Some(last_line) = windows.iter().map(|&(_, hi)| hi).max() else {
        return Ok(Verification::Verified);
    };
    let read = open().map_err(VerifyReadError::from).and_then(|opened| {
        let mut reader = std::io::BufReader::new(HashingReader {
            inner: opened,
            hasher: expected.map(|_| Sha256::new()),
        });
        let read = read_verification_lines(&mut reader, &windows, last_line, cancel)?;
        let digest = drain_digest(reader, cancel)?;
        Ok((read, digest))
    });
    let read = match read {
        Ok((_, Some(digest))) if expected != Some(Some(digest)) => {
            return Ok(Verification::Changed);
        }
        Ok((read, _)) if !read.unclassified => read,
        Ok(_) | Err(VerifyReadError::Io) => {
            for matched in shown.iter_mut() {
                matched.value = UNVERIFIED_PLACEHOLDER.to_owned();
            }
            return Ok(Verification::Unverified);
        }
        Err(VerifyReadError::Cancelled(reason)) => return Err(reason),
    };
    let mut verified = Verification::Verified;
    for (matched, &(lo, hi)) in shown.iter_mut().zip(&windows) {
        if !read.key_ranges.is_empty()
            && crate::security::match_window_intersects_key_block(
                matched.line,
                &matched.value,
                &read.key_ranges,
            )
        {
            matched.value = crate::security::key_fragment_placeholder();
            continue;
        }
        let line = matched.line as usize;
        if line == 0 {
            continue;
        }
        if line > read.lines_read {
            // The source shrank or was replaced since the search.
            matched.value = UNVERIFIED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        }
        let hi = hi.min(read.lines_read);
        let Some(window) = (lo..=hi)
            .map(|n| read.retained.get(&n).map(String::as_str))
            .collect::<Option<Vec<_>>>()
        else {
            matched.value = OVERSIZED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        };
        let sanitized = security.sanitize_text(&window.join("\n"), Some(source));
        if !sanitized.has_secrets {
            continue;
        }
        let exposed = matched.value.lines().any(|value_line| {
            let core = strip_clip_markers(value_line);
            !core.is_empty() && !sanitized.content.contains(core)
        });
        if exposed {
            matched.value = if match_only {
                "[REDACTED]".to_owned()
            } else {
                security
                    .sanitize_text(window[line - lo], Some(source))
                    .content
            };
        }
    }
    Ok(verified)
}

/// Outcome of [`guard_clipped_secrets`] for one file's shown matches.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verification {
    /// Every shown value was checked against the source.
    Verified,
    /// Some value was replaced by a placeholder because it could not be checked.
    Unverified,
    /// The source is not the stored scan's bytes; nothing may be shown.
    Changed,
}

/// Passes reads through, hashing them when a digest is wanted.
pub(super) struct HashingReader<R> {
    pub(super) inner: R,
    pub(super) hasher: Option<Sha256>,
}

impl<R: std::io::Read> std::io::Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if let Some(hasher) = &mut self.hasher {
            hasher.update(&buf[..read]);
        }
        Ok(read)
    }
}

/// Read the rest of a hashed source and return its digest; `None` when the
/// reader was not hashing.
pub(super) fn drain_digest<R: std::io::Read>(
    mut reader: std::io::BufReader<HashingReader<R>>,
    cancel: &impl CancellationCheck,
) -> Result<Option<super::manifest::Digest>, VerifyReadError> {
    use std::io::BufRead;
    if reader.get_ref().hasher.is_none() {
        return Ok(None);
    }
    let mut since_check = 0usize;
    loop {
        let len = reader.fill_buf()?.len();
        if len == 0 {
            break;
        }
        reader.consume(len);
        since_check += len;
        if since_check >= VERIFY_CHUNK_BYTES {
            since_check = 0;
            cancel.check().map_err(VerifyReadError::Cancelled)?;
        }
    }
    Ok(reader
        .into_inner()
        .hasher
        .map(|hasher| hasher.finalize().into()))
}

/// Value shown in place of a match whose source could not be re-read for the
/// clipped-secret check.
pub(super) const UNVERIFIED_PLACEHOLDER: &str = "[REDACTED: source unreadable for secret check]";

/// Value shown in place of a match whose source neighborhood exceeds
/// [`MAX_VERIFY_RETAINED_BYTES`].
pub(super) const OVERSIZED_PLACEHOLDER: &str =
    "[REDACTED: source lines too large for secret check]";

/// Source bytes one file's verification may retain for the shown matches'
/// neighborhoods; the rest of the prefix is streamed for key-block state only.
pub(super) const MAX_VERIFY_RETAINED_BYTES: usize = 16 * 1024 * 1024;

/// Bytes of a line outside every neighborhood buffered to classify it; a
/// longer line is consumed in chunks without being stored.
pub(super) const MAX_PROBE_LINE_BYTES: usize = 64 * 1024;

/// Lines (or 1 MiB chunks of one long line) between cancellation checks.
pub(super) const VERIFY_CANCEL_EVERY: usize = 4096;

pub(super) const VERIFY_CHUNK_BYTES: usize = 1024 * 1024;

pub(super) enum VerifyReadError {
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
pub(super) struct VerificationLines {
    /// Private-key block ranges over every line read.
    pub(super) key_ranges: Vec<(u32, u32)>,
    /// Neighborhood lines (1-based) that fit the retained-byte budget.
    pub(super) retained: std::collections::BTreeMap<usize, String>,
    pub(super) retained_bytes: usize,
    pub(super) lines_read: usize,
    /// A line too long to buffer mentions `PRIVATE KEY`, so key-block state
    /// is unknown and nothing read from this source can be trusted.
    pub(super) unclassified: bool,
}

/// Stream up to `limit` lines of `reader`, tracking private-key blocks over
/// all of them and keeping only lines inside `windows` (1-based inclusive),
/// within [`MAX_VERIFY_RETAINED_BYTES`]. Any read failure is an error (a
/// short source just yields fewer lines).
pub(super) fn read_verification_lines(
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

pub(super) enum BoundedLine {
    /// The whole line (with its terminator) is in the buffer.
    Whole,
    /// The line exceeded the cap and was consumed without being kept.
    Overflow { mentions_key: bool },
}

/// Read one line into `buf` if it fits `cap` bytes; otherwise consume it in
/// chunks, noting whether it mentions `PRIVATE KEY`. `None` at end of file.
pub(super) fn read_bounded_line(
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

#[cfg(test)]
mod verification_tests {
    use super::*;
    use crate::tools::cancel::NeverCancel;

    fn hit(line: u32, value: &str) -> octocode_engine::types::TextSearchMatch {
        octocode_engine::types::TextSearchMatch {
            line,
            column: 0,
            value: value.into(),
            count: None,
            kind: None,
            score_hint: None,
            rank: None,
            original_chars: None,
        }
    }

    fn open(source: &std::path::Path) -> std::io::BufReader<std::fs::File> {
        std::io::BufReader::new(std::fs::File::open(source).expect("fixture"))
    }

    fn file_with(
        matches: Vec<octocode_engine::types::TextSearchMatch>,
    ) -> octocode_engine::types::TextSearchFile {
        octocode_engine::types::TextSearchFile {
            path: "source.txt".into(),
            match_count: matches.len() as u32,
            matches,
            source: None,
        }
    }

    #[test]
    fn a_late_hit_retains_only_its_neighborhood_with_exact_coordinates() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let filler = "émoji ✓ filler line that is not near the hit\n".repeat(60_000);
        std::fs::write(&source, format!("{filler}before\nthe hit ✓\nafter\ntail\n"))
            .expect("fixture");
        let hit_line = 60_002;
        let read = read_verification_lines(
            &mut open(&source),
            &[(hit_line - 1, hit_line + 1)],
            hit_line + 1,
            &NeverCancel,
        )
        .unwrap_or_else(|_| panic!("readable"));
        assert_eq!(read.lines_read, hit_line + 1);
        assert_eq!(read.retained.len(), 3);
        assert_eq!(read.retained[&hit_line], "the hit ✓");
        assert!(read.retained_bytes < 64, "{}", read.retained_bytes);
        assert!(read.key_ranges.is_empty());
    }

    #[test]
    fn streamed_key_state_matches_the_whole_file_scan() {
        let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        for content in [
            format!(
                "a\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\nb\n"
            ),
            format!("a\n-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n{body}\n"),
            format!(
                "{}\n-----BEGIN EC PRIVATE KEY-----\n{body}\r\n-----END EC PRIVATE KEY-----\r\n",
                "x".repeat(200_000)
            ),
        ] {
            let dir = tempfile::tempdir().expect("fixture");
            let source = dir.path().join("key.txt");
            std::fs::write(&source, &content).expect("fixture");
            let read = read_verification_lines(&mut open(&source), &[], usize::MAX, &NeverCancel)
                .unwrap_or_else(|_| panic!("readable"));
            assert!(read.retained.is_empty());
            assert!(!read.unclassified);
            assert_eq!(
                read.key_ranges,
                crate::security::private_key_block_line_ranges(&content)
            );
        }
    }

    #[test]
    fn an_unbufferable_line_mentioning_a_private_key_fails_closed() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let giant = format!("{}PRIVATE KEY{}", " ".repeat(200_000), "-".repeat(10));
        std::fs::write(
            &source,
            format!("{giant}\nspacer\nspacer\nspacer\nMIIEpQIBAAKCAQEAinteriorKeyBody\n"),
        )
        .expect("fixture");
        let mut file = file_with(vec![hit(5, "…interiorKeyBo")]);
        let verified = guard_clipped_secrets(
            &mut file,
            &source,
            None,
            0..10,
            &ContentSecurity::new(),
            true,
            &NeverCancel,
        )
        .expect("not cancelled");
        assert_eq!(verified, Verification::Unverified);
        assert_eq!(file.matches[0].value, UNVERIFIED_PLACEHOLDER);
        // The same long line without a key mention is an ordinary line.
        std::fs::write(
            &source,
            format!(
                "{}\nspacer\nspacer\nspacer\nplain text here\n",
                " ".repeat(200_000)
            ),
        )
        .expect("fixture");
        let mut file = file_with(vec![hit(5, "plain text")]);
        assert_eq!(
            guard_clipped_secrets(
                &mut file,
                &source,
                None,
                0..10,
                &ContentSecurity::new(),
                true,
                &NeverCancel
            )
            .expect("not cancelled"),
            Verification::Verified
        );
        assert_eq!(file.matches[0].value, "plain text");
    }

    /// Values from a stored scan are checked only against the bytes the scan
    /// was stored with; other bytes, or no stored digest, mean `Changed`.
    #[test]
    fn a_stored_digest_binds_the_check_to_the_stored_bytes() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let stored = "one\nthe hit\nthree\n";
        std::fs::write(&source, stored).expect("fixture");
        let digest: super::super::manifest::Digest = Sha256::digest(stored.as_bytes()).into();
        let check = |expected| {
            let mut file = file_with(vec![hit(2, "the hit")]);
            let outcome = guard_clipped_secrets(
                &mut file,
                &source,
                expected,
                0..10,
                &ContentSecurity::new(),
                false,
                &NeverCancel,
            )
            .expect("not cancelled");
            (outcome, file.matches[0].value.clone())
        };
        assert_eq!(
            check(Some(Some(digest))),
            (Verification::Verified, "the hit".to_owned())
        );
        assert_eq!(check(Some(None)).0, Verification::Changed);
        std::fs::write(&source, "one\nthe hit\nthre3\n").expect("fixture");
        assert_eq!(check(Some(Some(digest))).0, Verification::Changed);
        assert_eq!(check(None).0, Verification::Verified);
    }

    #[test]
    fn a_neighborhood_over_the_retained_budget_fails_closed() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let giant = "a".repeat(MAX_VERIFY_RETAINED_BYTES + 1);
        std::fs::write(&source, format!("{giant}\n")).expect("fixture");
        let mut file = file_with(vec![hit(1, "…aaaa…")]);
        let verified = guard_clipped_secrets(
            &mut file,
            &source,
            None,
            0..10,
            &ContentSecurity::new(),
            false,
            &NeverCancel,
        )
        .expect("not cancelled");
        assert_eq!(verified, Verification::Unverified);
        assert_eq!(file.matches[0].value, OVERSIZED_PLACEHOLDER);
    }

    #[test]
    fn verification_stops_when_cancelled() {
        struct Cancelled;
        impl CancellationCheck for Cancelled {
            fn check(&self) -> Result<(), String> {
                Err("Cancelled".into())
            }
        }
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        std::fs::write(&source, "line\n".repeat(VERIFY_CANCEL_EVERY * 2)).expect("fixture");
        let mut file = file_with(vec![hit((VERIFY_CANCEL_EVERY * 2) as u32, "line")]);
        let reason = guard_clipped_secrets(
            &mut file,
            &source,
            None,
            0..10,
            &ContentSecurity::new(),
            false,
            &Cancelled,
        )
        .expect_err("cancelled during the stream");
        assert_eq!(reason, "Cancelled");
    }
}
