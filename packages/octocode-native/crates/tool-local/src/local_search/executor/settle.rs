//! The row of one page: its status, coverage warnings, leads, totals and
//! the limits it ends at.
use crate::local_search::local_search_query::View;

use super::super::coverage::Coverage;
use super::super::layout::Layout;
use super::super::leads::{BINARY_LISTING, Found, parent_folder_lead, restart_lead, scope_listing};
use super::super::types::{FileErrors, LocalSearchResult, Paging, SearchStats, SearchStatus};
use super::super::verify::PageScope;
use super::projection::Annotated;

use super::super::local_search_query;
use octocode_contract_types::LocalSearchQuery;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::response::result::ToolError;
use serde_json::{Value, json};

/// One cut and verified page, as [`settle_row`] reads it.
pub(super) struct Page<'a> {
    /// The validated search root (a file or a directory).
    pub(super) root: &'a std::path::Path,
    pub(super) layout: &'a Layout,
    pub(super) scanned: &'a octocode_engine::types::TextSearchStats,
    pub(super) skipped: &'a octocode_runtime_base::policy::discovery::WalkSkips,
    /// The empty page's other readings of the request.
    pub(super) alternates: &'a super::super::probes::Alternates<'a>,
    /// Files the scan collected, on every page.
    pub(super) total_files: u32,
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
        ..
    } = *scope;
    let (layout, total_files) = (page.layout, page.total_files);
    // A walk page that finds no row after earlier pages showed some is where
    // the walk ended: the page before could not know it had shown the last
    // row without reading on, so it offered this one. It is no miss.
    let walk_ended =
        total_files == 0 && layout.walk.is_some_and(|walk| walk.continued && !walk.more);
    let empty = total_files == 0 && !page.classified_hits && !walk_ended;
    // A walk's page counts what the walk read so far: what the pages before
    // it counted (carried by its cursor) and its own scan. Its totals are
    // the search's once the walk ends; until then they are absent.
    let walked = layout
        .walk
        .map(|walk| walk.carried.counted.stated(page.scanned));
    let mut stats = search_stats(
        local_search_query::view(query),
        walked.as_ref().unwrap_or(page.scanned),
        total_files,
    );
    if let Some(walk) = layout.walk {
        if walk.more {
            stats.total_occurrences = None;
            stats.matched_lines = None;
            stats.matched_files = None;
        }
        // A page before missed content: the walk's file count is not final.
        if walk.carried.counted.gap {
            stats.matched_files = None;
        }
    }
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
    if walk_ended {
        coverage.scope_miss = false;
    }
    // The page before found rows here it did not show, and none are left:
    // the file holding them changed since that page read it.
    let rows_gone = walk_ended && layout.walk.is_some_and(|walk| walk.carried.rows_owed);
    for reason in coverage.partial_reasons() {
        if !stats.partial_reasons.contains(&reason) {
            stats.partial_reasons.push(reason);
        }
    }
    let mut warnings = coverage.warnings(layout, rows.shown_redacted);
    if rows_gone {
        warnings.push(
            "The hits the page before left for this page are gone: their file changed since that page read it, and the walk ended."
                .to_owned(),
        );
    }
    warnings.extend(rows.notes);
    // A sparse checkout holds only some of its repository: a miss here is
    // no absence from it.
    warnings.extend(crate::sparse_scope::coverage(page.root));
    let capped = stats.capped.unwrap_or(false);
    let (status, settled_terminal) = settle(
        query,
        layout,
        empty,
        capped,
        coverage.gap(),
        continuation_missing,
    );
    let mut hints = coverage.empty_hints(empty);
    // A key path that left out every file in scope: the same search without
    // it reads them.
    if empty && coverage.key_path_miss.is_some() {
        let lead = restart_lead(query, json!({"keyPath": null}))?;
        lead_first(&mut next, "withoutKeyPath", lead);
    }
    if empty
        && stats.files_searched > 0
        && let Some(hint) = page.alternates.case_hint(query)
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
        limits: security.limits(),
    }
    .add_leads(&mut stats, &mut next, &mut hints, &mut warnings)?;
    if disclosed.skipped_match {
        coverage.skipped_paths_hold_matches(&mut hints);
    }
    // A zero-match search cut by a binary file proved nothing about the cut
    // files: the tree of the scope is the way to narrow it.
    let lists_scope =
        (coverage.scope_miss || (empty && coverage.binary_cut)).then_some(!coverage.scope_miss);
    route_beyond_pages(query, lists_scope, &mut next);
    // A plain miss (nothing skipped, filtered or cut explains it) under a
    // narrower path: the same search one folder up is the step, offered and
    // not run. Skipped binaries hold no text the search could match, so their
    // listing never stands in for this lead: it follows it.
    let mut beyond = None;
    if status == SearchStatus::Empty
        && only_binary_listing(next.as_ref())
        && coverage.plain_miss()
        && let Some(call) = parent_folder_lead(query, paths, page.root)?
    {
        lead_first(&mut next, "broadenSearch", call);
        beyond = Some("related[] searches one folder up".to_owned());
    }
    // An empty page that searched files and whose skipped paths hold nothing
    // says what it covered in one sentence, before any other fact.
    if status == SearchStatus::Empty && stats.files_searched > 0 && !disclosed.skipped_match {
        let absence = coverage.absence(
            local_search_query::view(query),
            (stats.files_searched, stats.binary_skipped.unwrap_or(0)),
            beyond,
        );
        hints.insert(0, absence);
    }
    let is_partial = coverage.gap() || capped;
    // A partial scan's file count is not final.
    if is_partial {
        stats.matched_files = None;
    }
    Ok(LocalSearchResult {
        status,
        stats,
        unique_values: None,
        files: rows.files,
        pagination: file_paging(query, layout, total_files),
        hints,
        next,
        // The requested scope was searched whole. Skipped paths it did not
        // search are outside it, and the row names them.
        is_partial,
        // A retry over the skipped paths reaches the match, so no limit ends
        // this answer.
        terminal_limit: settled_terminal && !disclosed.skipped_match,
        warnings,
        skipped_paths: disclosed.skipped_paths,
        source_snapshot: None,
        selection: None,
        derived_value: None,
    })
}

/// Whether `next` holds no lead that explains the miss: none, or only the
/// listing of skipped binaries and the other readings offered on any miss
/// (case ignored, the terms as a regex), which claim nothing found.
fn only_binary_listing(next: Option<&Value>) -> bool {
    next.and_then(Value::as_object).is_none_or(|calls| {
        calls
            .keys()
            .all(|name| matches!(name.as_str(), BINARY_LISTING | "asInsensitive" | "asRegex"))
    })
}

/// `lead` as `next`'s first call, before the ones already there.
fn lead_first(next: &mut Option<Value>, name: &str, lead: Value) {
    let mut calls = serde_json::Map::new();
    calls.insert(name.to_owned(), lead);
    if let Some(Value::Object(earlier)) = next.take() {
        calls.extend(earlier);
    }
    *next = Some(Value::Object(calls));
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

/// File paging, reported only when it routes somewhere: more pages, or a
/// requested page past the end. It counts files on list pages and hit rows
/// (lines, or matches on matchOnly pages) on the others; the cursor that
/// reads on lives in `next.*`.
fn file_paging(query: &LocalSearchQuery, layout: &Layout, total_files: u32) -> Option<Paging> {
    let page = local_search_query::page(query).max(1);
    let has_more = page < layout.total_pages;
    if let Some(walk) = layout.walk {
        let unit = if layout.list {
            "files"
        } else {
            layout.row_unit
        };
        // A continued walk states where it stands on every page, the one it
        // ended on without a row too.
        return (walk.continued || (total_files > 0 && walk.more)).then(|| {
            Paging::of(
                unit,
                (layout.offset, layout.length, layout.total_items as usize),
                walk.more,
                false,
            )
            .floor(walk.more)
        });
    }
    let unit = if layout.list {
        "files"
    } else {
        layout.row_unit
    };
    (total_files > 0 && (has_more || layout.total_pages > 1 || layout.out_of_range)).then(|| {
        Paging::of(
            unit,
            (layout.offset, layout.length, layout.total_items as usize),
            has_more,
            layout.out_of_range,
        )
    })
}

/// The row's status, and whether it ends at a limit nothing continues.
fn settle(
    query: &LocalSearchQuery,
    layout: &Layout,
    empty: bool,
    capped: bool,
    coverage_gap: bool,
    next_missing: bool,
) -> (SearchStatus, bool) {
    let has_more = local_search_query::page(query).max(1) < layout.total_pages;
    classify_search(empty, capped, has_more, coverage_gap, next_missing)
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
    coverage_gap: bool,
    next_missing: bool,
) -> (SearchStatus, bool) {
    if empty && !coverage_gap {
        return (SearchStatus::Empty, false);
    }
    let partial = capped || has_more || coverage_gap;
    let status = if partial {
        SearchStatus::Partial
    } else {
        SearchStatus::Success
    };
    // Later file pages may already be offered by the branch's first page.
    // Their absence here is not a cap.
    let terminal = (capped || coverage_gap) && next_missing;
    (status, terminal)
}
