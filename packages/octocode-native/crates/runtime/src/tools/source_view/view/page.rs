//! Cutting the returned page from the view and building its success row.
mod budget_hint;
mod source_lines;

use super::super::pagination::{SourceLines, continuation, result_counts};
use super::super::types::*;
use super::super::{page_scan::*, prepare::*};
use super::classified::attach_unfiltered;
use super::warnings::read_scope_warnings;
use super::{View, compress_ranges, past_end};
use crate::tools::source_view::local_fetch_query;
use budget_hint::budget_cut_hint;
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit, ReadView};
use source_lines::page_lines;

/// The scanned page of a view, and the query it was cut for.
pub(super) struct CutPage<'a> {
    /// The read's query; bounded when a complete view was over the window.
    pub(super) q: std::borrow::Cow<'a, LocalFetchQuery>,
    /// The read as asked: classified routes (the whole view, the next
    /// candidate page) repeat it, not its bounded first page.
    asked: &'a LocalFetchQuery,
    /// The caller's read as sent, before a cut to fit the row's share: the
    /// scope notes (a long-line byte walk) judge it, not the cut.
    pub(super) read: &'a LocalFetchQuery,
    scanned: ScannedPage,
}

/// Cut the returned page from the view. A complete view over the limit
/// returns its first bounded page inline.
pub(super) fn cut_page<'a>(
    q: &'a LocalFetchQuery,
    view: &View,
    ext: &super::super::extraction::Extraction,
    source: &Source<'_>,
    facts: &SourceFacts,
) -> Result<CutPage<'a>, Box<LocalFetchResult>> {
    let asked = q;
    let q = std::borrow::Cow::Borrowed(q);
    // A configured response window owns the page budget. Keep the smaller
    // fallback for direct callers without one and any explicit preview.
    let line_page_bytes = facts
        .window
        .map_or(super::super::pagination::LINE_PAGE_BYTES, |window| {
            crate::response::page_windows::page_budget(Some(window))
        });
    let page_q = q.as_ref();
    let off = match &facts.byte_origin {
        None => off_edge_offset(page_q, &view.text),
        Some(origin) => origin.off_edge(page_q, &view.text),
    };
    if let Some(offset) = off {
        return Err(Box::new(off_edge(page_q, offset)));
    }
    // A byte page of a streamed window is cut at its offset in the window.
    let in_window = facts
        .byte_origin
        .as_ref()
        .map(|origin| origin.within(page_q));
    let page_q = in_window.as_ref().unwrap_or(page_q);
    // A transformed view pages in the lines a raw read of the same
    // selection pages in: raw source lines for a whole file (the unit of
    // `totalLines`, the gutter and `ranges`), the extracted lines for
    // `ranges`; an outline pages its own lines.
    let source_lines = (view.mode == Some(ReadView::Compact))
        .then(|| match ext.source_lines.as_deref() {
            None => SourceLines::of_gutter(&view.text, source.total_lines),
            Some(selected) => SourceLines::of_selected_gutter(&view.text, selected),
        })
        .flatten();
    // A line page takes up to the row's whole share; the reader then cuts it
    // to fit exactly ([`fitted_lines`]). A line longer than the byte budget
    // reads in byte pages.
    let budget = super::super::pagination::LineBudget {
        fill: facts
            .window
            .map_or(line_page_bytes, |share| share.max(line_page_bytes)),
        line: line_page_bytes,
    };
    let window = PageWindow {
        query: page_q,
        view: &view.text,
        budget,
        source_lines: source_lines.as_ref(),
    };
    let mut scanned = page_of(&window);
    if let Some(origin) = &facts.byte_origin
        && scanned.page.pagination.unit == ReadProgressUnit::Bytes
    {
        origin.place(&mut scanned.page.pagination);
    }
    Ok(CutPage {
        q,
        asked,
        read: asked,
        scanned,
    })
}

/// A byte offset of `q` inside `view` that no window of it ends at: inside
/// a character, a CRLF pair or a redaction marker. Offsets count the
/// redacted view and every emitted window ends on a clean cut
/// ([`crate::response::cut::clean_cut`]), so such an offset was not emitted
/// for this view; it is refused, never snapped to a byte the caller did not
/// name.
fn off_edge_offset(q: &LocalFetchQuery, view: &str) -> Option<usize> {
    let offset = local_fetch_query::offset(q)?;
    (local_fetch_query::unit(q) == ReadProgressUnit::Bytes
        && offset <= view.len()
        && !crate::response::cut::is_clean_cut(view, offset))
    .then_some(offset)
}

/// The refusal of a byte cursor no window emitted: the read restarts from
/// its first page.
fn off_edge(q: &LocalFetchQuery, offset: usize) -> LocalFetchResult {
    let mut result = LocalFetchResult::error(
        local_fetch_query::path(q).to_owned(),
        octocode_contract_types::ErrorCode::InvalidPagination,
        format!(
            "readProgress byte offset {offset} is not a window edge of this view: it falls inside a character, a CRLF pair or a redaction marker. Continue with an emitted next call, or restart the read."
        ),
    );
    result.next = Some(past_end(q));
    result
}

/// The success row for a scanned page: its warnings, continuations and
/// source anchors.
pub(super) fn page_result(
    cut: CutPage,
    source: &Source<'_>,
    ext: &super::super::extraction::Extraction,
    view: &View,
    selection_outcome: Option<&super::super::selection::Chosen>,
    source_path: &std::path::Path,
    facts: &SourceFacts,
) -> LocalFetchResult {
    let CutPage {
        q,
        asked,
        read,
        scanned,
    } = cut;
    let q = q.as_ref();
    let mut warnings = source.warnings.clone();
    warnings.extend(ext.warnings.iter().cloned());
    if local_fetch_query::offset(asked).unwrap_or(0) == 0 {
        warnings.extend(view.note.iter().cloned());
    }
    warnings.extend(scanned.warnings);
    let pg = scanned.page;
    let source_redactions = if source.redacted_lines.is_empty() {
        0
    } else {
        source.redactions_on(page_lines(&pg, ext.source_lines.as_deref(), view.mode))
    };
    // The page shows a key block the source-wide pass redacted.
    if source.key_blocks_redacted
        && source_redactions > 0
        && pg
            .text
            .contains(&crate::security::key_fragment_placeholder())
    {
        warnings.push(KEY_BLOCK_NOTE.into());
    }
    let blanked = source_redactions > 0
        && pg
            .text
            .lines()
            .zip(page_lines(&pg, ext.source_lines.as_deref(), view.mode))
            .any(|(text, line)| text.trim().is_empty() && source.redactions_on([line]) > 0);
    warn_redacted(
        &pg.text,
        scanned.redacted,
        blanked,
        scanned.redactions + source_redactions,
        &mut warnings,
    );
    // The read's first page states what every page shares.
    if pg.pagination.offset == 0 || local_fetch_query::read_progress(asked).is_none() {
        warn_unmatchable(q, source, ext, &mut warnings);
        warnings.extend(
            selection_outcome
                .into_iter()
                .flat_map(super::super::selection::Chosen::disclosure),
        );
    }
    let out_of_range = pg.out_of_range;
    let token = selection_outcome.map(|chosen| chosen.outcome.token.as_str());
    let mut next = match continue_calls(q, &pg, token, ext) {
        Ok(next) => next,
        Err(error) => return super::classified::invalid_continuation(q, error),
    };
    if let Some(chosen) = selection_outcome
        && chosen.outcome.leaves_out()
        && let Err(error) =
            attach_unfiltered(next.get_or_insert_with(NextCalls::default), asked, chosen)
    {
        return super::classified::invalid_continuation(q, error);
    }
    read_scope_warnings(read, &pg, &mut warnings);
    // Redaction (source-wide for matchString, or on the page) replaces text
    // within lines; the anchors stay and the warning above says the text is
    // not verbatim.
    let source_ranges =
        if !out_of_range && !scanned.empty && view.mode.is_none() && scanned.lines_kept {
            page_source_ranges(ext.source_lines.as_deref(), pg.view_lines)
        } else {
            vec![]
        };
    let blocks = if out_of_range {
        vec![]
    } else {
        ext.blocks.clone()
    };
    let mut matched_lines = ext.matched_lines.clone();
    matched_lines.retain(|n| source_ranges.iter().any(|r| *n >= r.start && *n <= r.end));
    let match_keys =
        crate::tools::match_keys::line_key_paths(source_path, &source.text, &matched_lines);
    let (chars, ret_bytes, ret_lines) = result_counts(&pg.text);
    let minified_away = pg.text.is_empty() && view.mode.is_some() && !out_of_range;
    let budget_cut = budget_cut_hint(read, &pg.pagination, out_of_range);
    let mut result = LocalFetchResult {
        path: local_fetch_query::path(q).to_owned(),
        source_sha256: Some(source.sha256.clone()),
        content: Some(pg.text),
        view: view.mode,
        view_fallback: view.fallback.clone(),
        warnings,
        total_lines: Some(source.total_lines),
        start_line: ext.start,
        end_line: ext.end,
        source_line_ranges: source_ranges,
        match_ranges: ext.match_ranges.clone(),
        matched_lines,
        match_keys,
        blocks,
        // Under a classifier the row counts what this page kept; the
        // receipt counts the whole walk.
        selected_match_count: selection_outcome
            .map(|chosen| chosen.outcome.selected.len())
            .or(ext.count),
        modified: (view.mode != Some(ReadView::Symbols))
            .then(|| facts.modified.clone())
            .flatten(),
        returned_bytes: Some(ret_bytes),
        returned_lines: Some(ret_lines),
        pagination: Some(pg.pagination),
        // A capped regex scan selected a prefix of the matches.
        is_partial: ((next.as_ref().is_some_and(NextCalls::leaves_unread) && !out_of_range)
            || ext.match_limited)
            .then_some(true),
        partial_reasons: if ext.match_limited {
            vec![PartialReason::RegexMatchLimit]
        } else {
            Vec::new()
        },
        out_of_range,
        next,
        selection: selection_outcome.map(|chosen| chosen.outcome.receipt.clone()),
        ..LocalFetchResult::blank(ReadStatus::Success)
    };
    result.hints.extend(budget_cut);
    // The view removed every selected line (a comment-only range read
    // minified): an empty read whose lead is the same read as is.
    if minified_away && source.bytes > 0 {
        let Ok(query) = local_fetch_query::updated(q, serde_json::json!({"view":"content"})) else {
            return result;
        };
        result.status = ReadStatus::Empty;
        result.hints.push(MINIFIED_AWAY_HINT.into());
        result.next.get_or_insert_with(NextCalls::default).read = Some(FetchCall::related(query));
    }
    // A classified view that kept no chunk: an empty read whose unfiltered
    // lead is its recovery (zero kept is not absence).
    if chars == 0
        && result
            .selection
            .as_ref()
            .is_some_and(|receipt| receipt.total_items > 0)
        && selection_outcome.is_some_and(|chosen| chosen.outcome.selected.is_empty())
    {
        result.status = ReadStatus::Empty;
    }
    result
}

/// The hint of a read whose minified view removed every selected line.
const MINIFIED_AWAY_HINT: &str =
    "The compact view removed every selected line; use view:content to read the source.";

/// The page's continuations: its next page bound to the selection token, plus
/// follow-ups to data the extraction selected but did not return.
fn continue_calls(
    q: &LocalFetchQuery,
    pg: &super::super::pagination::Page,
    token: Option<&str>,
    ext: &super::super::extraction::Extraction,
) -> Result<Option<NextCalls>, String> {
    let mut next = page_next(q, pg);
    if let Some(token) = token
        && let Some(read) = next.as_mut().and_then(|next| next.continue_window.as_mut())
    {
        local_fetch_query::set_selection(&mut read.query, Some(token))
            .map_err(|error| error.to_string())?;
    }
    let follow_ups = ext.next.clone();
    if !pg.out_of_range && !follow_ups.is_empty() {
        next.get_or_insert_with(NextCalls::default)
            .absorb(follow_ups);
    }
    Ok(next)
}

/// Continue the selected view or restart an out-of-range saved position.
fn page_next(q: &LocalFetchQuery, pg: &super::super::pagination::Page) -> Option<NextCalls> {
    if pg.out_of_range {
        Some(past_end(q))
    } else {
        continuation(q, &pg.pagination)
    }
}

/// The source lines a page shows: through the view-to-source line map when
/// the extraction kept one, else the page's own line span.
fn page_source_ranges(
    source_lines: Option<&[usize]>,
    view_lines: (usize, usize),
) -> Vec<LineRange> {
    let Some(lines) = source_lines else {
        return vec![LineRange {
            start: view_lines.0,
            end: view_lines.1,
        }];
    };
    let mut page_lines: Vec<usize> = lines
        [view_lines.0.saturating_sub(1)..view_lines.1.min(lines.len())]
        .iter()
        .copied()
        .filter(|line| *line != super::super::extraction::OMISSION_LINE)
        .collect();
    page_lines.dedup();
    compress_ranges(&page_lines)
}
