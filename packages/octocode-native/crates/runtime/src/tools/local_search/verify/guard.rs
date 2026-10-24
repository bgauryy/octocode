//! The secret check of clipped values against their source, and the
//! digest of what was read.

use super::*;
use crate::tools::local_search::sources::Expected;

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
pub(in crate::tools::local_search) fn guard_clipped_secrets(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    expected: Expected,
    shown: std::ops::Range<usize>,
    check: &ValueCheck<'_, impl CancellationCheck>,
) -> Result<Verification, String> {
    guard_read(
        file,
        source,
        || octocode_engine::text::open_regular(source).map(|(file, _)| file),
        expected,
        shown,
        check,
    )
}

/// How shown values are checked against their source: the secret scanner,
/// whether values are matchOnly spans (an exposed one becomes a bare
/// placeholder), and the cancel that stops the read.
pub(in crate::tools::local_search) struct ValueCheck<'a, C> {
    pub(in crate::tools::local_search) security: &'a ContentSecurity,
    pub(in crate::tools::local_search) match_only: bool,
    pub(in crate::tools::local_search) cancel: &'a C,
}

/// [`guard_clipped_secrets`] over the reader `open` returns, opened only
/// when a shown value needs its source lines.
pub(in crate::tools::local_search) fn guard_read<R: std::io::Read>(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    open: impl FnOnce() -> std::io::Result<R>,
    expected: Expected,
    shown: std::ops::Range<usize>,
    check: &ValueCheck<'_, impl CancellationCheck>,
) -> Result<Verification, String> {
    let ValueCheck {
        security,
        match_only,
        cancel,
    } = *check;
    if expected == Expected::Stored(None) {
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
            hasher: matches!(expected, Expected::Stored(_)).then(Sha256::new),
        });
        let read = read_verification_lines(&mut reader, &windows, last_line, cancel)?;
        let digest = drain_digest(reader, cancel)?;
        Ok((read, digest))
    });
    let read = match read {
        Ok((_, Some(digest))) if expected != Expected::Stored(Some(digest)) => {
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
            let kept =
                redact_key_block_lines(matched.line, &matched.value, &read.key_ranges, |n| {
                    read.retained.get(&(n as usize)).map(String::as_str)
                });
            matched.value = security.sanitize_text(&kept, Some(source)).content;
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
        if crate::security::cut::exposes(&matched.value, &sanitized.content) {
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
pub(in crate::tools::local_search) enum Verification {
    /// Every shown value was checked against the source.
    Verified,
    /// Some value was replaced by a placeholder because it could not be checked.
    Unverified,
    /// The source is not the stored scan's bytes; nothing may be shown.
    Changed,
}

/// Passes reads through, hashing them when a digest is wanted.
pub(in crate::tools::local_search) struct HashingReader<R> {
    pub(in crate::tools::local_search) inner: R,
    pub(in crate::tools::local_search) hasher: Option<Sha256>,
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
pub(in crate::tools::local_search) fn drain_digest<R: std::io::Read>(
    mut reader: std::io::BufReader<HashingReader<R>>,
    cancel: &impl CancellationCheck,
) -> Result<Option<crate::tools::local_search::sources::Digest>, VerifyReadError> {
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
pub(in crate::tools::local_search) const UNVERIFIED_PLACEHOLDER: &str =
    "[REDACTED: source unreadable for secret check]";

/// Value shown in place of a match whose source neighborhood exceeds
/// [`MAX_VERIFY_RETAINED_BYTES`].
pub(in crate::tools::local_search) const OVERSIZED_PLACEHOLDER: &str =
    "[REDACTED: source lines too large for secret check]";

/// Source bytes one file's verification may retain for the shown matches'
/// neighborhoods; the rest of the prefix is streamed for key-block state only.
pub(in crate::tools::local_search) const MAX_VERIFY_RETAINED_BYTES: usize = 16 * 1024 * 1024;

/// Bytes of a line outside every neighborhood buffered to classify it; a
/// longer line is consumed in chunks without being stored.
pub(in crate::tools::local_search) const MAX_PROBE_LINE_BYTES: usize = 64 * 1024;

/// Lines (or 1 MiB chunks of one long line) between cancellation checks.
pub(in crate::tools::local_search) const VERIFY_CANCEL_EVERY: usize = 4096;

pub(in crate::tools::local_search) const VERIFY_CHUNK_BYTES: usize = 1024 * 1024;
