//! The secret scan of one returned page and the warnings it raises.

use super::pagination::{LongLines, SourceLines, page_within};
use super::prepare::*;
use crate::tools::source_view::local_fetch_query;
use octocode_contract_types::LocalFetchQuery;

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
/// bytes one line page may hold, the raw source lines a transformed view
/// pages in (see [`page_within`]), and how a raw view held whole pages its
/// long lines.
#[derive(Clone, Copy)]
pub(super) struct PageWindow<'a> {
    pub(super) query: &'a LocalFetchQuery,
    pub(super) view: &'a str,
    pub(super) budget: super::pagination::LineBudget,
    pub(super) source_lines: Option<&'a SourceLines>,
    pub(super) long_lines: Option<LongLines<'a>>,
}

/// Cut the page from its view. Every view is cut from a source redacted whole
/// before any view was built (a streamed window from its sealed view,
/// [`ByteOrigin`]), so the page is never scanned again, and only the
/// placeholders it shows count.
pub(super) fn page_of(cut: &PageWindow<'_>) -> ScannedPage {
    let PageWindow {
        query,
        view,
        budget,
        source_lines,
        long_lines,
    } = *cut;
    ScannedPage {
        page: page_within(view, query, budget, source_lines, long_lines),
        redacted: false,
        redactions: 0,
        empty: view.is_empty(),
        lines_kept: true,
        warnings: vec![],
    }
}

/// Redacted text is not the source: say so, as localSearch does. Only the
/// placeholders this read put on the page count (a byte page may show part
/// of a redacted line, so never more than the page holds): literal
/// `[REDACTED…]` text in the source is verbatim. A redaction spanning lines
/// shows its placeholder on its first and blanks the others, so a page of them
/// holds no placeholder: `blanked` says it shows such a line.
pub(super) fn warn_redacted(
    text: &str,
    view_redacted: bool,
    blanked: bool,
    redactions: usize,
    warnings: &mut Vec<String>,
) {
    let redactions = redactions.min(placeholders(text));
    if redactions > 0 || view_redacted || blanked {
        warnings.push(crate::security::redacted_content_warning(redactions));
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
    source: &Source<'_>,
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
