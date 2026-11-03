//! A request's decoded, key-block-redacted source and its source facts.

use super::extraction::line_count;
use super::types::*;
use super::view::*;
use crate::execution::cancel::CancellationCheck;
use crate::security::scan::ContentScan;
use crate::tools::source_view::local_fetch_query;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};

/// A returned window shows a private-key block redacted over the whole
/// source before the window was cut.
pub(super) const KEY_BLOCK_NOTE: &str =
    "Redacted private-key block(s) found in the source before selecting the window.";

/// The decoded source, redacted before any window is cut from it.
pub(super) struct Source<'a> {
    pub(super) sha256: String,
    /// Redacted source; unchanged UTF-8 remains borrowed from the input bytes.
    pub(super) text: std::borrow::Cow<'a, str>,
    pub(super) bytes: usize,
    pub(super) total_lines: usize,
    pub(super) offsets: Option<super::extraction::LineOffsets>,
    pub(super) key_blocks_redacted: bool,
    pub(super) match_redacted: bool,
    /// The source lines the source-wide redactions changed (1-based,
    /// ascending), with the placeholders each introduced: literal
    /// `[REDACTED…]` text in the source is not a redaction.
    pub(super) redacted_lines: Vec<(usize, usize)>,
    pub(super) warnings: Vec<String>,
}

impl Source<'_> {
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

/// The `[REDACTED…]` placeholders `text` shows.
pub(super) fn placeholders(text: &str) -> usize {
    text.matches(octocode_engine::security::REDACTION_PREFIX)
        .count()
}

/// Placeholders `redacted` holds beyond the literal ones of `raw`.
pub(super) fn placeholders_added(raw: &str, redacted: &str) -> usize {
    placeholders(redacted).saturating_sub(placeholders(raw))
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
pub(crate) struct Prepared<'a> {
    /// The decoded text, private-key blocks redacted.
    pub(super) keyed: std::borrow::Cow<'a, str>,
    /// The lines the key-block pass changed, with the placeholders each
    /// gained (empty when it changed nothing).
    pub(super) key_block_lines: Vec<(usize, usize)>,
    pub(super) key_blocks_redacted: bool,
    /// Bytes of the source as read (a leading BOM and the encoding
    /// fallback change the decoded text, not the file's size).
    pub(super) raw_bytes: usize,
    pub(super) warning: Option<&'static str>,
}

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
            warning: decoded.warning,
        }
    }
}

/// Prepare this request's source. No decoded or redacted source survives it.
pub(crate) fn prepared<'a>(
    bytes: &'a [u8],
    interior: bool,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<Prepared<'a>, String> {
    cancel.check()?;
    Ok(Prepared::of(bytes, interior, security))
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
    let key_blocks_redacted = prepared.key_blocks_redacted;
    // Sanitize before selection: match probes cannot confirm secrets, and a
    // range cannot detach a secret value from its identifying source context.
    // Both security passes preserve source line counts.
    let mut redacted = prepared.key_block_lines;
    let (text, match_redacted) =
        if local_fetch_query::has_matches(q) || local_fetch_query::has_ranges(q) {
            let (text, changed) = security
                .redact_source_lines(&prepared.keyed, source_path)
                .map_err(|(code, message)| {
                    Box::new(LocalFetchResult::error(
                        local_fetch_query::path(q).to_owned(),
                        code,
                        message,
                    ))
                })?;
            if changed {
                add_redacted_lines(&mut redacted, &redacted_lines(&prepared.keyed, &text));
            }
            let text = match text {
                std::borrow::Cow::Borrowed(_) => prepared.keyed,
                std::borrow::Cow::Owned(text) => std::borrow::Cow::Owned(text),
            };
            (text, changed)
        } else {
            (prepared.keyed, false)
        };
    let offsets = (local_fetch_query::has_matches(q) || local_fetch_query::has_ranges(q))
        .then(|| super::extraction::LineOffsets::new(&text));
    let total_lines = offsets
        .as_ref()
        .map_or_else(|| line_count(&text), super::extraction::LineOffsets::len);
    Ok(Source {
        sha256,
        text,
        bytes: prepared.raw_bytes,
        total_lines,
        offsets,
        key_blocks_redacted,
        match_redacted,
        redacted_lines: redacted,
        warnings: prepared.warning.map(str::to_owned).into_iter().collect(),
    })
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
