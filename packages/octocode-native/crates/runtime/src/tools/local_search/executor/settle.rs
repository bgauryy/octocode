//! The row of one page: its status, coverage warnings, leads, totals and
//! the limits it ends at.
use crate::tools::local_search::local_search_query::View;

use super::super::coverage::{Beyond, Coverage};
use super::super::layout::Layout;
use super::super::leads::{BINARY_LISTING, Found, parent_folder_lead, restart_lead, scope_listing};
use super::super::probes::{Ancestor, matching_ancestor};
use super::super::types::{FileErrors, LocalSearchResult, Paging, SearchStats, SearchStatus};
use super::super::verify::PageScope;
use super::projection::Annotated;

use super::super::local_search_query;
use crate::execution::cancel::CancellationCheck;
use crate::response::result::ToolError;
use octocode_contract_types::LocalSearchQuery;
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
    // A streamed page counts only the files it walked: the totals are the
    // search's only when its one page ended the walk.
    if layout.walk.is_some_and(|walk| walk.more || walk.continued) {
        stats.total_occurrences = None;
        stats.matched_lines = None;
        stats.matched_files = None;
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
    // narrower path: one probe finds the narrowest folder above it where the
    // same search matches, and the lead goes there; when the probe cannot
    // tell, the same search one folder up is the step, said as such. Skipped
    // binaries hold no text the search could match, so their listing never
    // stands in for this lead: it follows it.
    let mut beyond = None;
    if status == SearchStatus::Empty && only_binary_listing(next.as_ref()) && coverage.plain_miss()
    {
        beyond = match matching_ancestor(page.search_options, query, paths, page.root, cancel) {
            Ancestor::Found {
                dir,
                holding,
                files,
                floor,
                lifted,
            } => {
                let mut fields = json!({ "path": dir });
                for cause in &lifted {
                    let (field, value) = cause.flag();
                    fields[field] = value;
                }
                let call = restart_lead(query, fields)?;
                lead_first(&mut next, "broadenSearch", call);
                let floor = if floor { "at least " } else { "" };
                let what = if local_search_query::view(query) == View::FilesWithout {
                    "lack a match"
                } else {
                    "match"
                };
                let within = holding
                    .map(|holding| format!(", all in {holding}"))
                    .unwrap_or_default();
                Some(Beyond::Step(format!(
                    "{floor}{files} file(s) under {dir} {what}{within}{}: related[] searches there",
                    lifted_note(&lifted)
                )))
            }
            Ancestor::Nowhere { tree: None } => Some(Beyond::Nowhere(
                "nor anywhere else in the workspace under the same filters".to_owned(),
            )),
            Ancestor::Nowhere { tree: Some(tree) } => Some(Beyond::Nowhere(format!(
                "nor anywhere else under {tree} under the same filters"
            ))),
            Ancestor::Unknown => match parent_folder_lead(query, paths, page.root)? {
                Some(call) => {
                    lead_first(&mut next, "broadenSearch", call);
                    Some(Beyond::Step("related[] searches one folder up".to_owned()))
                }
                None => None,
            },
        };
    }
    // An empty page that searched files and whose skipped paths hold nothing
    // says what it covered in one sentence, before any other fact.
    if status == SearchStatus::Empty
        && stats.files_searched > 0
        && !disclosed.skipped_match
        && let Some(absence) = coverage.absence(
            local_search_query::view(query),
            (stats.files_searched, stats.binary_skipped.unwrap_or(0)),
            disclosed.checked.as_ref(),
            beyond,
        )
    {
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

/// What a widened search lifts, and why: the searched path is skipped by
/// those causes in the folder it widens to (` with includeIgnored: true (this
/// path is ignored there)`); empty when it lifts nothing.
fn lifted_note(lifted: &[crate::tools::empty_walk::Cause]) -> String {
    if lifted.is_empty() {
        return String::new();
    }
    let flags = lifted
        .iter()
        .map(|cause| {
            let (field, value) = cause.flag();
            format!("{field}: {value}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let causes = lifted
        .iter()
        .map(|cause| cause.label())
        .collect::<Vec<_>>()
        .join(" and ");
    format!(" with {flags} (this path is {causes} there)")
}

/// Whether `next` holds no lead, or only the listing of skipped binaries.
fn only_binary_listing(next: Option<&Value>) -> bool {
    next.and_then(Value::as_object)
        .is_none_or(|calls| calls.keys().all(|name| name == BINARY_LISTING))
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

/// File paging, reported only when it routes somewhere: more pages, rows
/// left in the shown files (`leftover_matches`), or a requested page past
/// the end. It counts files on list and grid pages and hit rows (lines, or
/// matches on matchOnly pages) on streamed pages; the cursor that reads on lives in `next.*`.
fn file_paging(
    query: &LocalSearchQuery,
    layout: &Layout,
    total_files: u32,
    leftover_matches: bool,
) -> Option<Paging> {
    let page = local_search_query::page(query).max(1);
    let has_more = page < layout.total_pages || leftover_matches;
    if let Some(walk) = layout.walk {
        let unit = if layout.list {
            "files"
        } else {
            layout.row_unit
        };
        return (total_files > 0 && (walk.more || walk.continued)).then(|| {
            Paging::of(
                unit,
                (layout.offset, layout.length, layout.total_items as usize),
                walk.more,
                false,
            )
            .floor(walk.more)
        });
    }
    let unit = if layout.list || !layout.streamed {
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
