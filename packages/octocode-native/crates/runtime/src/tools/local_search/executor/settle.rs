//! The row of one page: its status, coverage warnings, leads, totals and
//! the limits it ends at.
use crate::tools::local_search::local_search_query::View;

use super::super::coverage::{Coverage, uncollected_warning};
use super::super::layout::Layout;
use super::super::leads::{Found, scope_directories, scope_listing};
use super::super::types::{
    FileErrors, FilePagination, LocalSearchResult, SearchStats, SearchStatus,
};
use super::super::verify::PageScope;
use super::projection::Annotated;

use super::super::local_search_query;
use crate::execution::cancel::CancellationCheck;
use crate::response::result::ToolError;
use octocode_contract_types::{LocalSearchQuery, OutputPaginationVariant0Unit};
use octocode_engine::types::TextSearchOptions;
use serde_json::{Value, json};

/// One cut and verified page, as [`settle_row`] reads it.
pub(super) struct Page<'a> {
    /// The validated search root (a file or a directory).
    pub(super) root: &'a std::path::Path,
    pub(super) layout: &'a Layout,
    pub(super) scanned: &'a octocode_engine::types::TextSearchStats,
    pub(super) skipped: &'a crate::policy::discovery::WalkSkips,
    /// The query's own walk, for probes over the same scope.
    pub(super) search_options: &'a TextSearchOptions,
    /// Files the scan collected, on every page.
    pub(super) total_files: u32,
    /// Some values were redacted because their source could not be re-read.
    pub(super) unverified: bool,
    /// Rows remain in the shown files.
    pub(super) leftover_matches: bool,
}

/// The row of one page: its status, coverage warnings, leads and limits.
pub(super) fn settle_row(
    scope: &PageScope<'_, impl CancellationCheck>,
    page: &Page<'_>,
    rows: Annotated<'_>,
    mut next: Option<Value>,
) -> Result<LocalSearchResult, ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        cancel,
        ..
    } = *scope;
    let (layout, total_files) = (page.layout, page.total_files);
    let empty = total_files == 0
        && !(local_search_query::has_classification(query)
            && page.scanned.files_matched.unwrap_or(0) > 0);
    let mut stats = search_stats(local_search_query::view(query), page.scanned, total_files);
    let mut coverage = Coverage::of(
        query,
        &stats,
        page.scanned,
        page.root,
        output_root,
        page.skipped,
    );
    let mut warnings = coverage.warnings(layout, rows.shown_redacted, page.unverified, empty);
    warnings.extend(rows.notes);
    let capped = stats.capped.unwrap_or(false);
    let (status, settled_terminal) = settle(
        query,
        layout,
        empty,
        capped,
        page.leftover_matches,
        coverage.gap(),
        next.is_none(),
    );
    let ceilings = cursor_ceilings(query, layout, page.leftover_matches);
    stats.partial_reasons.extend(ceilings.partial_reasons);
    let mut hints = coverage.empty_hints(empty);
    // Leads go in after the status is settled: they are optional follow-ups
    // and never stand in for a page.
    let disclosed = Found {
        query,
        paths,
        root: page.root,
        output_root,
        files: &rows.files,
        layout,
        definition: rows.definition.as_ref(),
        symbol: rows.symbol,
        scanned: page.scanned,
        skipped: page.skipped,
        walk_complete: !coverage.walk_cut,
        empty,
    }
    .add_leads(
        &mut stats,
        &mut next,
        &mut hints,
        &mut warnings,
        page.search_options,
        cancel,
    )?;
    if disclosed.skipped_match {
        coverage.skipped_paths_hold_matches(&mut hints);
    }
    // A zero-match search cut by a binary file proved nothing about the cut
    // files: the tree of the scope is the way to narrow it.
    let lists_scope =
        (coverage.scope_miss || (empty && coverage.binary_cut)).then_some(!coverage.scope_miss);
    route_beyond_pages(
        query,
        &stats,
        total_files,
        lists_scope,
        &mut next,
        &mut warnings,
    );
    warnings.extend(ceilings.warnings);
    Ok(LocalSearchResult {
        status,
        stats,
        files: rows.files,
        pagination: file_paging(query, layout, total_files, page.leftover_matches),
        hints,
        next,
        is_partial: coverage.gap() || capped || ceilings.terminal || disclosed.unchecked,
        // A retry over the skipped paths reaches the match, so no limit ends
        // this answer.
        terminal_limit: (settled_terminal || ceilings.terminal) && !disclosed.skipped_match,
        warnings,
        skipped_paths: disclosed.skipped_paths,
        source_snapshot: None,
        selection: None,
    })
}

/// What lies past every page the row can continue to: files past the
/// collection cap are on no page (every page says so), and the row leads
/// to the scope's top directories to split the search by (`narrowScope`).
/// A scope miss or an unproven empty result (`lists_scope`) leads to the
/// scope's listing: with the search's globs when they matched files
/// (`Some(true)`), without them when they matched none, so the listing
/// shows what names exist. File pages past the last page a continuation
/// names are [`cursor_ceilings`].
fn route_beyond_pages(
    query: &LocalSearchQuery,
    stats: &SearchStats,
    total_files: u32,
    lists_scope: Option<bool>,
    next: &mut Option<Value>,
    warnings: &mut Vec<String>,
) {
    let uncollected = uncollected_warning(stats, total_files);
    if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
        if let Some(keep_globs) = lists_scope {
            map.insert("viewTree".into(), scope_listing(query, keep_globs));
        }
        if uncollected.is_some() {
            map.insert("narrowScope".into(), scope_directories(query));
        }
    }
    if next
        .as_ref()
        .is_some_and(|next| next.as_object().is_some_and(serde_json::Map::is_empty))
    {
        *next = None;
    }
    warnings.extend(uncollected);
}

/// What a row reports for the cursors its rest is past.
struct Ceilings {
    warnings: Vec<String>,
    partial_reasons: Vec<String>,
    terminal: bool,
}

/// Later file pages, and hits left in the shown files, past the contract
/// maximum of their cursor (`page`, `matchPage`): the row ends at a terminal
/// limit, written by the one cursor owner ([`crate::response::paging::next_cursor`]).
fn cursor_ceilings(query: &LocalSearchQuery, layout: &Layout, leftover_matches: bool) -> Ceilings {
    let page = local_search_query::page(query).max(1);
    let warnings: Vec<String> = [
        (page < layout.total_pages).then(|| ("page", page as usize + 1)),
        leftover_matches.then(|| {
            (
                "matchPage",
                local_search_query::match_page(query).max(1) as usize + 1,
            )
        }),
    ]
    .into_iter()
    .flatten()
    .filter_map(|(field, next)| {
        crate::response::paging::cursor_value(crate::id::ToolId::LocalSearch, None, field, next)
            .err()
    })
    .collect();
    let terminal = !warnings.is_empty();
    Ceilings {
        warnings,
        partial_reasons: terminal
            .then(|| crate::response::paging::CURSOR_MAXIMUM.to_owned())
            .into_iter()
            .collect(),
        terminal,
    }
}

/// File paging, reported only when it routes somewhere: more file pages,
/// rows left in the shown files (`leftover_matches`), or a requested page
/// past the end. Totals live once, in `stats` (occurrences and matched
/// lines); the next page number and the snapshot live once, in
/// `next.*.query`.
fn file_paging(
    query: &LocalSearchQuery,
    layout: &Layout,
    total_files: u32,
    leftover_matches: bool,
) -> Option<FilePagination> {
    let page = local_search_query::page(query).max(1);
    let match_page = local_search_query::match_page(query).max(1);
    let has_more = page < layout.total_pages || leftover_matches;
    let unit = if layout.list || !layout.streamed {
        Some(OutputPaginationVariant0Unit::Files)
    } else {
        (local_search_query::view(query) != View::MatchOnly)
            .then_some(OutputPaginationVariant0Unit::Lines)
    };
    (total_files > 0 && (has_more || layout.total_pages > 1 || layout.out_of_range)).then_some(
        FilePagination {
            unit,
            page,
            match_page: (match_page > 1).then_some(match_page),
            total_pages: layout.total_pages,
            files_per_page: layout.files_per_page,
            total_items: layout.total_items,
            has_more,
            out_of_range: layout.out_of_range,
        },
    )
}

/// The row's status, and whether it ends at a limit nothing continues
/// (a cursor past its maximum is [`cursor_ceilings`]).
fn settle(
    query: &LocalSearchQuery,
    layout: &Layout,
    empty: bool,
    capped: bool,
    leftover_matches: bool,
    coverage_gap: bool,
    next_missing: bool,
) -> (SearchStatus, bool) {
    let has_more = local_search_query::page(query).max(1) < layout.total_pages;
    classify_search(
        empty,
        capped,
        has_more,
        leftover_matches,
        coverage_gap,
        next_missing,
    )
}

/// The response's totals from the scan, in the units the view counted:
/// countLines counts no occurrences, and a filesWithout listing counts
/// neither (its files hold no hit). A view with nothing listed states zero.
pub(crate) fn search_stats(
    view: View,
    scanned: &octocode_engine::types::TextSearchStats,
    total_files: u32,
) -> SearchStats {
    let counted = |count: Option<u32>| {
        let listed_without = view == View::FilesWithout && total_files > 0;
        if listed_without {
            None
        } else {
            count.or((total_files == 0).then_some(0))
        }
    };
    SearchStats {
        total_occurrences: counted(scanned.match_count),
        matched_lines: counted(scanned.matched_lines),
        files_matched: scanned.files_matched.unwrap_or(total_files),
        files_searched: scanned.files_searched.unwrap_or(0),
        bytes_searched: scanned.bytes_searched,
        // A binary cut (capReason `binaryQuit`) stays `capped`: coverage ended
        // early, and the result is marked partial/terminal.
        capped: scanned.capped,
        cap_reasons: scanned.cap_reasons.clone(),
        partial_reasons: scanned
            .cap_reasons
            .iter()
            .map(|reason| reason.name().to_owned())
            .collect(),
        error_count: scanned.error_count.filter(|n| *n > 0),
        first_error: scanned.first_error.clone(),
        file_errors: scanned
            .error_count
            .filter(|n| *n > 0)
            .map(|count| FileErrors {
                count,
                first: scanned.first_error.clone(),
            }),
        binary_skipped: None,
    }
}

/// Status and `terminalLimit` for a search result. `coverage_gap` means some
/// candidate content was not searched (unreadable paths, a binary cut on an
/// otherwise empty result): such a result is partial, never `empty`.
pub(crate) fn classify_search(
    empty: bool,
    capped: bool,
    has_more: bool,
    leftover_matches: bool,
    coverage_gap: bool,
    next_missing: bool,
) -> (SearchStatus, bool) {
    if empty && !coverage_gap {
        return (SearchStatus::Empty, false);
    }
    let partial = capped || has_more || leftover_matches || coverage_gap;
    let status = if partial {
        SearchStatus::Partial
    } else {
        SearchStatus::Success
    };
    // Later file pages may already be offered by the branch's first page.
    // Their absence here is not a cap; cursor ceilings are settled separately.
    let terminal = (capped || leftover_matches || coverage_gap) && next_missing;
    (status, terminal)
}
