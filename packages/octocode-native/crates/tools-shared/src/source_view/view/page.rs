//! Cutting the returned page from the view and building its success row.
mod source_lines;

use super::super::pagination::{SourceLines, continuation, result_counts};
use super::super::types::*;
use super::super::{page_scan::*, prepare::*};
use super::warnings::read_scope_warnings;
use super::{View, compress_ranges, past_end};
use crate::source_view::local_fetch_query;
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit, ReadView};
use source_lines::page_lines;

/// The scanned page of a view, and the query it was cut for.
pub(super) struct CutPage<'a> {
    /// The read's query; bounded when a complete view was over the window.
    pub(super) q: std::borrow::Cow<'a, LocalFetchQuery>,
    /// The read as asked, not its bounded first page.
    asked: &'a LocalFetchQuery,
    /// The caller's read as sent, before a cut to fit the row's share: the
    /// scope notes (a long-line byte walk) judge it, not the cut.
    pub(super) read: &'a LocalFetchQuery,
    scanned: ScannedPage,
    /// What the page's lines were cut to: a refit of a row wider than its
    /// share counts its lines the same way.
    pub(super) budget: super::super::pagination::LineBudget,
}

/// Cut the returned page from the view. A complete view over the limit
/// returns its first bounded page inline. A line page's lines take `fill`
/// response characters at most, as they render (`None`: the row's whole
/// share, or the fallback page without one).
pub(super) fn cut_page<'a>(
    q: &'a LocalFetchQuery,
    view: &View,
    ext: &super::super::extraction::Extraction,
    source: &Source<'_>,
    facts: &SourceFacts,
    fill: Option<usize>,
) -> Result<CutPage<'a>, Box<LocalFetchResult>> {
    let asked = q;
    let q = std::borrow::Cow::Borrowed(q);
    // A configured response window owns the page budget. Keep the smaller
    // fallback for direct callers without one and any explicit preview.
    let line_page_bytes = super::super::pagination::line_page_bytes(facts.window);
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
    // A line page's lines take `fill`, counted as they render: escapes and
    // the gutter the response adds to a numbered view. The first cut takes
    // the row's whole share; the row's measured skeleton then refits it
    // (`view::render`), so the row as published fits its share. A line
    // longer than the byte budget (`line`) reads in byte pages.
    let numbered = matches!(view.mode, None | Some(ReadView::Content));
    let budget = super::super::pagination::LineBudget {
        fill: fill.unwrap_or(facts.window.unwrap_or(line_page_bytes)),
        line: line_page_bytes,
        gutter: if numbered {
            super::super::pagination::gutter_chars(facts.file_lines.unwrap_or(source.total_lines))
        } else {
            0
        },
    };
    let raw = facts.byte_origin.is_none() && view.mode.is_none();
    // A raw selection's bytes are placed in its source: a byte page names
    // the file's bytes, as a plain read of it does.
    let offsets;
    let placement = match (raw, ext.source_lines.as_deref()) {
        (true, Some(selected)) => {
            let source_offsets = match source.offsets.as_deref() {
                Some(known) => known,
                None => {
                    offsets = super::super::extraction::LineOffsets::new(&source.text);
                    &offsets
                }
            };
            Some(super::super::pagination::Placement::new(
                &view.text,
                selected,
                source_offsets,
                source.text.len(),
            ))
        }
        _ => None,
    };
    let long_lines = raw.then(|| super::super::pagination::LongLines::of(ext, placement.as_ref()));
    let window = PageWindow {
        query: page_q,
        view: &view.text,
        budget,
        source_lines: source_lines.as_ref(),
        long_lines,
        edges: facts.byte_origin.as_ref().map_or(
            super::super::pagination::Edges::WHOLE,
            |origin| super::super::pagination::Edges {
                lead: origin.lead,
                more: origin.base + view.text.len() < origin.total,
            },
        ),
    };
    let mut scanned = page_of(&window);
    if let Some(origin) = &facts.byte_origin
        && scanned.page.pagination.unit == ReadProgressUnit::Bytes
    {
        origin.place(&mut scanned.page.pagination);
    }
    let pagination = &mut scanned.page.pagination;
    if let Some(placement) = placement.as_ref()
        && pagination.unit == ReadProgressUnit::Bytes
        && !scanned.page.out_of_range
    {
        pagination.placed = placement
            .source_byte(pagination.offset)
            .map(|at| (at, placement.total));
    }
    Ok(CutPage {
        q,
        asked,
        read: asked,
        scanned,
        budget,
    })
}

/// A byte offset of `q` inside `view` that no window of it ends at: inside
/// a character, a CRLF pair or a redaction marker. Offsets count the
/// redacted view and every emitted window ends on a clean cut
/// ([`octocode_runtime_base::response::cut::clean_cut`]), so such an offset was not emitted
/// for this view; it is refused, never snapped to a byte the caller did not
/// name.
fn off_edge_offset(q: &LocalFetchQuery, view: &str) -> Option<usize> {
    let offset = local_fetch_query::offset(q)?;
    (local_fetch_query::unit(q) == ReadProgressUnit::Bytes
        && offset <= view.len()
        && !octocode_runtime_base::response::cut::is_clean_cut(view, offset))
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
    source_path: &std::path::Path,
    facts: &SourceFacts,
) -> LocalFetchResult {
    let CutPage {
        q,
        asked,
        read,
        scanned,
        budget: _,
    } = cut;
    let q = q.as_ref();
    let mut warnings = source.warnings.clone();
    warnings.extend(ext.warnings.iter().cloned());
    let pg = scanned.page;
    // A raw page's numbered lines say where it ends; a transformed view may
    // have dropped the file's last line, so its page does not.
    let last_shown = (view.mode.is_none() && !pg.out_of_range && !pg.text.is_empty())
        .then(|| page_source_ranges(ext.source_lines.as_deref(), pg.view_lines))
        .and_then(|ranges| ranges.last().map(|range| range.end));
    warnings.extend(ext.clip_warnings(last_shown, source.total_lines));
    if local_fetch_query::offset(asked).unwrap_or(0) == 0 {
        warnings.extend(view.note.iter().cloned());
    }
    warnings.extend(scanned.warnings);
    let source_redactions = if source.redacted_lines.is_empty() {
        0
    } else {
        source.redactions_on(page_lines(&pg, ext.source_lines.as_deref(), view.mode))
    };
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
    let next = continue_calls(q, &pg, ext);
    if pg.rest_of_line
        && let Some(line) = page_source_ranges(ext.source_lines.as_deref(), pg.view_lines).first()
    {
        warnings.push(format!(
            "Every match in line {} is shown with its window; the rest of the line is not. The related read returns the whole line.",
            line.start
        ));
    }
    // The read's first page states what every page shares.
    let first_page = pg.pagination.offset == 0 || local_fetch_query::read_progress(asked).is_none();
    if first_page {
        warnings.extend(super::super::extraction::unmatched_warning(
            asked,
            &ext.unmatched,
        ));
    }
    let out_of_range = pg.out_of_range;
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
    // The line a byte page opens inside: the source line of its first view
    // line, which a view with its own gutter states there.
    let continues_line = (pg.opens_inside && !out_of_range && !scanned.empty)
        .then(|| match view.mode {
            None => page_source_ranges(
                ext.source_lines.as_deref(),
                (pg.view_lines.0, pg.view_lines.0),
            )
            .first()
            .map(|range| range.start),
            Some(_) => view
                .text
                .lines()
                .nth(pg.view_lines.0.saturating_sub(1))
                .and_then(|line| {
                    let (number, _) =
                        line.split_once(octocode_runtime_base::response::numbered::SEPARATOR)?;
                    number.split('-').next()?.parse().ok()
                }),
        })
        .flatten();
    let blocks = if out_of_range {
        vec![]
    } else {
        ext.blocks.clone()
    };
    let mut matched_lines = ext.matched_lines.clone();
    matched_lines.retain(|n| source_ranges.iter().any(|r| *n >= r.start && *n <= r.end));
    let match_keys = crate::match_keys::line_key_paths(source_path, &source.text, &matched_lines);
    let (_, ret_bytes, _) = result_counts(&pg.text);
    // The view lines the page shows, as `sourceLineRanges` places them: a
    // page of match windows of one long line shows that line once, whatever
    // the gap markers between its windows add to the text.
    let ret_lines = if pg.text.is_empty() {
        0
    } else {
        (pg.view_lines.1 + 1).saturating_sub(pg.view_lines.0)
    };
    let minified_away = pg.text.is_empty() && view.mode.is_some() && !out_of_range;
    let mut result = LocalFetchResult {
        path: local_fetch_query::path(q).to_owned(),
        source_sha256: Some(source.sha256.clone()),
        judged: judged_page(
            &pg.text,
            Sealed {
                asked,
                source,
                view,
            },
            source_path,
            facts,
        ),
        content: Some(pg.text),
        view: view.mode,
        view_fallback: view.fallback.clone(),
        warnings,
        total_lines: Some(source.total_lines),
        start_line: ext.start,
        end_line: ext.end,
        source_line_ranges: source_ranges,
        continues_line,
        match_ranges: ext.match_ranges.clone(),
        matched_lines,
        match_keys,
        blocks,
        selected_match_count: ext.count,
        // A match read searched the whole source: its masked spans do not
        // depend on the match strings.
        masked_spans: Some(source.redactions_total())
            .filter(|spans| *spans > 0 && !local_fetch_query::match_strings(asked).is_empty()),
        returned_bytes: Some(ret_bytes),
        returned_lines: Some(ret_lines),
        pagination: Some(pg.pagination),
        // A capped regex scan selected a prefix of the matches.
        is_partial: ((next.as_ref().is_some_and(NextCalls::leaves_unread) && !out_of_range)
            || ext.match_limited)
            .then_some(true),
        partial_reasons: if ext.match_limited {
            vec![PartialReason::ScanBudget]
        } else {
            Vec::new()
        },
        out_of_range,
        next,
        continued: !first_page,
        ..LocalFetchResult::blank(ReadStatus::Success)
    };
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
    result
}

/// Where a page's text was cut from, to find the text the whole-source pass
/// sealed.
struct Sealed<'a> {
    asked: &'a LocalFetchQuery,
    source: &'a Source<'a>,
    view: &'a View,
}

impl Sealed<'_> {
    /// The whole-source pass's output this page is a slice of. A whole read
    /// moved it into its view (`view.rs`, `select_view`), where it stays
    /// as long as no outline rewrote the view; a replayed page
    /// cuts from that same kept view. The compact view is that output laid
    /// out again: compaction drops whitespace and comments between the
    /// tokens of the sealed text and numbers the lines it keeps, so each
    /// token it shows was judged with the syntax around it, and a secret the
    /// pass replaced is a placeholder in it. Judging its pages again, as
    /// numbered fragments no grammar reads, would redact code the content
    /// view of the same lines shows. Any other read keeps the pass's output
    /// in its source. A rewritten whole view has neither: its text is empty,
    /// so no page of it is marked.
    fn text(&self) -> &str {
        let untouched_whole =
            super::super::extraction::selects_whole(self.asked) && self.view.mode.is_none();
        let laid_out_again = self.view.mode == Some(ReadView::Compact);
        if untouched_whole || laid_out_again {
            &self.view.text
        } else {
            &self.source.text
        }
    }
}

/// The page's judged-leaf mark: only for text the whole-source pass of this
/// request (or the read it replays) sealed, and only when the page is a
/// slice of it, or slices of it with the read's own separators between its
/// windows ([`octocode_runtime_base::security::JudgedSource::leaf`]): a read of ranges
/// shows each line as the whole read does. A window of a sealed large
/// source arrives judged elsewhere.
fn judged_page(
    page: &str,
    sealed: Sealed<'_>,
    source_path: &std::path::Path,
    facts: &SourceFacts,
) -> Option<octocode_runtime_base::security::JudgedLeaf> {
    if facts.byte_origin.is_some() {
        return None;
    }
    octocode_runtime_base::security::JudgedSource::new(sealed.text(), source_path).leaf(
        page,
        octocode_runtime_base::response::numbered::is_separator,
    )
}

/// The hint of a read whose minified view removed every selected line.
const MINIFIED_AWAY_HINT: &str =
    "The compact view removed every selected line; use view:content to read the source.";

/// The page's continuations: its next page, plus follow-ups to data the
/// extraction selected but did not return.
fn continue_calls(
    q: &LocalFetchQuery,
    pg: &super::super::pagination::Page,
    ext: &super::super::extraction::Extraction,
) -> Option<NextCalls> {
    let mut next = page_next(q, pg);
    // Every match of a long line shown: its whole line is a lead.
    if pg.rest_of_line
        && let Some(line) = line_read(
            q,
            &page_source_ranges(ext.source_lines.as_deref(), pg.view_lines),
        )
    {
        let next = next.get_or_insert_with(NextCalls::default);
        if next.read.is_none() {
            next.read = Some(FetchCall::needed(line));
        }
    }
    let follow_ups = ext.next.clone();
    if !pg.out_of_range && !follow_ups.is_empty() {
        next.get_or_insert_with(NextCalls::default)
            .absorb(follow_ups);
    }
    next
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
