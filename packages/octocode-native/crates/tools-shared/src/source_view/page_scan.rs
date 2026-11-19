//! The secret scan of one returned page and the warnings it raises.

use super::pagination::{Edges, LongLines, SourceLines, page_within};
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
    /// Where the view sits in the view it was cut from.
    pub(super) edges: Edges,
}

/// Cut the page from its view. Every view is cut from a source redacted whole
/// before any view was built (a streamed window from its sealed view,
/// [`ByteOrigin`](super::types::ByteOrigin)), so the page is never scanned again, and only the
/// placeholders it shows count.
pub(super) fn page_of(cut: &PageWindow<'_>) -> ScannedPage {
    let PageWindow {
        query,
        view,
        budget,
        source_lines,
        long_lines,
        edges,
    } = *cut;
    ScannedPage {
        page: page_within(view, query, budget, source_lines, long_lines, edges),
        redacted: false,
        redactions: 0,
        empty: view.is_empty(),
        lines_kept: true,
        warnings: vec![],
    }
}

/// Redacted text is not the source: the row says so with its `redacted` flag
/// ([`octocode_runtime_base::security::redacted_mark`]). Only the placeholders this read put
/// on the page count (a byte page may show part of a redacted line, so never
/// more than the page holds): literal `[REDACTED…]` text in the source is
/// verbatim. A redaction spanning lines shows its placeholder on its first
/// and blanks the others, so a page of them holds no placeholder: `blanked`
/// says it shows such a line.
pub(super) fn warn_redacted(
    text: &str,
    view_redacted: bool,
    blanked: bool,
    redactions: usize,
    warnings: &mut Vec<String>,
) {
    let redactions = redactions.min(octocode_runtime_base::security::placeholders(text));
    if redactions > 0 || view_redacted || blanked {
        warnings.push(octocode_runtime_base::security::redacted_mark());
    }
}
