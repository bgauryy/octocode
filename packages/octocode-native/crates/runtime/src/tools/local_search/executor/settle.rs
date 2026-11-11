//! The row of one page: its status, coverage warnings, leads, totals and
//! the limits it ends at.
use crate::tools::local_search::local_search_query::View;

use super::super::coverage::Coverage;
use super::super::layout::Layout;
use super::super::leads::{Found, restart_lead, scope_listing, workspace_root_lead};
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
    /// The empty page's other readings of the request.
    pub(super) alternates: &'a super::super::probes::Alternates<'a>,
    /// Files the scan collected, on every page.
    pub(super) total_files: u32,
    /// Some values were redacted because their source could not be re-read.
    pub(super) unverified: bool,
    /// Rows remain in the shown files.
    pub(super) leftover_matches: bool,
    /// Classification judged hits of this scan: a page it kept none of is
    /// no absence (its counts describe the kept hits, so they say zero).
    pub(super) classified_hits: bool,
}

/// The row of one page: its status, coverage warnings, leads and limits.
pub(super) fn settle_row(
    scope: &PageScope<'_, impl CancellationCheck>,
    page: &Page<'_>,
    rows: Annotated<'_>,
    mut next: Option<Value>,
    continuation_missing: bool,
) -> Result<LocalSearchResult, ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        security,
        cancel,
        ..
    } = *scope;
    let (layout, total_files) = (page.layout, page.total_files);
    let empty = total_files == 0 && !page.classified_hits;
    let mut stats = search_stats(local_search_query::view(query), page.scanned, total_files);
    // Rows name paths from the workspace; so does the first unreadable one.
    if let Some(errors) = &mut stats.file_errors {
        errors.first = errors
            .first
            .as_deref()
            .map(|first| paths.workspace_relative_text(first));
    }
    let mut coverage = Coverage::of(
        query,
        &stats,
        page.scanned,
        page.root,
        output_root,
        page.skipped,
    );
    for reason in coverage.partial_reasons() {
        if !stats.partial_reasons.contains(&reason) {
            stats.partial_reasons.push(reason);
        }
    }
    let mut warnings = coverage.warnings(layout, rows.shown_redacted, page.unverified);
    warnings.extend(rows.notes);
    let capped = stats.capped.unwrap_or(false);
    let (status, settled_terminal) = settle(
        query,
        layout,
        empty,
        capped,
        page.leftover_matches,
        coverage.gap(),
        continuation_missing,
    );
    let mut hints = coverage.empty_hints(empty);
    if empty
        && stats.files_searched > 0
        && let Some(hint) = page.alternates.case_hint(
            query,
            paths,
            page.search_options,
            status == SearchStatus::Empty,
            cancel,
        )
    {
        hints.push(hint);
        // The hint's advice as a call: the same scope with case ignored.
        let lead = restart_lead(query, json!({"caseMode": "insensitive"}))?;
        if let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut() {
            map.insert("asInsensitive".into(), lead);
        }
    }
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
        searched: page.alternates.listed.as_deref(),
        limits: security.limits(),
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
    route_beyond_pages(query, lists_scope, &mut next);
    // A plain miss (nothing skipped, filtered or cut explains it) under a
    // narrower path: the same search from the workspace root is the step.
    if status == SearchStatus::Empty
        && next.is_none()
        && coverage.plain_miss()
        && let Some(lead) = workspace_root_lead(query, paths, page.root)?
    {
        next = Some(json!({ "broadenSearch": lead }));
    }
    let is_partial = coverage.gap() || capped;
    // A partial scan's file count is not final.
    if is_partial {
        stats.matched_files = None;
    }
    Ok(LocalSearchResult {
        status,
        stats,
        files: rows.files,
        pagination: file_paging(query, layout, total_files, page.leftover_matches),
        hints,
        next,
        // The requested scope was searched whole. What a probe of the skipped
        // paths left unchecked is outside it, and the row names it.
        is_partial,
        // A retry over the skipped paths reaches the match, so no limit ends
        // this answer.
        terminal_limit: settled_terminal && !disclosed.skipped_match,
        warnings,
        skipped_paths: disclosed.skipped_paths,
        source_snapshot: None,
        selection: None,
    })
}

/// The lead past the pages: a scope miss or an unproven empty result
/// (`lists_scope`) leads to the scope's listing, with the search's globs
/// when they matched files (`Some(true)`), without them when they matched
/// none, so the listing shows what names exist.
fn route_beyond_pages(
    query: &LocalSearchQuery,
    lists_scope: Option<bool>,
    next: &mut Option<Value>,
) {
    if let Some(keep_globs) = lists_scope
        && let Some(listing) = scope_listing(query, keep_globs)
        && let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut()
    {
        map.insert("viewTree".into(), listing);
    }
    if next
        .as_ref()
        .is_some_and(|next| next.as_object().is_some_and(serde_json::Map::is_empty))
    {
        *next = None;
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

/// The row's status, and whether it ends at a limit nothing continues.
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
/// neither (its files hold no hit). A view with nothing listed counts zero;
/// an empty row drops those zeros ([`LocalSearchResult::to_row`]).
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
    let files_matched = scanned.files_matched.unwrap_or(total_files);
    let whole_scan = !scanned.capped.unwrap_or(false) && scanned.error_count.unwrap_or(0) == 0;
    let shows_hits = matches!(
        view,
        View::Content | View::MatchOnly | View::CountLines | View::CountMatches
    );
    SearchStats {
        total_occurrences: counted(scanned.match_count),
        matched_lines: counted(scanned.matched_lines),
        matched_files: (shows_hits && whole_scan).then_some(files_matched),
        files_matched,
        files_searched: scanned.files_searched.unwrap_or(0),
        // A binary cut (capReason `binaryQuit`) stays `capped`: coverage ended
        // early, and the result is marked partial/terminal.
        capped: scanned.capped,
        cap_reasons: scanned.cap_reasons.clone(),
        partial_reasons: scanned
            .cap_reasons
            .iter()
            .copied()
            .map(super::super::types::cap_partial_reason)
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
        test_code_excluded: scanned
            .test_code_excluded
            .and_then(octocode_engine::types::TestCodeExcluded::stated),
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
    // Their absence here is not a cap.
    let terminal = (capped || leftover_matches || coverage_gap) && next_missing;
    (status, terminal)
}
