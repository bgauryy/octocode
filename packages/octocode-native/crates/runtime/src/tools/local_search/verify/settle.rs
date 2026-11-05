//! Each shown value settled against what its source's read said: its key block, its
//! whole-source scan, or the lines around it.

use super::guard::{OVERSIZED_PLACEHOLDER, UNVERIFIED_PLACEHOLDER};
use super::lines::VerificationLines;
use super::*;
use std::collections::BTreeSet;

/// A whole source's text as a read of it shows it, `None` when it could not be
/// scanned whole.
pub(super) struct WholeScan(pub(super) Option<String>);

/// What the read of one source said of its shown values.
pub(super) struct Judged<'a> {
    pub(super) read: &'a VerificationLines,
    /// The whole source's scan, when one decides.
    pub(super) whole: Option<WholeScan>,
    /// The lines of `read` a walk of the source masked ([`super::exposed`]).
    pub(super) masked: &'a BTreeSet<usize>,
}

/// Settle `shown`, each with the lines `windows` holds for it.
pub(super) fn settle_rows(
    shown: &mut [octocode_engine::types::TextSearchMatch],
    windows: &[(usize, usize)],
    judged: &Judged<'_>,
    source: &std::path::Path,
    security: &ContentSecurity,
    match_only: bool,
) -> Verification {
    let Judged {
        read,
        whole,
        masked,
    } = judged;
    let mut verified = Verification::Verified;
    for (matched, &(lo, hi)) in shown.iter_mut().zip(windows) {
        let shown_source = read
            .retained
            .get(&(matched.line as usize))
            .map(String::as_str);
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
            matched.value = security
                .sanitize_text(&kept, Some(source))
                .content
                .into_owned();
            if let Some(scan) = &whole
                && !settle_with_whole_source(matched, &scan.0, match_only, shown_source, source)
            {
                verified = Verification::Unverified;
            }
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
        if let Some(scan) = &whole {
            if !settle_with_whole_source(matched, &scan.0, match_only, shown_source, source) {
                verified = Verification::Unverified;
            }
            continue;
        }
        let span = matched.value.lines().count().max(1);
        let (lo, hi) = row_window(&read.retained, (lo, hi.min(read.lines_read)), (line, span));
        // Too long to keep for a second look, the search judged it, and only a
        // window of it or its own span is on the row.
        if read.long_lines.contains(&line) {
            continue;
        }
        let Some(window) = (lo..=hi)
            .map(|n| read.retained.get(&n).map(String::as_str))
            .collect::<Option<Vec<_>>>()
        else {
            matched.value = OVERSIZED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        };
        let joined = window.join("\n");
        let sanitized = security.sanitize_text(&joined, Some(source));
        if !sanitized.has_secrets && !(lo..=hi).any(|at| masked.contains(&at)) {
            continue;
        }
        if crate::security::cut::exposes(&matched.value, &sanitized.content) {
            matched.value = if match_only {
                span_shown(matched, window[line - lo], source)
            } else {
                shown_line(
                    window[line - lo],
                    line - lo,
                    &sanitized.content,
                    source,
                    security,
                )
            };
        }
    }
    verified
}

/// A span that overlapped a secret, as its line shows it: its own bytes, a secret's
/// part of them replaced. A span the line alone cannot say so of goes whole.
fn span_shown(
    matched: &octocode_engine::types::TextSearchMatch,
    line: &str,
    source: &std::path::Path,
) -> String {
    let start = matched.byte_offset as usize;
    let span = start..start + matched.value.len();
    if line.get(span.clone()) != Some(matched.value.as_str()) {
        return "[REDACTED]".to_owned();
    }
    let path = source.to_string_lossy();
    let shown =
        octocode_engine::security::LongLine::judged(line, Some(path.as_ref())).cut(line, span);
    if shown == matched.value {
        return "[REDACTED]".to_owned();
    }
    shown
}

/// The lines a row is judged with: `lo..=hi`, or its own when those are more than
/// the scanner judges at once. A multi-line secret cannot be judged across such a
/// neighbor anyway, and a row must not go with it.
fn row_window(
    retained: &std::collections::BTreeMap<usize, String>,
    (lo, hi): (usize, usize),
    (line, span): (usize, usize),
) -> (usize, usize) {
    let bytes: usize = (lo..=hi)
        .filter_map(|at| retained.get(&at))
        .map(|text| text.len() + 1)
        .sum();
    if bytes > octocode_engine::security::MAX_CONTENT_SIZE {
        (line, (line + span - 1).min(hi))
    } else {
        (lo, hi)
    }
}

/// The line at `at` of a window `clean` is the scan of: as scanned alone, or when
/// only the lines around it make it secret, as the window's scan left it.
fn shown_line(
    line: &str,
    at: usize,
    clean: &str,
    source: &std::path::Path,
    security: &ContentSecurity,
) -> String {
    let alone = security.sanitize_text(line, Some(source)).content;
    if alone != line {
        return alone.into_owned();
    }
    clean
        .split('\n')
        .nth(at)
        .filter(|scanned| !scanned.trim().is_empty())
        .unwrap_or("[REDACTED]")
        .to_owned()
}

/// The whole source's text as a read of it shows it, for a source whose secrets a
/// window of it cannot show ([`VerificationLines::source_wide`]) or with a key's
/// armor in it. `None` when it could not be scanned whole.
pub(super) fn whole_source_scan(
    read: &VerificationLines,
    source: &std::path::Path,
    security: &ContentSecurity,
) -> Option<String> {
    if !read.whole_retained {
        return None;
    }
    let text = read
        .retained
        .values()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    let (keyed, _) = crate::security::redact_private_key_blocks(&text);
    Some(
        security
            .sanitize_text(&keyed, Some(source))
            .content
            .into_owned(),
    )
}

/// Check `matched` against the whole source's `scan`: a value that overlapped a
/// redaction there becomes the lines it came from as the scan left them, or a
/// bare placeholder for a span, or where a multi-line redaction emptied them.
/// False when no scan could run, and the value is withheld.
fn settle_with_whole_source(
    matched: &mut octocode_engine::types::TextSearchMatch,
    scan: &Option<String>,
    match_only: bool,
    source_line: Option<&str>,
    source: &std::path::Path,
) -> bool {
    let Some(clean) = scan else {
        matched.value = OVERSIZED_PLACEHOLDER.to_owned();
        return false;
    };
    if crate::security::cut::exposes(&matched.value, clean) {
        let span = matched.value.lines().count().max(1);
        let lines = clean
            .split('\n')
            .skip((matched.line as usize).saturating_sub(1))
            .take(span)
            .collect::<Vec<_>>()
            .join("\n");
        matched.value = if match_only {
            source_line.map_or_else(
                || "[REDACTED]".to_owned(),
                |line| span_shown(matched, line, source),
            )
        } else if lines.trim().is_empty() {
            "[REDACTED]".to_owned()
        } else {
            lines
        };
    }
    true
}
