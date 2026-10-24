//! The secret scan of one returned page and the warnings it raises.

use super::extraction::line_count;
use super::pagination::{SourceLines, page_within, sanitize_byte_page, sanitize_line_page};
use super::prepare::*;
use super::types::*;
use super::view::single_line_query;
use crate::security::scan::ContentScan;
use crate::tools::source_view::local_fetch_query;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadProgressUnit};

/// One returned page after the secret scan.
pub(super) struct ScannedPage {
    pub(super) page: super::pagination::Page,
    /// The scan changed the page.
    pub(super) redacted: bool,
    /// Placeholders the scan put on the page.
    pub(super) redactions: usize,
    /// The scanned view is empty.
    pub(super) empty: bool,
    /// The scan kept the view's line count, so the page maps onto source lines.
    pub(super) lines_kept: bool,
    pub(super) warnings: Vec<String>,
}

/// How a page is cut from a view: the page's query, the view text, the
/// bytes one line page may hold, and the raw source lines a transformed
/// view pages in (see [`page_within`]).
#[derive(Clone, Copy)]
pub(super) struct PageWindow<'a> {
    pub(super) query: &'a LocalFetchQuery,
    pub(super) view: &'a str,
    pub(super) budget: super::pagination::LineBudget,
    pub(super) source_lines: Option<&'a SourceLines>,
}

/// Cut the page and scan it for secrets. A line or byte page is scanned on
/// its own window (see `sanitize_line_page` / `sanitize_byte_page`); a
/// complete view, or a window whose scan fails, takes the whole-view scan,
/// which owns the typed security-limit recovery.
pub(super) fn scan_page(
    q: &LocalFetchQuery,
    cut: &PageWindow<'_>,
    source_path: &std::path::Path,
    security: &impl ContentScan,
    source: &Source,
    first_line: Option<usize>,
) -> Result<ScannedPage, Box<LocalFetchResult>> {
    let PageWindow {
        query: page_q,
        view: selected,
        budget,
        source_lines,
    } = *cut;
    {
        let raw = page_within(selected, page_q, budget, source_lines);
        let literal = placeholders(&raw.text);
        let windowed = if raw.pagination.unit == ReadProgressUnit::Lines {
            sanitize_line_page(selected, raw, source_path, security).unwrap_or(None)
        } else {
            sanitize_byte_page(selected, raw, source_path, security).unwrap_or(None)
        };
        // Page sanitizers keep line counts, so a redacted page still maps
        // onto source lines.
        if let Some((page, redacted)) = windowed {
            let redactions = if redacted {
                placeholders(&page.text).saturating_sub(literal)
            } else {
                0
            };
            return Ok(ScannedPage {
                page,
                redacted,
                redactions,
                empty: selected.is_empty(),
                lines_kept: true,
                warnings: vec![],
            });
        }
    }
    let (safe, warnings) = match security.sanitize(selected, source_path) {
        Ok(scanned) => scanned,
        Err((code, _)) if code == ErrorCode::ContentSecurityLimit => {
            return Err(Box::new(security_limit(q, code, source, first_line)));
        }
        Err((code, message)) => {
            return Err(Box::new(LocalFetchResult::error(
                local_fetch_query::path(q).to_owned(),
                code,
                message,
            )));
        }
    };
    // A whole-view scan maps onto source lines only when it kept the count.
    let lines_kept = safe == selected || line_count(&safe) == line_count(selected);
    let page = page_within(&safe, page_q, budget, source_lines.filter(|_| lines_kept));
    let redacted = safe != selected;
    // The same page of the unscanned view, when the scan kept its lines;
    // otherwise every placeholder past the view's literal ones counts.
    let redactions = match redacted {
        false => 0,
        true if lines_kept => placeholders_added(
            &page_within(selected, page_q, budget, source_lines).text,
            &page.text,
        ),
        true => placeholders_added(selected, &safe),
    };
    Ok(ScannedPage {
        page,
        redacted,
        redactions,
        empty: safe.is_empty(),
        lines_kept,
        warnings,
    })
}

/// The selected view is too large to scan: a terminal row that offers one
/// source line instead.
fn security_limit(
    q: &LocalFetchQuery,
    code: ErrorCode,
    source: &Source,
    first_line: Option<usize>,
) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        code,
        "The selected content view exceeds the secret scanner size limit. Byte windows cannot safely split unscanned content. Select a smaller source-line range.".into(),
    );
    result.total_lines = Some(source.total_lines);
    result.source_chars = Some(source.chars);
    result.source_bytes = Some(source.bytes);
    result.terminal_limit(PartialReason::SecuritySelectedView);
    if source.total_lines > 1
        && (local_fetch_query::start_line(q).is_none()
            || local_fetch_query::start_line(q) != local_fetch_query::end_line(q))
    {
        let line = first_line.or(local_fetch_query::start_line(q)).unwrap_or(1);
        result.next = Some(NextCalls {
            read_bounded_lines: single_line_query(q, line).map(FetchCall::related),
            ..NextCalls::default()
        });
    }
    result
}

/// Redacted text is not the source: say so, as localSearch does. Only the
/// placeholders this read put on the page count (a byte page may show part
/// of a redacted line, so never more than the page holds): literal
/// `[REDACTED…]` text in the source is verbatim.
pub(super) fn warn_redacted(
    text: &str,
    view_redacted: bool,
    redactions: usize,
    warnings: &mut Vec<String>,
) {
    let redactions = redactions.min(placeholders(text));
    if redactions > 0 {
        warnings.push(format!(
            "redactedContent: {redactions} secret-shaped value(s) in the returned text were replaced by [REDACTED…] placeholders; this content is not verbatim source."
        ));
    } else if view_redacted {
        warnings.push(
            "redactedContent: secret-shaped text in the requested view was redacted; this content is not verbatim source.".into(),
        );
    }
}

/// matchString runs on redacted text, so a match inside a redacted value is
/// not selected: name how many redacted lines the selection does not show
/// (as a hit or as context), so they are not dropped silently. The count
/// depends only on the file and the selection (a plain read shows the same
/// placeholders), never on the guess. It is the same on every page, so the
/// first page states it. A `ranges` read counts hits inside its ranges only,
/// so it is left out.
pub(super) fn warn_unmatchable(
    q: &LocalFetchQuery,
    source: &Source,
    ext: &super::extraction::Extraction,
    warnings: &mut Vec<String>,
) {
    if !source.match_redacted
        || !local_fetch_query::has_matches(q)
        || local_fetch_query::has_ranges(q)
    {
        return;
    }
    let mut shown = match (&ext.source_lines, ext.start, ext.end) {
        (Some(lines), _, _) => lines.clone(),
        (None, Some(start), Some(end)) => (start..=end).collect(),
        _ => ext.matched_lines.clone(),
    };
    shown.sort_unstable();
    let unseen = source
        .redacted_lines
        .iter()
        .filter(|(line, _)| shown.binary_search(line).is_err())
        .count();
    if unseen > 0 {
        let lines = if unseen == 1 { "line" } else { "lines" };
        warnings.push(format!(
            "{unseen} redacted {lines} outside this result: matchString cannot see text behind a [REDACTED…] placeholder; a plain read shows where they are."
        ));
    }
}
