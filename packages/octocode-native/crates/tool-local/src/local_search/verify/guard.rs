//! The secret check of selected values against their source, and the
//! digest of what was read.

use super::*;
use crate::local_search::sources::{Digest, Expected};

/// What a check's read of a source proves of the values it settles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::local_search) enum Proof {
    /// The bytes the values were made from: the check's judgment is theirs.
    Same,
    /// Not the bytes a stored scan holds: nothing of the file may be shown.
    Changed,
    /// Not the bytes a fresh scan made its values from: the check judged
    /// other text than they show.
    Other,
}

/// What a check's read of bytes that hash to `digest` proves. A stored
/// scan's file must hash to its stored digest (`expected`). A fresh scan's
/// values were made from the bytes its search read, which it hashed
/// (`scanned`) and which every value it rebuilt was proved against
/// ([`super::lines::visit_values`]): only a read of those bytes judges them.
/// A fresh scan that bound its values to no digest has no bytes a read
/// could prove them against: every read is of other bytes.
pub(in crate::local_search) fn proof(
    expected: Expected,
    scanned: Option<Digest>,
    digest: Digest,
) -> Proof {
    match expected {
        Expected::Stored(stored) if stored != Some(digest) => Proof::Changed,
        Expected::Fresh if scanned != Some(digest) => Proof::Other,
        Expected::Stored(_) | Expected::Fresh => Proof::Same,
    }
}

/// Settles `shown`, rows of `source` that no read of it proved: the check
/// read other bytes than their values were made from, or none. Each shows
/// what the shared redactor makes of its own text, judged as text of
/// `source` ([`octocode_runtime_base::security::row_shown_unchecked`]), and one that then shows a
/// placeholder is noted in `hidden`: no read says which of its placeholders
/// the source spells.
pub(in crate::local_search) fn unproven(
    shown: &mut [octocode_engine::types::TextSearchMatch],
    source: &std::path::Path,
    (security, hidden): (&ContentSecurity, &HiddenRows),
) -> Verification {
    for matched in shown {
        matched.value = octocode_runtime_base::security::row_shown_unchecked(
            security,
            &matched.value,
            Some(source),
        );
        if octocode_runtime_base::security::placeholders(&matched.value) > 0 {
            hidden.note(matched);
        }
    }
    Verification::Unverified
}

/// Security: a matchOnly span can select a fragment of a secret that no
/// longer matches a secret pattern on its own. For each
/// shown match, sanitize the full source lines the value was cut from; a value
/// line not literally present in that sanitized text overlapped a redaction and
/// is replaced (placeholder for spans, the sanitized match line otherwise).
/// Private-key blocks are detected from the whole file prefix, streamed once
/// without retaining it; only the shown matches' neighborhoods are kept.
///
/// The whole source is hashed while it is read ([`proof`]): a judgment is of
/// the values only when the bytes read are the ones they were made from.
/// With `expected` (values from a stored scan), bytes that differ from the
/// stored digest, or a missing digest, return `Changed` and the values must
/// not be shown. A fresh scan's values made from other bytes than the read
/// are judged alone ([`unproven`]).
///
/// Fails closed: when the source cannot be re-read or no longer holds a shown
/// line, the value is replaced by a placeholder and the result is
/// `Unverified`. Cancellation stops the read and returns the reason.
pub(in crate::local_search) fn guard_source_secrets(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    expected: Expected,
    shown: std::ops::Range<usize>,
    check: &ValueCheck<'_, impl CancellationCheck>,
) -> Result<Verification, String> {
    // A source a read streams instead of judging whole shows the lines that
    // read serves: one decision for a file of any size.
    if let Ok((opened, meta)) = octocode_engine::text::open_regular(source)
        && meta.len() > check.security.limits().whole_source_bytes()
    {
        return super::large::guard_large(file, source, (&opened, &meta), expected, shown, check);
    }
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
pub(in crate::local_search) struct ValueCheck<'a, C> {
    pub(in crate::local_search) security: &'a ContentSecurity,
    pub(in crate::local_search) shape: RowShape,
    pub(in crate::local_search) cancel: &'a C,
    /// The stored scan's judged windows, reused for a source whose read
    /// proved the stored digest (`None`: judge every window afresh).
    pub(in crate::local_search) judged: Option<&'a settle::JudgedWindows>,
    /// Where the check notes each shown row whose value shows text the
    /// redaction wrote.
    pub(in crate::local_search) hidden: &'a HiddenRows,
}

/// What a shown row's value holds of its source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::local_search) enum RowShape {
    /// A matchOnly span of its match.
    Span,
    /// Its match's lines with the context the search joined around them.
    Lines(octocode_engine::types::SearchContext),
}

impl RowShape {
    /// The shape `query`'s rows have: its view, and the context the search
    /// resolved for it.
    pub(in crate::local_search) fn of(query: &LocalSearchQuery) -> Self {
        if local_search_query::view(query) == local_search_query::View::MatchOnly {
            Self::Span
        } else {
            Self::Lines(local_search_query::context(query))
        }
    }
}

/// [`guard_source_secrets`] over the reader `open` returns, opened only
/// when a shown value needs its source lines.
pub(in crate::local_search) fn guard_read<R: std::io::Read>(
    file: &mut octocode_engine::types::TextSearchFile,
    source: &std::path::Path,
    open: impl Fn() -> std::io::Result<R>,
    expected: Expected,
    shown: std::ops::Range<usize>,
    check: &ValueCheck<'_, impl CancellationCheck>,
) -> Result<Verification, String> {
    let ValueCheck {
        security,
        shape,
        cancel,
        judged: known,
        hidden,
    } = *check;
    if expected == Expected::Stored(None) {
        return Ok(Verification::Changed);
    }
    let scanned = file.source.map(|searched| searched.digest);
    let end = shown.end.min(file.matches.len());
    let start = shown.start.min(end);
    let shown = &mut file.matches[start..end];
    // 1-based inclusive source lines each shown value needs re-sanitized.
    // Each row's lines, and the window around them its value is judged in.
    let placed = shown
        .iter()
        .map(|m| settle::row_lines(shape, m))
        .collect::<Vec<_>>();
    let windows = shown
        .iter()
        .zip(&placed)
        .map(|(m, lines)| {
            let (first, span) = lines.map_or((m.line as usize, 1), |l| (l.first, l.span));
            (first.saturating_sub(span).max(1), first + 2 * span - 1)
        })
        .collect::<Vec<_>>();
    if windows.is_empty() {
        return Ok(Verification::Verified);
    }
    // A source an earlier page of this walk judged whole is not read for a
    // scan again: its bytes are proved to be the ones judged, and each row is
    // cut from that judgment.
    if let (Some(known), Expected::Stored(Some(stored))) = (known, expected)
        && let Some(verdict) = known.whole_verdict(source, stored)
    {
        let proven = open().map_err(VerifyReadError::from).and_then(|opened| {
            drain_digest(
                std::io::BufReader::new(HashingReader {
                    inner: opened,
                    hasher: Some(octocode_engine::digest::HashWriter::default()),
                }),
                cancel,
            )
        });
        return match proven {
            Ok(Some(digest)) if digest == stored => {
                hidden.judged_whole();
                Ok(settle::settle_judged(
                    shown,
                    &placed,
                    shape,
                    &verdict,
                    (usize::MAX, hidden),
                    (source, security),
                ))
            }
            Ok(_) => Ok(Verification::Changed),
            Err(VerifyReadError::Io) => Ok(unproven(shown, source, (security, hidden))),
            Err(VerifyReadError::Cancelled(reason)) => Err(reason),
        };
    }
    // A source a search of this process read and judged whole, as these same
    // bytes at this path, is proved by its digest: its rows are cut from
    // that judgment, and no line of it is read for a scan again.
    if let Some((digest, kept)) = kept_whole(&open, source, security, cancel) {
        return Ok(match proof(expected, scanned, digest) {
            Proof::Same => kept.settle(shown, &placed, check, source),
            Proof::Changed => Verification::Changed,
            Proof::Other => unproven(shown, source, (security, hidden)),
        });
    }
    // Every source is read whole and judged as a read of it is; one with a
    // line too long to hold is judged in its rows' windows.
    let read = open().map_err(VerifyReadError::from).and_then(|opened| {
        // Hashed for a fresh scan too: its judgment is kept under the
        // digest, for the stored scan's later pages.
        let mut reader = std::io::BufReader::new(HashingReader {
            inner: opened,
            hasher: Some(octocode_engine::digest::HashWriter::default()),
        });
        let read = match decoded::byte_order(&mut reader)? {
            Some(order) => {
                let mut text = std::io::BufReader::new(decoded::Utf16Text::new(&mut reader, order));
                read_verification_lines(&mut text, &windows, usize::MAX, true, cancel)?
            }
            None => read_verification_lines(&mut reader, &windows, usize::MAX, true, cancel)?,
        };
        let digest = drain_digest(reader, cancel)?;
        Ok((read, digest))
    });
    let (read, proven) = match read {
        Ok((read, proven)) => match proven.map(|digest| proof(expected, scanned, digest)) {
            Some(Proof::Changed) => return Ok(Verification::Changed),
            Some(Proof::Same) | None if !read.unclassified => (read, proven),
            Some(Proof::Same | Proof::Other) | None => {
                return Ok(unproven(shown, source, (security, hidden)));
            }
        },
        Err(VerifyReadError::Io) => return Ok(unproven(shown, source, (security, hidden))),
        Err(VerifyReadError::Cancelled(reason)) => return Err(reason),
    };
    let whole = settle::whole_source_scan(&read, source, security, known.zip(proven));
    if whole.is_some() {
        hidden.judged_whole();
    }
    if let (Some(verdict), Some(digest)) = (&whole, proven) {
        keep_whole(verdict, read.lines_read, &digest, source, security);
    }
    Ok(settle::settle_rows(
        shown,
        &windows,
        &settle::Judged { read: &read, whole },
        source,
        security,
        (&placed, shape, known.zip(proven)),
        hidden,
    ))
}

/// A whole source's judgment taken from what an earlier search kept of it.
struct KeptWhole {
    verdict: settle::Verdict,
    /// How many lines the source has.
    lines: usize,
}

impl KeptWhole {
    /// Settle `shown`, rows of `source` whose values were made from the
    /// bytes this judgment is of ([`proof`]): each is cut from the judgment,
    /// as on the search that made it.
    fn settle(
        self,
        shown: &mut [octocode_engine::types::TextSearchMatch],
        placed: &[Option<settle::RowLines>],
        check: &ValueCheck<'_, impl CancellationCheck>,
        source: &std::path::Path,
    ) -> Verification {
        check.hidden.judged_whole();
        settle::settle_judged(
            shown,
            placed,
            check.shape,
            &self.verdict,
            (self.lines, check.hidden),
            (source, check.security),
        )
    }
}

/// Keep what this read found of a source it held whole, classified and
/// judged (`verdict`), as `lines` lines of bytes that hash to `digest`: its
/// rows are settled by its line count and its judgment alone, which the next
/// search of these bytes takes by their digest ([`kept_whole`]).
fn keep_whole(
    verdict: &settle::Verdict,
    lines: usize,
    digest: &crate::local_search::sources::Digest,
    source: &std::path::Path,
    security: &ContentSecurity,
) {
    let found = octocode_tools_shared::judged_rows::WholeSource {
        lines,
        redacted_text: verdict
            .as_ref()
            .map(|judged| octocode_engine::digest::sha256_digest(judged.raw().as_bytes())),
    };
    octocode_tools_shared::judged_rows::keep_whole_source(digest, source, security, found);
}

/// The judgment an earlier search of this process made of the whole source
/// `open` reads, with the digest of its bytes: `None` when none is kept for
/// these bytes at this path, or the source cannot be read here (the read
/// that follows then fails as it would have).
///
/// A source judged clean needs nothing but its line count. One the pass
/// changed is cut from its judged text, which is kept nowhere: the text is
/// read again from the bytes just hashed, as the lines of a UTF-8 source
/// whose line breaks are `\n` join, and is the text judged exactly when it
/// hashes to the digest kept of that text. Any other source is read line by
/// line, as on its first search.
fn kept_whole<R: std::io::Read>(
    open: &impl Fn() -> std::io::Result<R>,
    source: &std::path::Path,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
) -> Option<(crate::local_search::sources::Digest, KeptWhole)> {
    use std::io::Read;
    // No more than a source judged whole holds: a longer one is not kept.
    let limit = security.limits().whole_source_bytes();
    let mut opened = open().ok()?.take(limit.saturating_add(1));
    let mut bytes = Vec::new();
    let mut chunk = vec![0_u8; VERIFY_CHUNK_BYTES];
    loop {
        cancel.check().ok()?;
        let read = opened.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    if u64::try_from(bytes.len()).map_or(true, |len| len > limit) {
        return None;
    }
    let digest = octocode_engine::digest::sha256_digest(&bytes);
    let kept = octocode_tools_shared::judged_rows::whole_source(&digest, source, security)?;
    let verdict = match kept.redacted_text {
        None => None,
        Some(judged_text) => {
            let text = std::str::from_utf8(&bytes).ok()?;
            let text = text.strip_suffix('\n').unwrap_or(text);
            if octocode_engine::digest::sha256_digest(text.as_bytes()) != judged_text {
                return None;
            }
            let judged =
                octocode_tools_shared::judged_rows::judged(text, &judged_text, source, security);
            // A pass that replaces nothing of this text was never kept as one
            // that did: read the source as a first search does.
            if !judged.is_redacted() {
                return None;
            }
            Some(std::sync::Arc::new(judged))
        }
    };
    Some((
        digest,
        KeptWhole {
            verdict,
            lines: kept.lines,
        },
    ))
}

/// Outcome of [`guard_source_secrets`] for one file's shown matches.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::local_search) enum Verification {
    /// Every shown value was checked against the source.
    Verified,
    /// Some value was replaced by a placeholder because it could not be checked.
    Unverified,
    /// The source is not the stored scan's bytes; nothing may be shown.
    Changed,
}

/// Passes reads through, hashing them when a digest is wanted.
pub(in crate::local_search) struct HashingReader<R> {
    pub(in crate::local_search) inner: R,
    pub(in crate::local_search) hasher: Option<octocode_engine::digest::HashWriter>,
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
pub(in crate::local_search) fn drain_digest<R: std::io::Read>(
    mut reader: std::io::BufReader<HashingReader<R>>,
    cancel: &impl CancellationCheck,
) -> Result<Option<crate::local_search::sources::Digest>, VerifyReadError> {
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

/// Bytes of a line outside every neighborhood buffered to classify it; a
/// longer line is consumed in chunks without being stored.
pub(in crate::local_search) const MAX_PROBE_LINE_BYTES: usize = 64 * 1024;

/// Lines (or 1 MiB chunks of one long line) between cancellation checks.
pub(in crate::local_search) const VERIFY_CANCEL_EVERY: usize = 4096;

pub(in crate::local_search) const VERIFY_CHUNK_BYTES: usize = 1024 * 1024;
