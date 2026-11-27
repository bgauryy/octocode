//! The character budget of patch windows: a share of the automatic
//! response page.

/// Patch characters one call carries across a page's files by default,
/// derived from the effective automatic response page (the runtime's
/// `auto_page_chars`, or a call's `responseChars`, or a batch row's share
/// of it). The window takes 4/5 of the page and leaves the rest to the
/// row's header, metadata and escaping: a patch that fits its page beside
/// its row arrives whole, since agents skip a row's `next.patches` (0 of
/// 14 followed in the 2026-11-23/24 benchmarks, each cut in a reply with
/// room to spare under the former fixed 5,000-char reserve).
/// An explicit `length` is honoured up to that one-row budget: a larger
/// window would only be re-sliced into response `rowPart`s, more calls for
/// the same patch text. A row that still outgrows its page is lossless:
/// automatic paging splits it by whole rows, and the walk's
/// `sectionPagination.patches.next.nextPage` and the file page ride the row's last part
/// (`response::row_pages`' part shares), so following it never skips a part.
/// A bigger window comes with a bigger response page (`responseChars`).
const PATCH_DEFAULT_SHARE: (usize, usize) = (4, 5);

/// The page a call carries: the runtime's, else (direct callers, tests) the
/// constant automatic page.
pub(in crate::gh_get_history_item) fn auto_page(auto_page: Option<usize>) -> usize {
    auto_page
        .filter(|page| *page > 0)
        .unwrap_or(octocode_runtime_base::execution::RuntimeLimits::DEFAULT.auto_page_chars)
}

/// The patch window of one row: most of its response page. An explicit
/// `length` is clamped to that budget, so one row's window always fits one
/// response page.
pub(in crate::gh_get_history_item) fn patch_window(auto_page_chars: Option<usize>) -> usize {
    let page = auto_page(auto_page_chars);
    let (num, den) = PATCH_DEFAULT_SHARE;
    (page * num / den).max(1)
}
