use super::types::*;
use super::{
    coverage::*, cursor::*, enclosing::*, handoff::*, layout::*, leads::*, regex_repair::*,
    rows::*, verify::*,
};
use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::response::result::ToolError;
use crate::security::ContentSecurity;
use octocode_contract_types::{ErrorCode, OutputPaginationVariant0Unit};
use octocode_engine::search::text_search::search_cancellable;
use octocode_engine::types::{
    CaseRule, PatternSyntax, SearchOutput, SearchSort, SourceValues, SpanValues, TextSearchOptions,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// Soft budget (in value chars) for a single localSearch response body.
/// Distributed across the matches shown on a page so a pathological giant line
/// or a raised `matchContentLength`/`matchPageSize` cannot emit a multi-MB
/// body. It bounds displayed value size only: every match row and its line
/// anchor are preserved, and a clipped row's `expandValues` lead reads its
/// value whole.
pub(super) const RESPONSE_VALUE_CHAR_BUDGET: usize = 1_000_000;

/// Floor on the per-match display cap the budget may impose, so a page with many
/// matches still shows a useful slice of each rather than a few characters.
pub(super) const MIN_MATCH_VALUE_CHARS: usize = 40;

/// Lean agent-facing defaults (caller values always win). A/B over realistic
/// locate-the-code queries kept every target file+line on page 1 while cutting
/// default response bytes ~54% versus the old 100 files / 20 rows / 500 chars /
/// ±2 lines: one clipped-around-the-hit line per row is enough to pick the
/// file+line, and `detailed`/`contextLines` or localFetch add surrounding code.
pub(super) const DEFAULT_MATCH_CONTENT_LENGTH: u32 = 200;

/// Cap on the context-scaled default; an explicit matchContentLength may exceed it.
pub(super) const MAX_DEFAULT_MATCH_CONTENT_LENGTH: u32 = 4000;

/// Rows per file when a result is too large to show whole.
pub(super) const DEFAULT_MAX_MATCHES_PER_FILE: u32 = 10;

/// Files per page for snippet views (the first page of a default-layout
/// result larger than the budget); path-only list views stay at 100.
pub(super) const DEFAULT_SNIPPET_PAGE_SIZE: u32 = 20;

pub(super) const DEFAULT_LIST_PAGE_SIZE: u32 = 100;

/// Matched files the engine collects for pages; more are counted only.
pub(super) const MAX_COLLECTED_FILES: u32 = 10_000;

/// One localSearch row; `selection` judges it under `clasifyContext`. A list
/// `matchString` runs as the one regex query that finds the same lines.
pub fn execute_local_search(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
    walk_threads: Option<u32>,
    response_window: Option<usize>,
    selection: Option<&crate::tools::selection::Runtime>,
) -> Result<LocalSearchResult, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let query = &*local_search_query::normalized(query)?;
    let scan = scan_query(query, paths, cancel, walk_threads, response_window)?;
    page_of(
        query,
        paths,
        security,
        cancel,
        walk_threads,
        scan,
        selection,
    )
}

/// The page `query` asks for, cut from `scan` (fresh, or stored and proved
/// unchanged when [`scan_query`] took it). Every file the page reads or
/// shows is checked again here, so a file changed since then restarts the
/// snapshot or is denied rather than shown.
pub(super) fn page_of(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
    walk_threads: Option<u32>,
    mut scan: Scanned,
    selection: Option<&crate::tools::selection::Runtime>,
) -> Result<LocalSearchResult, ToolError> {
    // The bytes a stored scan's secret check must read, or `None` when the
    // scan is fresh; a stored file whose digest is missing reads as changed.
    let digests = scan.digests.take();
    let expected_digest = |source: &std::path::Path| -> Option<Option<super::manifest::Digest>> {
        digests.as_ref().map(|digests| digests.get(source).copied())
    };
    let root = &scan.validated.canonical;
    let output_root = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root.as_path()
    };
    let scope = PageScope {
        query,
        paths,
        output_root,
        expected_digest: &expected_digest,
        security,
        cancel,
        walk_threads,
    };
    let shaped = shape_page(
        &scope,
        &mut scan.parsed,
        scan.page_budget,
        scan.stored_redaction.as_deref(),
        &scan.name_prices,
        selection,
        &scan.identity,
    )?;
    let total_files = scan.parsed.files.len() as u32;
    let reusable = scan
        .reusable
        .take()
        .zip(shaped.packed)
        .map(|(walked, value)| super::manifest::Fresh {
            value,
            query_key: std::mem::take(&mut scan.query_key),
            redaction: shaped.redaction,
            skipped: Arc::clone(&scan.skipped),
            walked,
            prices: Arc::clone(&scan.name_prices),
        });
    let selection_token = shaped
        .selection
        .as_ref()
        .map(super::selection::Selected::token);
    let (next, leftover_matches) = cursor(
        query,
        &scan.parsed,
        &shaped.layout,
        &scan.identity,
        reusable,
        std::mem::take(&mut scan.policy_key),
        selection_token,
        &|| cancel.check().is_err(),
    );
    cancel.check().map_err(ToolError::cancelled)?;
    let rows = annotate_rows(
        &scope,
        &shaped.layout,
        std::mem::take(&mut scan.parsed.files),
        &shaped.redacted,
        shaped.outlines,
        scan.page_budget,
    );
    let page = Page {
        root,
        layout: &shaped.layout,
        scanned: &scan.parsed.stats,
        skipped: &scan.skipped,
        search_options: &scan.search_options,
        total_files,
        unverified: shaped.unverified,
        leftover_matches,
    };
    let mut result = settle_row(&scope, &page, rows, next)?;
    if let Some(selected) = &shaped.selection {
        let leads = selected.leads(query, &scan.identity, !root.is_file());
        if !leads.is_empty() {
            let next = result.next.get_or_insert_with(|| json!({}));
            for (name, lead) in leads {
                next[name] = lead;
            }
        }
        // The pre-selection totals hold for every page: said on the first.
        if local_search_query::page(query).max(1) == 1
            && local_search_query::match_page(query).max(1) == 1
        {
            result.warnings.extend(selected.disclosure());
        }
        // Hits were judged and none kept: an empty row whose unfiltered
        // lead is its recovery (zero kept is not absence).
        if selected.receipt().total_items > 0 && result.files.is_empty() {
            result.status = SearchStatus::Empty;
        }
    }
    result.source_snapshot = Some(scan.identity);
    result.selection = shaped.selection.map(|selection| selection.outcome.receipt);
    Ok(result)
}

/// One cut and verified page, as [`settle_row`] reads it.
struct Page<'a> {
    /// The validated search root (a file or a directory).
    root: &'a std::path::Path,
    layout: &'a Layout,
    scanned: &'a octocode_engine::types::TextSearchStats,
    skipped: &'a crate::policy::discovery::WalkSkips,
    /// The query's own walk, for probes over the same scope.
    search_options: &'a TextSearchOptions,
    /// Files the scan collected, on every page.
    total_files: u32,
    /// Some values were redacted because their source could not be re-read.
    unverified: bool,
    /// Rows remain in the shown files.
    leftover_matches: bool,
}

/// The row of one page: its status, coverage warnings, leads and limits.
fn settle_row(
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
        && !(query.clasify_context.is_some() && page.scanned.files_matched.unwrap_or(0) > 0);
    let mut stats = search_stats(query.result_view, page.scanned, total_files);
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
        source_root: output_root.to_path_buf(),
        selection: None,
    })
}

/// The page's rows, projected and annotated: each hit's match keys and
/// enclosing declaration, and the searched symbol's declaration among them.
struct Annotated<'q> {
    files: Vec<SearchFile>,
    /// Shown values redaction changed.
    shown_redacted: usize,
    symbol: Option<&'q str>,
    definition: Option<Definition>,
    /// Warnings the annotations add.
    notes: Vec<String>,
}

/// Project the page's shown rows and annotate them (see [`Annotated`]).
fn annotate_rows<'q>(
    scope: &PageScope<'q, impl CancellationCheck>,
    layout: &Layout,
    scanned: Vec<octocode_engine::types::TextSearchFile>,
    redacted: &Redacted,
    outlines: Outlines,
    page_budget: usize,
) -> Annotated<'q> {
    let PageScope {
        query,
        output_root,
        expected_digest,
        security,
        ..
    } = *scope;
    let (mut files, shown_redacted) = project_files(query, layout, scanned, redacted);
    let symbol = searched_symbol(query, layout);
    // Rows render with the root's prefix, which the page cut charged.
    let keys_budget = page_budget.saturating_sub(layout.prefix_chars.saturating_mul(files.len()));
    let mut notes = Vec::new();
    notes.extend(annotate_match_keys(
        &mut files,
        output_root,
        keys_budget,
        expected_digest,
        security,
    ));
    let (definition, enclosing_note) = annotate_enclosing(
        &mut files,
        symbol,
        symbol.filter(|_| super::enclosing::declaration_search(local_search_query::pattern(query))),
        output_root,
        expected_digest,
        outlines,
        security,
    );
    notes.extend(enclosing_note);
    Annotated {
        files,
        shown_redacted,
        symbol,
        definition,
        notes,
    }
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

/// The symbol a page names declarations for: a small page names each hit's
/// enclosing declaration, and a searched symbol declared on a shown hit
/// line gets an lspSearch lead.
pub(super) fn searched_symbol<'q>(query: &'q LocalSearchQuery, layout: &Layout) -> Option<&'q str> {
    (!layout.list
        && query.case_mode != LocalSearchQueryCaseMode::Insensitive
        && local_search_query::regex_mode(query) != LocalSearchQueryRegex::Pcre2
        && query.invert_match != Some(true))
    .then(|| super::enclosing::searched_symbol(local_search_query::pattern(query)))
    .flatten()
}

/// File paging, reported only when it routes somewhere: more file pages,
/// rows left in the shown files (`leftover_matches`), or a requested page
/// past the end. Totals live once, in `stats` (occurrences and matched
/// lines); the next page number and the snapshot live once, in
/// `next.*.query`.
pub(super) fn file_paging(
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
        (query.result_view != LocalSearchQueryResultView::MatchOnly)
            .then_some(OutputPaginationVariant0Unit::Lines)
    };
    (total_files > 0 && (has_more || layout.total_pages > 1 || layout.out_of_range)).then_some(
        FilePagination {
            unit,
            current_page: page,
            match_page: (match_page > 1).then_some(match_page),
            total_pages: layout.total_pages,
            files_per_page: layout.files_per_page,
            total_items: layout.total_items,
            has_more,
            out_of_range: layout.out_of_range,
        },
    )
}

/// A cut page whose shown values are verified.
pub(super) struct Shaped {
    pub(super) packed: Option<super::packed::Packed>,
    pub(super) layout: Layout,
    pub(super) redacted: Redacted,
    /// What redacting the scan changed, for a stored scan's later pages.
    pub(super) redaction: Redaction,
    /// Some values were redacted because their source could not be re-read.
    pub(super) unverified: bool,
    /// Outlines parsed from the verified reads (see [`verify_shown`]).
    pub(super) outlines: Outlines,
    pub(super) selection: Option<super::selection::Selected>,
}

/// Redact the scan, order it, cut the page, and verify what it shows.
///
/// Every file the page reads or shows passes the read policy first: the
/// key-block reads inside [`redact_scan`] and each shown file here. A file
/// the page neither reads nor shows is checked on the page that shows it,
/// so a path swapped for a symlink after the walk never has its bytes read
/// or its rows shown.
pub(super) fn shape_page(
    scope: &PageScope<'_, impl CancellationCheck>,
    parsed: &mut octocode_engine::types::TextSearchResult,
    page_budget: usize,
    stored_redaction: Option<&Redaction>,
    name_prices: &super::manifest::NamePrices,
    selection: Option<&crate::tools::selection::Runtime>,
    identity: &str,
) -> Result<Shaped, ToolError> {
    let PageScope {
        query,
        paths,
        output_root,
        ..
    } = *scope;
    let (mut redacted, redaction) = redact_scan(scope, parsed, stored_redaction)?;
    let packed = if stored_redaction.is_none() {
        super::manifest::pack_checked(parsed, &|| scope.cancel.check().is_err())
    } else {
        None
    };
    order_files(query, &mut parsed.files);
    let selection =
        super::selection::select_hits(scope, parsed, selection, identity, &mut redacted)?;
    let mut cut = CutOutlines::new();
    let mut outline = |files: &[(usize, &octocode_engine::types::TextSearchFile)]| {
        outline_for_cut(scope, files, &mut cut)
    };
    let mut pricing = NamePricing::new(name_prices.by_index(&parsed.files), &mut outline);
    let layout = Layout::cut(query, paths, output_root, parsed, page_budget, &mut pricing)?;
    name_prices.learn(&parsed.files, pricing.priced);
    for (index, _) in &layout.shown {
        validate_matched(paths, &output_root.join(&parsed.files[*index].path))?;
    }
    let mut outlines = Outlines::new();
    let unverified = verify_shown(
        scope,
        parsed,
        &layout,
        &mut redacted,
        (&mut cut, &mut outlines),
    )?;
    Ok(Shaped {
        packed,
        layout,
        redacted,
        redaction,
        unverified,
        outlines,
        selection,
    })
}

/// The row's status, and whether it ends at a limit nothing continues
/// (a cursor past its maximum is [`cursor_ceilings`]).
pub(super) fn settle(
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

/// A scan with what every later stage needs from it.
pub(super) struct Scanned {
    pub(super) validated: crate::policy::path::ValidatedPath,
    pub(super) parsed: octocode_engine::types::TextSearchResult,
    pub(super) digests: ScanDigests,
    /// Chars one page's rows may take.
    pub(super) page_budget: usize,
    /// The result's snapshot identity.
    pub(super) identity: String,
    /// The query key `identity` was derived from.
    pub(super) query_key: String,
    /// A stored scan's redaction to replay (see [`redact_scan`]).
    pub(super) stored_redaction: Option<Arc<Redaction>>,
    /// The names a stored scan's cuts priced, or a fresh scan's empty set.
    pub(super) name_prices: Arc<super::manifest::NamePrices>,
    /// A fresh walk offered for storage with the priced, unranked scan.
    pub(super) reusable: Option<super::manifest::Walked>,
    /// The query's own walk, for probes over the same scope.
    pub(super) search_options: TextSearchOptions,
    pub(super) policy_key: String,
    /// What the walk left out: policy-withheld, ignored, default-excluded
    /// and hidden entries.
    pub(super) skipped: Arc<crate::policy::discovery::WalkSkips>,
}

/// Plan and run the walk (or reuse its stored scan) and prove a cursor
/// still answers this query.
pub(super) fn scan_query(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    cancel: &impl CancellationCheck,
    walk_threads: Option<u32>,
    response_window: Option<usize>,
) -> Result<Scanned, ToolError> {
    if let Some(error) = malformed_snapshot(query) {
        return Err(error);
    }
    let validated = search_root(query, paths)?;
    let options = search_options(query, &validated.canonical, walk_threads);
    check_pattern(query)?;
    check_language(query)?;
    let search_options = options.clone();
    let policy_key = paths.identity();
    let ScanOutput {
        parsed,
        digests,
        skipped,
        stored,
        walked,
    } = scan(query, paths, options, &policy_key, cancel)?;
    // Pages are cut from serialized sizes so a default-layout page, with
    // the row around it, fits one response window. Sized by the canonical
    // root, not its spelling: a continuation names the same root relative
    // to the workspace and must cut the same pages.
    let mut sized = normalized_query(
        &local_search_query::canonical(query),
        streamed_layout(query),
    );
    sized["path"] = json!(validated.canonical.to_string_lossy());
    let page_budget = crate::response::page_windows::page_budget(response_window, &sized);
    // A stored scan was stored under the snapshot derived from its query
    // key and its unchanged collected result: a query with the same key
    // derives that snapshot again, so only another key needs the digest.
    let query_key = query_key(query, &validated.canonical, page_budget);
    let (identity, stored_redaction, name_prices) =
        match (stored, local_search_query::snapshot(query)) {
            (Some(stored), Some(snapshot)) if stored.query_key == query_key => {
                (snapshot.to_owned(), stored.redaction, stored.prices)
            }
            (Some(_), Some(_)) => return Err(stale_snapshot(query)),
            _ => (
                fingerprint(&query_key, &parsed.files, &parsed.stats),
                None,
                Arc::default(),
            ),
        };
    // A cached scan is checked too: its identity comes from the submitted
    // query's key, so a cursor reused with different search semantics never
    // serves the old query's matches.
    if local_search_query::snapshot(query).is_some_and(|expected| expected != identity) {
        return Err(stale_snapshot(query));
    }
    let reusable = walked;
    Ok(Scanned {
        validated,
        parsed,
        digests,
        page_budget,
        identity,
        query_key,
        stored_redaction,
        name_prices,
        reusable,
        search_options,
        policy_key,
        skipped,
    })
}

/// The search root under the path policy; a missing one is not-found
/// (exit 3), not an I/O failure, and leads to a tree of its nearest
/// existing parent, where a typo's siblings show.
pub(super) fn search_root(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
) -> Result<crate::policy::path::ValidatedPath, ToolError> {
    paths
        .validate(query.path.as_str())
        .map_err(|error| ToolError::root_policy(error, query.path.as_str(), paths))
}

/// The engine walk the query asks for.
pub(super) fn search_options(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    walk_threads: Option<u32>,
) -> TextSearchOptions {
    let view = query.result_view;
    let path_list = matches!(
        view,
        LocalSearchQueryResultView::Files | LocalSearchQueryResultView::FilesWithout
    );
    let multiline = query.multiline;
    TextSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: local_search_query::pattern(query).to_owned(),
        syntax: match local_search_query::regex_mode(query) {
            LocalSearchQueryRegex::Literal => PatternSyntax::Literal,
            LocalSearchQueryRegex::Rust => PatternSyntax::Regex,
            LocalSearchQueryRegex::Pcre2 => PatternSyntax::Pcre2,
        },
        case: match query.case_mode {
            LocalSearchQueryCaseMode::Smart => CaseRule::Smart,
            LocalSearchQueryCaseMode::Sensitive => CaseRule::Sensitive,
            LocalSearchQueryCaseMode::Insensitive => CaseRule::Insensitive,
        },
        whole_word: query.whole_word,
        invert_match: query.invert_match,
        multiline: Some(multiline != LocalSearchQueryMultiline::Off),
        multiline_dotall: Some(multiline == LocalSearchQueryMultiline::Dotall),
        output: match view {
            LocalSearchQueryResultView::Files => SearchOutput::FilesWithMatch,
            LocalSearchQueryResultView::FilesWithout => SearchOutput::FilesWithoutMatch,
            LocalSearchQueryResultView::CountLines => SearchOutput::CountLines,
            LocalSearchQueryResultView::CountMatches => SearchOutput::CountMatches,
            LocalSearchQueryResultView::MatchOnly => SearchOutput::Spans(match query.unique {
                LocalSearchQueryUnique::Off => SpanValues::Every,
                LocalSearchQueryUnique::List => SpanValues::Unique,
                LocalSearchQueryUnique::Count => SpanValues::Counted,
            }),
            LocalSearchQueryResultView::Paginated => SearchOutput::Lines,
        },
        context: local_search_query::context(query),
        lang_type: query.language.clone(),
        file_contains_all: Some(query.file_has.iter().map(|t| t.to_string()).collect())
            .filter(|terms: &Vec<String>| !terms.is_empty()),
        file_contains_none: Some(query.file_lacks.iter().map(|t| t.to_string()).collect())
            .filter(|terms: &Vec<String>| !terms.is_empty()),
        include: Some(crate::policy::include::include_globs(&query.include))
            .filter(|include| !include.is_empty()),
        // Credential-named files are withheld, and generated files skipped
        // (both counted), by the walk's path filter, not by globs here.
        exclude: Some(query.exclude.clone().into_iter().collect()),
        // The default-excluded directories are pruned by the walk's path
        // filter too (`WalkAdmission`), which keeps the ones git tracks.
        exclude_dir: None,
        // Each flag lifts its own cause, as in every local walk; the dot
        // entries left out are recorded by the walk's path filter.
        ignore: octocode_engine::search::walk::IgnorePolicy::new(
            query.include_ignored,
            query.include_hidden,
        ),
        max_depth: local_search_query::max_depth(query),
        // The engine owns the relevance order (for a bare-identifier search,
        // source files declaring the name first; then count, then source
        // before test/generated paths, then declaration > code >
        // comment/string hits, then path) so its top-k and cap keep the same
        // survivors.
        sort: match query.sort {
            LocalSearchQuerySort::Traversal => SearchSort::Traversal,
            LocalSearchQuerySort::Relevance => SearchSort::Relevance,
            LocalSearchQuerySort::Path => SearchSort::Path,
            LocalSearchQuerySort::Modified => SearchSort::Modified,
            LocalSearchQuerySort::Accessed => SearchSort::Accessed,
            LocalSearchQuerySort::Created => SearchSort::Created,
            // Match-count order chooses the collection cap's survivors by
            // match count across every searched file; path-list views rank
            // by path.
            LocalSearchQuerySort::MatchCount if path_list => SearchSort::Path,
            LocalSearchQuerySort::MatchCount => SearchSort::MatchCount,
        },
        sort_reverse: query.reverse,
        max_snippet_chars: Some(effective_match_content_length(query)),
        // The engine keeps the first 10k matched files in the engine sort order
        // above (by match count for relevance/matchCount), chosen across every
        // searched file; stats totals still count all matched files and the
        // cap surfaces as capReason "maxCollectedFiles".
        max_collected_files: Some(MAX_COLLECTED_FILES),
        // Use the engine default per-file byte ceiling (skips pathological
        // multi-GB files, surfaced as a maxFileSize diagnostic).
        max_file_bytes: None,
        // The batch's share of the cores (see `BatchBudget`); `None` walks
        // on every core.
        walk_threads,
        // A scan small enough to store for later pages keeps the digest of
        // each matched file from the read that searched it (see `manifest`).
        values: SourceValues::Deferred,
    }
}

/// A scan and the digests of its matched files when it was stored.
pub(super) type ScanDigests =
    Option<std::collections::HashMap<std::path::PathBuf, super::manifest::Digest>>;

/// A walk's matches, the stored digests a reused scan must still hash to,
/// and what the walk left out.
pub(super) struct ScanOutput {
    pub(super) parsed: octocode_engine::types::TextSearchResult,
    pub(super) digests: ScanDigests,
    pub(super) skipped: Arc<crate::policy::discovery::WalkSkips>,
    /// What was stored with a reused scan: its query key, redaction and
    /// name prices.
    pub(super) stored: Option<StoredDerived>,
    /// What a fresh walk searched; `None` for a reused scan.
    pub(super) walked: Option<super::manifest::Walked>,
}

/// What a stored scan's fresh page derived from it.
pub(super) struct StoredDerived {
    pub(super) query_key: String,
    pub(super) redaction: Option<Arc<Redaction>>,
    pub(super) prices: Arc<super::manifest::NamePrices>,
}

/// The matches: a continuation reuses its page-1 scan instead of walking
/// the tree again. A stored scan is reused only under the path policy that
/// produced it; the snapshot comparison then proves it answers this query.
/// The walk admitted each matched path under the read policy; a page checks
/// again each file it reads or shows (see [`shape_page`]).
pub(super) fn scan(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    options: TextSearchOptions,
    policy_key: &str,
    cancel: &impl CancellationCheck,
) -> Result<ScanOutput, ToolError> {
    let (parsed, digests, withheld, derived, walked) = if let Some(snapshot) =
        local_search_query::snapshot(query)
        && let Some(stored) =
            super::manifest::get_checked(snapshot, policy_key, &|| cancel.check().is_err())
    {
        let derived = StoredDerived {
            query_key: stored.query_key,
            redaction: stored.redaction,
            prices: stored.prices,
        };
        (
            stored.value,
            Some(stored.digests),
            stored.skipped,
            Some(derived),
            None,
        )
    } else {
        let walk = Arc::new(crate::policy::discovery::WalkAdmission::search(
            paths.clone(),
            std::path::PathBuf::from(&options.path),
            query.default_excludes.defaults(),
            options.include.as_deref().unwrap_or_default(),
        ));
        let recorder = Arc::new(super::walked::Recorder::new(
            Arc::clone(&walk),
            paths.clone(),
            std::path::PathBuf::from(&options.path),
            query.include_ignored != Some(true),
            super::manifest::MAX_BYTES,
        ));
        let parsed = search_cancellable(options, recorder.clone(), &|| cancel.check().is_err())
            .map_err(|error| {
                // A malformed glob or file type is caller input.
                let code = match error.kind() {
                    octocode_engine::error::ErrorKind::InvalidInput => ErrorCode::InvalidInput,
                    octocode_engine::error::ErrorKind::InvalidGlob => ErrorCode::InvalidGlob,
                    _ => ErrorCode::ExecutionFailed,
                };
                ToolError::new(code, error.to_string())
            })?;
        (
            parsed,
            None,
            Arc::new(walk.skipped()),
            None,
            Some(recorder.walked()),
        )
    };
    cancel.check().map_err(ToolError::cancelled)?;
    if parsed.files.is_empty()
        && parsed.stats.files_searched.unwrap_or(0) == 0
        && parsed.stats.error_count.unwrap_or(0) > 0
    {
        return Err(unreadable_scope(&parsed.stats));
    }
    Ok(ScanOutput {
        parsed,
        digests,
        skipped: withheld,
        stored: derived,
        walked,
    })
}

fn is_count_view(query: &LocalSearchQuery) -> bool {
    matches!(
        query.result_view,
        LocalSearchQueryResultView::CountMatches | LocalSearchQueryResultView::CountLines
    )
}

/// The runtime's own orders (path, matchCount), then `reverse`. Engine-side
/// time and relevance sorts already honour `reverse`; every order the
/// runtime (re)establishes — path, matchCount, traversal — is reversed here,
/// as the schema promises ("after sort, before pagination").
pub(super) fn order_files(
    query: &LocalSearchQuery,
    files: &mut [octocode_engine::types::TextSearchFile],
) {
    let sort = query.sort;
    match sort {
        LocalSearchQuerySort::Path => files.sort_by(|a, b| a.path.cmp(&b.path)),
        LocalSearchQuerySort::MatchCount => files.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.path.cmp(&b.path))
        }),
        // A count view under relevance ranks the most hits first; the stable
        // sort keeps the engine's relevance order (declaration before code,
        // comment, string) among equal counts. Under `reverse` the engine
        // already reversed that order, so the fewest hits come first with
        // ties left in the engine's reversed order: the exact reverse of the
        // forward listing.
        LocalSearchQuerySort::Relevance if is_count_view(query) => {
            if query.reverse.unwrap_or(false) {
                files.sort_by_key(|file| file.match_count);
            } else {
                files.sort_by_key(|file| std::cmp::Reverse(file.match_count));
            }
        }
        _ => {}
    }
    // Relevance (count views included, above) and the time sorts arrive in
    // order with `reverse` applied.
    let engine_sorted = matches!(
        sort,
        LocalSearchQuerySort::Modified
            | LocalSearchQuerySort::Accessed
            | LocalSearchQuerySort::Created
            | LocalSearchQuerySort::Relevance
    );
    if query.reverse.unwrap_or(false) && !engine_sorted {
        files.reverse();
    }
}

/// The response's totals from the scan, in the units the view counted:
/// countLines counts no occurrences, and a filesWithout listing counts
/// neither (its files hold no hit). A view with nothing listed states zero.
pub(super) fn search_stats(
    view: LocalSearchQueryResultView,
    scanned: &octocode_engine::types::TextSearchStats,
    total_files: u32,
) -> SearchStats {
    let counted = |count: Option<u32>| {
        let listed_without = view == LocalSearchQueryResultView::FilesWithout && total_files > 0;
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

/// The page's file entries, and how many shown values were redacted.
pub(super) fn project_files(
    query: &LocalSearchQuery,
    layout: &Layout,
    scanned: Vec<octocode_engine::types::TextSearchFile>,
    redacted: &Redacted,
) -> (Vec<SearchFile>, usize) {
    let match_only = query.result_view == LocalSearchQueryResultView::MatchOnly;
    let match_page = local_search_query::match_page(query).max(1);
    let mut shown_redacted = 0usize;
    let mut slots = scanned.into_iter().map(Some).collect::<Vec<_>>();
    let files = layout
        .shown
        .iter()
        .filter_map(|(index, rows)| Some((slots.get_mut(*index)?.take()?, rows.clone())))
        .map(|(f, rows)| {
            // One key per file, its line and column set per row.
            let mut key = (f.path.clone(), 0, 0);
            let total = f.matches.len() as u32;
            let end = rows.end.min(f.matches.len());
            let start = rows.start.min(end);
            let mut page_rows = f.matches[start..end].iter().collect::<Vec<_>>();
            page_rows.sort_by_key(|m| (m.line, m.column));
            let shown = page_rows
                .into_iter()
                .inspect(|m| {
                    (key.1, key.2) = (m.line, m.column);
                    if redacted.contains(&key) {
                        shown_redacted += 1;
                    }
                })
                .map(|m| project_match(m, layout.display_cap))
                .map(|mut row| {
                    if !match_only {
                        row.column = None;
                    }
                    row
                })
                .collect::<Vec<_>>();
            // Rows on later pages, named by line so a reader can fetch them
            // directly instead of paging.
            let mut later = f.matches[end..].iter().map(|m| m.line).collect::<Vec<_>>();
            later.sort_unstable();
            later.dedup();
            let shown = match layout.merge_context {
                Some(context) => {
                    let mut hits = f.matches.iter().map(|m| m.line).collect::<Vec<_>>();
                    hits.sort_unstable();
                    hits.dedup();
                    merge_context_windows(
                        shown,
                        context,
                        effective_match_content_length(query) as usize,
                        &hits,
                    )
                }
                None => shown,
            };
            let span = (start, end, shown.len());
            SearchFile {
                matches: (!layout.list).then_some(shown),
                total_occurrences: (query.result_view == LocalSearchQueryResultView::CountMatches)
                    .then_some(f.match_count),
                total_matched_lines: (query.result_view == LocalSearchQueryResultView::CountLines)
                    .then_some(f.match_count),
                pagination: file_pagination(layout, total, span, &later, match_page),
                path: f.path,
                match_keys: Vec::new(),
            }
        })
        .collect::<Vec<_>>();
    // A later match page must not re-send files whose rows ended on an
    // earlier page as empty `outOfRange` rows. Keep them only when every
    // file on this page is exhausted, so a forged/stale matchPage still gets
    // its out-of-range diagnostic instead of a silent empty page.
    let exhausted = |file: &SearchFile| file.pagination.as_ref().is_some_and(|p| p.out_of_range);
    let mut files = files;
    if !layout.list && files.iter().any(|file| !exhausted(file)) {
        files.retain(|file| !exhausted(file));
    }
    (files, shown_redacted)
}

/// Every candidate failed before it could be searched: there is no evidence,
/// so this is an execution failure, not an empty result.
pub(super) fn unreadable_scope(stats: &octocode_engine::types::TextSearchStats) -> ToolError {
    let count = stats.error_count.unwrap_or(0);
    let first = stats.first_error.as_deref().unwrap_or("unknown error");
    // Every file was readable; the regex failed in each (a backtracking or
    // JIT-stack limit): the pattern, not the scope, needs the change.
    if stats.pattern_error_count == Some(count) {
        return ToolError::new(
            ErrorCode::InvalidPattern,
            format!(
                "The regex hit the engine's matching limit in every file ({count}; first: {first}). Simplify nested repetition, or use regex:\"rust\" (no backtracking)."
            ),
        );
    }
    ToolError {
        ..ToolError::new(
            ErrorCode::FileAccessFailed,
            format!(
                "No file under the search path could be read ({count} failure(s); first: {first})."
            ),
        )
    }
}

/// Per-hit character budget. An omitted matchContentLength scales with the
/// effective context window so requested context is not silently clipped;
/// continuations and snapshot identity must use this same value.
pub(super) fn effective_match_content_length(q: &LocalSearchQuery) -> u32 {
    local_search_query::match_content_length(q).unwrap_or_else(|| {
        let view = q.result_view;
        let context = if uses_context(view) {
            local_search_query::context(q)
                .before
                .saturating_add(local_search_query::context(q).after)
        } else {
            0
        };
        DEFAULT_MATCH_CONTENT_LENGTH
            .saturating_mul(context.saturating_add(1))
            .min(MAX_DEFAULT_MATCH_CONTENT_LENGTH)
    })
}

/// Default files per page: snippet views stay lean; path-only views are cheap.
pub(super) fn default_page_size(view: LocalSearchQueryResultView) -> u32 {
    match view {
        LocalSearchQueryResultView::Files
        | LocalSearchQueryResultView::FilesWithout
        | LocalSearchQueryResultView::CountLines
        | LocalSearchQueryResultView::CountMatches => DEFAULT_LIST_PAGE_SIZE,
        _ => DEFAULT_SNIPPET_PAGE_SIZE,
    }
}

/// Views whose match rows carry a context window (so `contextLines` matters).
pub(super) fn uses_context(view: LocalSearchQueryResultView) -> bool {
    view == LocalSearchQueryResultView::Paginated
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
