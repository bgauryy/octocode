//! A request's decoded, key-block-redacted source and its source facts.

use super::extraction::line_count;
use super::types::*;
use super::view::*;
use crate::source_view::local_fetch_query;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::security::scan::ContentScan;

/// The decoded source, redacted before any window is cut from it.
pub(super) struct Source<'a> {
    pub(super) sha256: String,
    /// Redacted source; unchanged UTF-8 remains borrowed from the input bytes.
    pub(super) text: std::borrow::Cow<'a, str>,
    pub(super) bytes: usize,
    pub(super) total_lines: usize,
    pub(super) offsets: Option<std::sync::Arc<super::extraction::LineOffsets>>,
    pub(super) key_blocks_redacted: bool,
    pub(super) match_redacted: bool,
    /// The source lines the source-wide redactions changed (1-based,
    /// ascending), with the placeholders each introduced: literal
    /// `[REDACTED…]` text in the source is not a redaction.
    pub(super) redacted_lines: Vec<(usize, usize)>,
    pub(super) warnings: Vec<String>,
}

impl Source<'_> {
    /// This source for one more read: its text stays where it is, and what a
    /// read changes on its own view is its own copy.
    pub(super) fn shared(&self) -> Source<'_> {
        Source {
            sha256: self.sha256.clone(),
            text: std::borrow::Cow::Borrowed(&self.text),
            bytes: self.bytes,
            total_lines: self.total_lines,
            offsets: self.offsets.clone(),
            key_blocks_redacted: self.key_blocks_redacted,
            match_redacted: self.match_redacted,
            redacted_lines: self.redacted_lines.clone(),
            warnings: self.warnings.clone(),
        }
    }

    /// Placeholders the source-wide redactions put on `lines` (1-based).
    pub(super) fn redactions_on(&self, lines: impl IntoIterator<Item = usize>) -> usize {
        lines
            .into_iter()
            .filter_map(|line| {
                self.redacted_lines
                    .binary_search_by_key(&line, |(redacted, _)| *redacted)
                    .ok()
            })
            .map(|index| self.redacted_lines[index].1)
            .sum()
    }
}

/// Count `lines` (the placeholders a view's own redaction put on source
/// lines) with the source-wide `redacted` ones.
pub(super) fn add_redacted_lines(redacted: &mut Vec<(usize, usize)>, lines: &[(usize, usize)]) {
    if lines.is_empty() {
        return;
    }
    redacted.extend_from_slice(lines);
    redacted.sort_unstable_by_key(|(line, _)| *line);
    redacted.dedup_by(|later, kept| {
        let same = later.0 == kept.0;
        if same {
            kept.1 += later.1;
        }
        same
    });
}

/// Placeholders `redacted` holds beyond the literal ones of `raw`.
pub(super) fn placeholders_added(raw: &str, redacted: &str) -> usize {
    octocode_runtime_base::security::placeholders(redacted)
        .saturating_sub(octocode_runtime_base::security::placeholders(raw))
}

/// The lines a line-preserving redaction of `raw` into `redacted` changed,
/// with the placeholders each gained.
pub(super) fn redacted_lines(raw: &str, redacted: &str) -> Vec<(usize, usize)> {
    raw.lines()
        .zip(redacted.lines())
        .enumerate()
        .filter(|(_, (raw, redacted))| raw != redacted)
        .map(|(index, (raw, redacted))| (index + 1, placeholders_added(raw, redacted).max(1)))
        .collect()
}

/// A decoded source with its whole-file private-key blocks redacted, and
/// the facts every page of it states. It never holds the key material: the
/// request owns it until its selected view has been rendered.
pub struct Prepared<'a> {
    /// The decoded text, private-key blocks redacted.
    pub(super) keyed: std::borrow::Cow<'a, str>,
    /// The lines the key-block pass changed, with the placeholders each
    /// gained (empty when it changed nothing).
    pub(super) key_block_lines: Vec<(usize, usize)>,
    pub(super) key_blocks_redacted: bool,
    /// Bytes of the source as read (a leading BOM and the encoding
    /// fallback change the decoded text, not the file's size).
    pub(super) raw_bytes: usize,
    /// What the decode changed: an encoding fallback, a dropped BOM.
    pub(super) warnings: Vec<&'static str>,
}

/// The warning of a source whose leading UTF-8 byte-order mark the decode
/// dropped: the served text, its byte offsets and totals start after it.
pub(super) const UTF8_BOM_WARNING: &str = "Dropped a leading UTF-8 byte-order mark (3 bytes): byte offsets and totals count the text after it, while snapshots identify the original file bytes.";

impl<'a> Prepared<'a> {
    pub(super) fn of(bytes: &'a [u8], interior: bool, security: &impl ContentScan) -> Self {
        // Binary detection is a separate heuristic; text that is not UTF-8
        // is decoded (Latin-1 or lossy UTF-8) with a warning, never refused.
        let decoded = if interior {
            octocode_engine::text::Decoded::interior(bytes)
        } else {
            octocode_engine::text::decode(bytes)
        };
        let raw = decoded.text;
        // Redact whole private-key blocks across the full file BEFORE any
        // window/extraction, so a bounded read of an interior body line
        // cannot leak a key whose BEGIN/END markers fall outside the
        // selected window (the anchored built-in patterns only match a
        // complete block in a single view).
        let (keyed, changed) = security.redact_key_blocks(&raw);
        Self {
            raw_bytes: bytes.len(),
            // The pass keeps the line count, so its changes map onto lines.
            key_block_lines: if changed {
                redacted_lines(&raw, &keyed)
            } else {
                Vec::new()
            },
            key_blocks_redacted: changed,
            keyed: match keyed {
                std::borrow::Cow::Borrowed(_) => raw,
                std::borrow::Cow::Owned(text) => std::borrow::Cow::Owned(text),
            },
            warnings: decoded
                .warning
                .into_iter()
                .chain(
                    (!interior
                        && !octocode_engine::text::has_utf16_bom(bytes)
                        && octocode_engine::text::bom_len(bytes) > 0)
                        .then_some(UTF8_BOM_WARNING),
                )
                .collect(),
        }
    }
}

/// What the line-preserving secret pass changed on a keyed text: the
/// redacted text when it changed anything, and the lines it changed with the
/// placeholders each gained.
pub(crate) struct Redaction {
    text: Option<String>,
    lines: Vec<(usize, usize)>,
}

impl Redaction {
    /// A window cut from a view sealed already: nothing left to redact, and
    /// `lines` are the window's lines that sealing changed.
    pub(super) fn sealed_already(lines: &[(usize, usize)]) -> Self {
        Self {
            text: None,
            lines: lines.to_vec(),
        }
    }
}

/// The view every source read serves and cuts its windows from; built by
/// [`sealed_view`].
pub struct SealedView<'a> {
    /// The sealed text; unchanged UTF-8 stays borrowed from the input bytes.
    pub text: std::borrow::Cow<'a, str>,
    pub(crate) key_blocks_redacted: bool,
    /// Whether the secret pass changed any line.
    pub(crate) match_redacted: bool,
    /// The source lines either pass changed (1-based, ascending), with the
    /// placeholders each gained: literal `[REDACTED…]` text in the source is
    /// not a redaction.
    pub(crate) redacted_lines: Vec<(usize, usize)>,
    /// Bytes of the source as read.
    pub(crate) raw_bytes: usize,
    /// What the decode changed: an encoding fallback, a dropped BOM.
    pub(crate) warnings: Vec<&'static str>,
}

impl<'a> Prepared<'a> {
    /// The secret pass over the keyed text, line by line, keeping the line
    /// count. The scanner's refusal (`ContentSecurityLimit` for a source too
    /// large to judge) is the error.
    pub(crate) fn redaction(
        &self,
        path: &std::path::Path,
        security: &impl ContentScan,
    ) -> Result<Redaction, (ErrorCode, String)> {
        Ok(self.redacted(security.redact_source_lines(&self.keyed, path)?))
    }

    /// [`Self::redaction`] of a whole source's text: the same pass, taken
    /// from the one kept of this text at this path
    /// ([`crate::judged_rows::scanned_whole`]) when the scan is the
    /// process's scanner and no key-block pass changed the text (the keyed
    /// text is then the source's decoded text, which that pass is kept by).
    /// A read of bytes already judged cuts from that pass and scans nothing.
    pub(super) fn whole_redaction(
        &self,
        path: &std::path::Path,
        security: &impl ContentScan,
    ) -> Result<Redaction, (ErrorCode, String)> {
        let Some(real) = security.kept().filter(|_| !self.key_blocks_redacted) else {
            return self.redaction(path, security);
        };
        let whole = octocode_runtime_base::security::scan::redact_with(&self.keyed, |text| {
            Ok(crate::judged_rows::scanned_whole(text, path, real))
        })?;
        Ok(self.redacted(whole))
    }

    /// What a line-preserving pass that made `text` of the keyed text
    /// changed (`changed`: anything at all).
    fn redacted(&self, (text, changed): (std::borrow::Cow<'_, str>, bool)) -> Redaction {
        if changed {
            Redaction {
                lines: redacted_lines(&self.keyed, &text),
                text: Some(text.into_owned()),
            }
        } else {
            Redaction {
                text: None,
                lines: Vec::new(),
            }
        }
    }

    /// This source sealed by `redaction`: its text and every line either pass
    /// changed.
    pub(crate) fn sealed(self, redaction: Redaction) -> SealedView<'a> {
        let mut redacted_lines = self.key_block_lines;
        add_redacted_lines(&mut redacted_lines, &redaction.lines);
        SealedView {
            match_redacted: redaction.text.is_some() || !redaction.lines.is_empty(),
            text: redaction.text.map_or(self.keyed, std::borrow::Cow::Owned),
            key_blocks_redacted: self.key_blocks_redacted,
            redacted_lines,
            raw_bytes: self.raw_bytes,
            warnings: self.warnings,
        }
    }
}

/// The sealed view of a whole source's `bytes`: the one text localFetch,
/// ghGetFileContent and lspSearch serve and cut windows from, so a byte
/// offset one of them names is a byte offset of the others.
///
/// Contract:
/// - Decode: UTF-8, else Latin-1 or lossy UTF-8 with a warning, never a
///   refusal; a leading UTF-8 byte-order mark is dropped with
///   [`UTF8_BOM_WARNING`].
/// - Private-key blocks are redacted across the whole source first, so no
///   window cut later shows key material whose BEGIN/END markers fall
///   outside it.
/// - Secrets are then redacted line by line. Both passes keep the line
///   count: line N of the view is line N of the source, and
///   [`SealedView::redacted_lines`] names every line they changed.
/// - Byte offsets into [`SealedView::text`] are the offsets localFetch byte
///   pages and `readProgress` (unit bytes) count; a snapshot pins the raw
///   bytes. Cut windows with `octocode_runtime_base::response::cut` (`clean_cut`,
///   `window_end`, `window_at`), which never split a redaction marker, a CRLF
///   pair or a character; byte pages inside a long line start at that line's
///   first byte, so required pages carry the line exactly once.
/// - The error is the scanner's refusal, `(code, message)`.
///
/// This is the in-memory form. A source too large to hold whole is sealed
/// with the same two passes as it streams (`tools::large_source`,
/// whose windows arrive sealed: see [`Redaction::sealed_already`]).
pub fn sealed_view<'a>(
    bytes: &'a [u8],
    path: &std::path::Path,
    security: &impl ContentScan,
) -> Result<SealedView<'a>, (ErrorCode, String)> {
    let prepared = Prepared::of(bytes, false, security);
    let redaction = prepared.redaction(path, security)?;
    Ok(prepared.sealed(redaction))
}

/// Prepare this request's source. Of what it decodes and redacts, only a whole
/// scanned source outlives the request ([`super::retained::ScannedSource`]).
pub fn prepared<'a>(
    bytes: &'a [u8],
    interior: bool,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<Prepared<'a>, String> {
    cancel.check()?;
    Ok(Prepared::of(bytes, interior, security))
}

/// Largest source a window may be streamed from.
pub const MAX_STREAM_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;

/// Whether every read of the source at `path` is known to be refused, whatever it
/// selects: past the streaming ceiling, or UTF-16 past the whole-file ceiling (it
/// needs a whole-file decode) at `limits`.
pub fn every_read_refused(
    path: &std::path::Path,
    limits: octocode_runtime_base::security::SourceLimits,
) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.len() > MAX_STREAM_SOURCE_BYTES {
        return true;
    }
    if meta.len() <= limits.whole_source_bytes() {
        return false;
    }
    let mut head = [0_u8; 2];
    std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut head))
        .is_ok_and(|()| octocode_engine::text::has_utf16_bom(&head))
}

/// Why `q` selects nothing from `keyed` whatever the secret scan changes: a
/// pattern that does not compile, or ranges that all start past the end (the
/// scan keeps the line count). The error is the one extraction gives.
fn refusal_before_scan(q: &LocalFetchQuery, keyed: &str) -> Option<ExtractionError> {
    use octocode_engine::search::pcre2_ranges::{Pcre2RangesError, pcre2_find_ranges};
    use octocode_engine::search::rust_ranges::rust_find_ranges;
    let does_not_compile = |pattern: &&str| {
        let insensitive = !local_fetch_query::case_sensitive_for(q, pattern);
        if local_fetch_query::is_pcre2(q) {
            matches!(
                pcre2_find_ranges(pattern, insensitive, "", 1),
                Err(Pcre2RangesError::InvalidPattern(_))
            )
        } else {
            rust_find_ranges(pattern, insensitive, "", 1).is_err()
        }
    };
    if local_fetch_query::is_regex(q)
        && local_fetch_query::match_strings(q)
            .iter()
            .any(does_not_compile)
    {
        return super::extraction::match_spans(q, "")
            .err()
            .filter(|error| error.code == ErrorCode::InvalidPattern);
    }
    let requested = local_fetch_query::line_ranges(q);
    let total = line_count(keyed);
    if requested.is_empty() || requested.iter().any(|range| range.start <= total) {
        return None;
    }
    let lines = super::extraction::LineOffsets::new(keyed);
    super::extraction::extract_indexed(q, keyed, &lines).err()
}

/// Decode and redact the source; a stale snapshot, a cancel or a failed
/// redaction ends the read. The decoded, key-redacted text of a large source
/// stays owned by this request (see [`prepared`]).
pub(super) fn prepare_source<'a>(
    q: &LocalFetchQuery,
    bytes: &'a [u8],
    sha256: String,
    facts: &SourceFacts,
    source_path: &std::path::Path,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<Source<'a>, Box<LocalFetchResult>> {
    if let Some(expected) = local_fetch_query::snapshot(q)
        && expected != sha256
    {
        return Err(Box::new(stale_snapshot(q)));
    }
    let cancelled = |e| {
        Box::new(LocalFetchResult::error(
            local_fetch_query::path(q).to_owned(),
            ErrorCode::Cancelled,
            e,
        ))
    };
    cancel.check().map_err(cancelled)?;
    let prepared = prepared(bytes, facts.interior, security, cancel).map_err(cancelled)?;
    let scan_refused = |(code, message)| {
        let mut result =
            LocalFetchResult::error(local_fetch_query::path(q).to_owned(), code, message);
        if code == ErrorCode::ContentSecurityLimit {
            result.terminal_limit(PartialReason::SecuritySelectedView);
            one_line_recovery(q, &prepared.keyed, &mut result);
        }
        Box::new(result)
    };
    let selects = local_fetch_query::has_matches(q) || local_fetch_query::has_ranges(q);
    if selects && let Some(error) = refusal_before_scan(q, &prepared.keyed) {
        // A scanner refuses a source by its longest line, and that refusal
        // comes first.
        let longest = prepared
            .keyed
            .split('\n')
            .max_by_key(|line| line.len())
            .unwrap_or_default();
        security
            .sanitize(longest, source_path)
            .map_err(scan_refused)?;
        return Err(Box::new(extraction_error(
            q,
            error,
            line_count(&prepared.keyed),
        )));
    }
    // Redact before anything selects, reduces or cuts: match probes cannot
    // confirm secrets, a range cannot detach a secret value from its
    // identifying source context, a compact or outline view reduces redacted
    // text, and every page window is cut from the redacted view (its byte
    // offsets count that view). Both security passes preserve source line
    // counts. A walk redacts once: its kept view replays (`retained`).
    let redaction = if let Some(origin) = &facts.byte_origin {
        // A window of a sealed view is redacted already.
        Redaction::sealed_already(&origin.redacted)
    } else {
        let redaction = if facts.local_file {
            prepared.whole_redaction(source_path, security)
        } else {
            prepared.redaction(source_path, security)
        };
        redaction.map_err(scan_refused)?
    };
    let SealedView {
        text,
        key_blocks_redacted,
        match_redacted,
        redacted_lines: redacted,
        raw_bytes,
        warnings,
    } = prepared.sealed(redaction);
    let offsets = selects.then(|| std::sync::Arc::new(super::extraction::LineOffsets::new(&text)));
    let total_lines = offsets
        .as_ref()
        .map_or_else(|| line_count(&text), |offsets| offsets.len());
    Ok(Source {
        sha256,
        text,
        bytes: raw_bytes,
        total_lines,
        offsets,
        key_blocks_redacted,
        match_redacted,
        redacted_lines: redacted,
        warnings: warnings.into_iter().map(str::to_owned).collect(),
    })
}

/// A source too large for the secret scanner offers its first selected line
/// on its own, unless the read already selects one line.
fn one_line_recovery(q: &LocalFetchQuery, keyed: &str, result: &mut LocalFetchResult) {
    let total_lines = line_count(keyed);
    result.total_lines = Some(total_lines);
    if total_lines > 1
        && (local_fetch_query::start_line(q).is_none()
            || local_fetch_query::start_line(q) != local_fetch_query::end_line(q))
    {
        let line = local_fetch_query::start_line(q).unwrap_or(1);
        result.next = Some(NextCalls {
            read_bounded_lines: single_line_query(q, line).map(FetchCall::related),
            ..NextCalls::default()
        });
    }
}

/// A failed selection retains its typed error. Ranges past the file end
/// offer a related read of the last window.
pub(crate) fn extraction_error(
    q: &LocalFetchQuery,
    error: ExtractionError,
    total_lines: usize,
) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        error.code,
        error.message(),
    );
    let repair = match error.code {
        ErrorCode::InvalidInput => {
            if let Some((start, end)) = local_fetch_query::last_window_span(q, total_lines) {
                let mut query = q.clone();
                if let Err(error) = local_fetch_query::set_line_span(&mut query, start, end) {
                    return LocalFetchResult::error(
                        local_fetch_query::path(q).to_owned(),
                        ErrorCode::ExecutionFailed,
                        error.to_string(),
                    );
                }
                Some(FetchCall::related(query))
            } else {
                None
            }
        }
        _ => None,
    };
    result.next = repair.map(|repair| NextCalls {
        repair: Some(repair),
        ..NextCalls::default()
    });
    result
}
