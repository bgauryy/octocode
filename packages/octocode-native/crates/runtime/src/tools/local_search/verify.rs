//! Secret safety of shown values: redaction of the scan, the re-read that
//! checks clipped values against their source bytes, and bounded line reads.

use super::executor::*;
use super::types::*;
use super::{cursor::*, layout::*};
use crate::security::ContentSecurity;
use crate::tools::cancel::CancellationCheck;
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

/// Make paths relative to `output_root` and redact secrets in every value.
/// A match on an interior base64 body line of a private key would leak the
/// key even though the match view holds no BEGIN/END marker (the anchored
/// built-in patterns need a complete block). Only when a snippet actually
/// looks like key material is the full file scanned for private-key block
/// ranges; matches whose window intersects a block are redacted. Innocent
/// base64 triggers a scan that finds no block and redacts nothing.
pub(super) fn redact_scan(
    query: &LocalSearchQuery,
    parsed: &mut octocode_engine::types::RipgrepParseResult,
    output_root: &std::path::Path,
    expected_digest: &ExpectedDigest<'_>,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
) -> Result<Redacted, LocalSearchError> {
    let mut redacted = Redacted::new();
    for file in &mut parsed.files {
        if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
            file.path = relative.to_string_lossy().into_owned();
        }
        let source_path = output_root.join(&file.path);
        let key_ranges = if file
            .matches
            .iter()
            .any(|m| crate::security::snippet_may_hold_key_material(&m.value))
        {
            let bytes = std::fs::metadata(&source_path)
                .ok()
                .filter(|meta| meta.len() <= MAX_KEY_SCAN_BYTES)
                .and_then(|_| std::fs::read(&source_path).ok());
            if let (Some(bytes), Some(expected)) = (&bytes, expected_digest(&source_path))
                && expected != Some(<[u8; 32]>::from(Sha256::digest(bytes)))
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
        for matched in &mut file.matches {
            cancel.check().map_err(cancelled)?;
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
            }
        }
    }
    Ok(redacted)
}

/// Re-read the shown files. A page from a stored scan shows each of its
/// files only while that file still hashes to the stored bytes; text views
/// prove it in the secret check, which reads the same bytes. Returns whether
/// some values were redacted because their source could not be re-read.
#[allow(clippy::too_many_arguments)]
pub(super) fn verify_shown(
    query: &LocalSearchQuery,
    parsed: &mut octocode_engine::types::RipgrepParseResult,
    layout: &Layout,
    output_root: &std::path::Path,
    expected_digest: &ExpectedDigest<'_>,
    redacted: &mut Redacted,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
) -> Result<bool, LocalSearchError> {
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
    for (index, rows) in &layout.shown {
        let file = &mut parsed.files[*index];
        cancel.check().map_err(cancelled)?;
        let before = file
            .matches
            .iter()
            .map(|matched| matched.value.clone())
            .collect::<Vec<_>>();
        let source = output_root.join(&file.path);
        match guard_clipped_secrets(
            file,
            &source,
            expected_digest(&source),
            rows.clone(),
            security,
            query.result_view == LocalSearchQueryResultView::MatchOnly,
            cancel,
        )
        .map_err(cancelled)?
        {
            Verification::Verified => {}
            Verification::Unverified => unverified = true,
            Verification::Changed => return Err(changed_since_scan(query)),
        }
        for (matched, before) in file.matches.iter().zip(before) {
            if matched.value != before {
                redacted.insert((file.path.clone(), matched.line, matched.column));
            }
        }
    }
    Ok(unverified)
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
    file: &mut octocode_engine::types::RipgrepFile,
    source: &std::path::Path,
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
    let read = std::fs::File::open(source)
        .map_err(VerifyReadError::from)
        .and_then(|opened| {
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

    fn hit(line: u32, value: &str) -> octocode_engine::types::RipgrepMatch {
        octocode_engine::types::RipgrepMatch {
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
        matches: Vec<octocode_engine::types::RipgrepMatch>,
    ) -> octocode_engine::types::RipgrepFile {
        octocode_engine::types::RipgrepFile {
            path: "source.txt".into(),
            match_count: matches.len() as u32,
            matches,
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
