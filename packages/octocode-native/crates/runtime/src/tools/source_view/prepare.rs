//! A read's decoded, key-block-redacted source, memoized for large files.

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
pub(super) struct Source {
    pub(super) sha256: String,
    /// The redacted text; a non-match read shares its preparation's copy.
    pub(super) text: std::sync::Arc<String>,
    pub(super) chars: usize,
    pub(super) bytes: usize,
    pub(super) total_lines: usize,
    pub(super) key_blocks_redacted: bool,
    pub(super) match_redacted: bool,
    /// The source lines the source-wide redactions changed (1-based,
    /// ascending), with the placeholders each introduced: literal
    /// `[REDACTED…]` text in the source is not a redaction.
    pub(super) redacted_lines: Vec<(usize, usize)>,
    pub(super) warnings: Vec<String>,
}

impl Source {
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
/// memo keeps it across calls.
pub(crate) struct Prepared {
    /// The decoded text, private-key blocks redacted.
    pub(super) keyed: std::sync::Arc<String>,
    /// The lines the key-block pass changed, with the placeholders each
    /// gained (empty when it changed nothing).
    pub(super) key_block_lines: Vec<(usize, usize)>,
    pub(super) key_blocks_redacted: bool,
    /// Bytes of the source as read (a leading BOM and the encoding
    /// fallback change the decoded text, not the file's size).
    pub(super) raw_bytes: usize,
    pub(super) chars: usize,
    pub(super) total_lines: usize,
    pub(super) warning: Option<&'static str>,
}

impl Prepared {
    pub(super) fn of(bytes: &[u8], interior: bool, security: &impl ContentScan) -> Self {
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
            chars: raw.encode_utf16().count(),
            total_lines: line_count(&raw),
            raw_bytes: bytes.len(),
            // The pass keeps the line count, so its changes map onto lines.
            key_block_lines: if changed {
                redacted_lines(&raw, &keyed)
            } else {
                Vec::new()
            },
            key_blocks_redacted: changed,
            keyed: std::sync::Arc::new(keyed),
            warning: decoded.warning,
        }
    }
}

/// Sources at least this large are prepared once per version for every
/// page a runtime serves; smaller ones are cheap to prepare again.
pub(crate) const PREPARED_MIN_BYTES: usize = 256 * 1024;

/// A prepared source's key: its path, byte digest and whether the bytes
/// begin inside the file ([`SourceFacts::interior`]).
type PreparedKey = (std::path::PathBuf, String, bool);

/// Prepared sources by [`PreparedKey`] (the digest names the bytes, so an
/// entry never goes stale). One process holds one content scanner.
static PREPARED: octocode_io::memo::Memo<PreparedKey, Prepared> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 4,
        max_weight: 96 * 1024 * 1024,
        keep_newest: false,
    });

/// One preparation per large version at a time: rows of a batch reading it
/// concurrently wait for the first and take its value from the slot.
pub(crate) static PREPARING: octocode_io::flight::KeyedFlight<
    PreparedKey,
    Option<std::sync::Arc<Prepared>>,
> = octocode_io::flight::KeyedFlight::transient();

/// The prepared source of `bytes` (digest `sha256`) at `source_path`: from
/// the memo when an earlier page (or a concurrent row) prepared the same
/// version. A row waiting for a concurrent row's preparation stops when
/// `cancel` fails.
pub(crate) fn prepared(
    bytes: &[u8],
    sha256: &str,
    interior: bool,
    source_path: &std::path::Path,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<std::sync::Arc<Prepared>, String> {
    if bytes.len() < PREPARED_MIN_BYTES {
        return Ok(std::sync::Arc::new(Prepared::of(bytes, interior, security)));
    }
    // Rows prepare in blocking row workers, never on an async worker.
    PREPARING.memoized_blocking(
        (source_path.to_path_buf(), sha256.to_owned(), interior),
        |key| PREPARED.get(key).map(|hit| hit.value),
        |key| {
            let prepared = Prepared::of(bytes, interior, security);
            let weight = prepared.keyed.len()
                + prepared.key_block_lines.len() * 2 * std::mem::size_of::<usize>();
            PREPARED.insert(key, prepared, weight, None)
        },
        || cancel.check(),
    )
}

/// Decode and redact the source; a stale snapshot, a cancel or a failed
/// redaction ends the read. The decoded, key-redacted text of a large source
/// is prepared once per version (see [`prepared`]).
pub(super) fn prepare_source(
    q: &LocalFetchQuery,
    bytes: &[u8],
    sha256: String,
    facts: &SourceFacts,
    source_path: &std::path::Path,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> Result<Source, Box<LocalFetchResult>> {
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
    let prepared = prepared(
        bytes,
        &sha256,
        facts.interior,
        source_path,
        security,
        cancel,
    )
    .map_err(cancelled)?;
    let key_blocks_redacted = prepared.key_blocks_redacted;
    // Sanitize before selection: match probes cannot confirm secrets, and a
    // range cannot detach a secret value from its identifying source context.
    // The shared sanitizer preserves line counts and memoizes source versions.
    let (text, match_redacted) =
        if local_fetch_query::has_matches(q) || local_fetch_query::has_ranges(q) {
            let (text, redacted) = security
                .redact_source_lines(&prepared.keyed, source_path, &sha256)
                .map_err(|(code, message)| {
                    Box::new(LocalFetchResult::error(
                        local_fetch_query::path(q).to_owned(),
                        code,
                        message,
                    ))
                })?;
            (std::sync::Arc::new(text), redacted)
        } else {
            (std::sync::Arc::clone(&prepared.keyed), false)
        };
    // Both passes keep the line count, so their changes map onto lines.
    let mut redacted = prepared.key_block_lines.clone();
    if match_redacted {
        add_redacted_lines(&mut redacted, &redacted_lines(&prepared.keyed, &text));
    }
    Ok(Source {
        sha256,
        text,
        chars: prepared.chars,
        bytes: prepared.raw_bytes,
        total_lines: prepared.total_lines,
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
