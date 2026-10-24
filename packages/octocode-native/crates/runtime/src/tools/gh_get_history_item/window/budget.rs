//! The character budget of patch windows: a share of the automatic
//! response page.

/// Patch characters one call carries across a page's files by default,
/// derived from the effective automatic response page (the runtime's
/// `auto_page_chars`, or a call's `responseChars`). Rendered text prints
/// patches verbatim (no escaping), so the default window takes 4/5 of the
/// page less a fixed reserve for the row header and metadata, never below
/// 2/5 of it: a default window plus row metadata fits one response page.
/// An explicit `length` is honoured up to that one-row budget: a larger
/// window would only be re-sliced into response `rowPart`s, more calls for
/// the same patch text. A row that still outgrows its page is lossless:
/// automatic paging splits it by whole rows, and the walk's
/// `next.continuePatch`/`nextFilePage` rides the row's last part
/// (`id::resumes_after_shown`), so following it never skips a part.
/// A bigger window comes with a bigger response page (`responseChars`).
const PATCH_DEFAULT_SHARE: (usize, usize) = (4, 5);

const PATCH_DEFAULT_RESERVE: usize = 5_000;

/// The page a call carries: the runtime's, else (direct callers, tests) the
/// constant automatic page.
pub(in crate::tools::gh_get_history_item) fn auto_page(auto_page: Option<usize>) -> usize {
    auto_page
        .filter(|page| *page > 0)
        .unwrap_or(crate::execution::RuntimeLimits::DEFAULT.auto_page_chars)
}

/// The default window of a `matchString` view: the page share without the
/// fixed reserve.
pub(in crate::tools::gh_get_history_item) fn literal_patch_window(
    auto_page_chars: Option<usize>,
) -> usize {
    let page = auto_page(auto_page_chars);
    let (num, den) = PATCH_DEFAULT_SHARE;
    (page * num / den).max(1)
}

/// The patch window of one row: most of its response page. An explicit
/// `length` is clamped to that budget, so one row's window always fits one
/// response page.
pub(in crate::tools::gh_get_history_item) fn patch_window(auto_page_chars: Option<usize>) -> usize {
    let page = auto_page(auto_page_chars);
    let (num, den) = PATCH_DEFAULT_SHARE;
    (page * num / den)
        .min(page.saturating_sub(PATCH_DEFAULT_RESERVE))
        .max(page * 2 / 5)
        .max(1)
}
