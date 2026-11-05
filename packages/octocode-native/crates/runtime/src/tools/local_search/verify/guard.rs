//! The secret check of selected values against their source, and the
//! digest of what was read.

use super::*;
use crate::tools::local_search::sources::Expected;
use octocode_engine::security::WholeSourcePatterns;

/// Security: a matchOnly span can select a fragment of a secret that no
/// longer matches a secret pattern on its own. For each
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
pub(in crate::tools::local_search) fn guard_source_secrets(
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

/// [`guard_source_secrets`] over the reader `open` returns, opened only
/// when a shown value needs its source lines.
pub(in crate::tools::local_search) fn guard_read<R: std::io::Read>(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    open: impl Fn() -> std::io::Result<R>,
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
    let across = WholeSourcePatterns::for_path(&source.to_string_lossy());
    let pass = |across| {
        open().map_err(VerifyReadError::from).and_then(|opened| {
            let mut reader = std::io::BufReader::new(HashingReader {
                inner: opened,
                hasher: matches!(expected, Expected::Stored(_))
                    .then(octocode_engine::digest::HashWriter::default),
            });
            let read = match decoded::byte_order(&mut reader)? {
                Some(order) => {
                    let mut text =
                        std::io::BufReader::new(decoded::Utf16Text::new(&mut reader, order));
                    read_verification_lines(&mut text, &windows, last_line, across, cancel)?
                }
                None => read_verification_lines(&mut reader, &windows, last_line, across, cancel)?,
            };
            let digest = drain_digest(reader, cancel)?;
            Ok((read, digest))
        })
    };
    let read = pass(across).and_then(|(read, digest)| {
        if read.source_wide {
            return Ok((read, digest));
        }
        // A window shows a value only the rest of its source can judge, or follows
        // a key's armor, whose judgment a read makes from the whole file.
        let named = read
            .retained
            .values()
            .any(|line| WholeSourcePatterns::shown_by(line).is_some())
            && decoded::names_an_object(std::io::BufReader::new(open()?), cancel)?;
        if !named && read.key_ranges.is_empty() {
            return Ok((read, digest));
        }
        let (mut whole, digest) = pass(Some(WholeSourcePatterns::any_file().and(across)))?;
        whole.source_wide |= named;
        Ok((whole, digest))
    });
    let judged = read.and_then(|(read, digest)| {
        // A source too large to scan whole fails closed only where its windows
        // cannot decide; a key block's own rows are settled by its ranges below.
        let scan = (read.source_wide || !read.key_ranges.is_empty())
            .then(|| settle::whole_source_scan(&read, source, security))
            .flatten();
        let none = std::collections::BTreeSet::new();
        if read.unclassified || !read.source_wide || scan.is_some() {
            return Ok((read, digest, scan, none));
        }
        // What its structure says then decides each row. Only when that outruns the
        // memory budget do its rows stay withheld.
        let walked = exposed::walked_pass(&open, expected, &windows, last_line, source, cancel)?;
        Ok(match walked {
            Some(walked) => (walked.read, walked.digest, None, walked.masked),
            None => (read, digest, None, none),
        })
    });
    let (read, scan, masked) = match judged {
        Ok((_, Some(digest), ..)) if expected != Expected::Stored(Some(digest)) => {
            return Ok(Verification::Changed);
        }
        Ok((read, _, scan, masked)) if !read.unclassified => (read, scan, masked),
        Ok(_) | Err(VerifyReadError::Io) => {
            for matched in shown.iter_mut() {
                matched.value = UNVERIFIED_PLACEHOLDER.to_owned();
            }
            return Ok(Verification::Unverified);
        }
        Err(VerifyReadError::Cancelled(reason)) => return Err(reason),
    };
    let whole = match scan {
        None if !read.source_wide => None,
        scan => Some(settle::WholeScan(scan)),
    };
    Ok(settle::settle_rows(
        shown,
        &windows,
        &settle::Judged {
            read: &read,
            whole,
            masked: &masked,
        },
        source,
        security,
        match_only,
    ))
}

/// Outcome of [`guard_source_secrets`] for one file's shown matches.
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
    pub(in crate::tools::local_search) hasher: Option<octocode_engine::digest::HashWriter>,
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
        .map(octocode_engine::digest::HashWriter::finalize))
}

/// Value shown in place of a match whose source could not be re-read for the
/// source-secret check.
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
