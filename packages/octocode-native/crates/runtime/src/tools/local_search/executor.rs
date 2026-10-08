use super::types::*;
use super::{cursor::*, layout::*, leads::*, regex_repair::*, rows::*, verify::*};
use crate::policy::discovery::{CREDENTIAL_FILE_EXTENSIONS, CREDENTIAL_FILE_NAMES};
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::policy::prune::PruneMode;
use crate::security::ContentSecurity;
use crate::tools::cancel::CancellationCheck;
use crate::tools::id::query_limits::local_search::{MATCH_PAGE_MAXIMUM, PAGE_MAXIMUM};
use crate::tools::result::ToolError;
use octocode_engine::{portable::search_text_cancellable, types::TextSearchOptions};
use serde_json::json;
use std::sync::Arc;

/// Soft budget (in value chars) for a single localSearch response body.
/// Distributed across the matches shown on a page so a pathological giant line
/// or a raised `matchContentLength`/`matchPageSize` cannot emit a multi-MB
/// body. It bounds displayed value size only — every match row and its line
/// anchor are preserved, so no continuation cursor is required.
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

/// Row continuations that copy the query: `next.nextPage` and the clasify
/// handoff's search resource.
pub(super) const ROW_QUERY_COPIES: usize = 2;

/// Files per page for snippet views (the first page of a default-layout
/// result larger than the budget); path-only list views stay at 100.
pub(super) const DEFAULT_SNIPPET_PAGE_SIZE: u32 = 20;

pub(super) const DEFAULT_LIST_PAGE_SIZE: u32 = 100;

pub fn execute_local_search(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
    walk_threads: Option<u32>,
    response_window: Option<usize>,
) -> Result<LocalSearchResult, ToolError> {
    cancel.check().map_err(ToolError::cancelled)?;
    let Scanned {
        validated,
        mut parsed,
        digests,
        page_budget,
        identity: result_identity,
        query_key,
        stored_redaction,
        reusable,
        probe_options,
        policy_key,
        skipped,
    } = scan_query(query, paths, cancel, walk_threads, response_window)?;
    // The bytes a stored scan's secret check must read, or `None` when the
    // scan is fresh; a stored file whose digest is missing reads as changed.
    let expected_digest = |source: &std::path::Path| -> Option<Option<super::manifest::Digest>> {
        digests.as_ref().map(|digests| digests.get(source).copied())
    };
    let root = &validated.canonical;
    let output_root = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root.as_path()
    };
    let Shaped {
        layout,
        redacted,
        redaction,
        unverified,
        outlines,
    } = shape_page(
        query,
        paths,
        &mut parsed,
        output_root,
        page_budget,
        &expected_digest,
        stored_redaction.as_ref(),
        security,
        cancel,
    )?;
    let total_files = parsed.files.len() as u32;
    let empty = total_files == 0;
    let reusable = reusable.map(|value| super::manifest::Fresh {
        value,
        query_key,
        redaction,
        skipped: skipped.clone(),
    });
    let (mut next, leftover_matches) = cursor(
        query,
        &parsed,
        &layout,
        &result_identity,
        reusable,
        policy_key,
    );
    let octocode_engine::types::TextSearchResult {
        files: scanned,
        stats: scanned_stats,
    } = parsed;
    let stats = search_stats(&scanned_stats, total_files);
    let (mut files, shown_redacted) = project_files(query, &layout, scanned, &redacted);
    let symbol = searched_symbol(query, &layout);
    let __t2 = std::time::Instant::now();
    let (definition, enclosing_note) = annotate_enclosing(
        &mut files,
        symbol,
        symbol.filter(|_| super::enclosing::declaration_search(&query.match_string)),
        output_root,
        page_budget,
        &|source| expected_digest(source),
        outlines,
        security,
    );
    let coverage = Coverage::of(query, &stats, &scanned_stats, root, output_root, &skipped);
    let mut warnings = coverage.warnings(&files, &layout, shown_redacted, unverified, empty);
    warnings.extend(enclosing_note);
    let capped = stats.capped.unwrap_or(false);
    let (status, terminal_limit) = settle(
        query,
        &layout,
        empty,
        capped,
        leftover_matches,
        coverage.gap(),
        next.is_none(),
    );
    let mut hints = coverage.empty_hints(query, empty);
    // Leads go in after the status is settled: they are optional follow-ups
    // and never stand in for a page.
    Found {
        query,
        paths,
        root,
        output_root,
        files: &files,
        layout: &layout,
        definition: definition.as_ref(),
        symbol,
        scanned: &scanned_stats,
        skipped: &skipped,
        complete: !capped && !coverage.gap(),
        empty,
    }
    .add_leads(&mut next, &mut hints, &mut warnings, probe_options, cancel);
    if std::env::var_os("OCTOCODE_LS_TIMING").is_some() { eprintln!("TIMING enclosing+leads {:?}", __t2.elapsed()); }
    if coverage.scope_miss
        && let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut()
    {
        map.insert("viewTree".into(), scope_listing(query));
    }
    Ok(LocalSearchResult {
        status,
        stats,
        files,
        pagination: file_paging(query, &layout, total_files),
        hints,
        next,
        is_partial: coverage.gap() || capped,
        terminal_limit,
        warnings,
        source_snapshot: Some(result_identity),
        source_root: output_root.to_path_buf(),
    })
}

/// The symbol a page names declarations for: a small page names each hit's
/// enclosing declaration, and a searched symbol declared on a shown hit
/// line gets an lspSearch lead.
pub(super) fn searched_symbol<'q>(query: &'q LocalSearchQuery, layout: &Layout) -> Option<&'q str> {
    (!layout.list
        && query.case_mode != LocalSearchQueryCaseMode::Insensitive
        && query.regex_mode() != LocalSearchQueryRegex::Pcre2
        && query.invert_match != Some(true))
    .then(|| super::enclosing::searched_symbol(&query.match_string))
    .flatten()
}

/// File paging, reported only when it routes somewhere: more file pages, or
/// a requested page past the end. Totals live once, in `stats` (occurrences
/// and matched lines); the next page number and the snapshot live once, in
/// `next.*.query`.
pub(super) fn file_paging(
    query: &LocalSearchQuery,
    layout: &Layout,
    total_files: u32,
) -> Option<FilePagination> {
    let page = query.page().max(1);
    let match_page = query.match_page().max(1);
    (total_files > 0 && (layout.total_pages > 1 || layout.out_of_range)).then_some(FilePagination {
        current_page: page,
        match_page: (match_page > 1).then_some(match_page),
        total_pages: layout.total_pages,
        files_per_page: layout.files_per_page,
        total_files,
        has_more: page < layout.total_pages,
        out_of_range: layout.out_of_range,
    })
}

/// A cut page whose shown values are verified.
pub(super) struct Shaped {
    pub(super) layout: Layout,
    pub(super) redacted: Redacted,
    /// What redacting the scan changed, for a stored scan's later pages.
    pub(super) redaction: Redaction,
    /// Some values were redacted because their source could not be re-read.
    pub(super) unverified: bool,
    /// Outlines parsed from the verified reads (see [`verify_shown`]).
    pub(super) outlines: Outlines,
}

/// Redact the scan, order it, cut the page, and verify what it shows.
///
/// Every file the page reads or shows passes the read policy first: the
/// key-block reads inside [`redact_scan`] and each shown file here. A file
/// the page neither reads nor shows is checked on the page that shows it,
/// so a path swapped for a symlink after the walk never has its bytes read
/// or its rows shown.
#[allow(clippy::too_many_arguments)]
pub(super) fn shape_page(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    parsed: &mut octocode_engine::types::TextSearchResult,
    output_root: &std::path::Path,
    page_budget: usize,
    expected_digest: &ExpectedDigest<'_>,
    stored_redaction: Option<&Redaction>,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
) -> Result<Shaped, ToolError> {
    let __t = std::time::Instant::now();
    let (mut redacted, redaction) = redact_scan(
        query,
        parsed,
        output_root,
        expected_digest,
        paths,
        security,
        cancel,
        stored_redaction,
    )?;
    if std::env::var_os("OCTOCODE_LS_TIMING").is_some() { eprintln!("TIMING {} {:?}", "redact", __t.elapsed()); } let __t = std::time::Instant::now();
    order_files(query, &mut parsed.files);
    let layout = Layout::cut(query, paths, output_root, parsed, page_budget);
    for (index, _) in &layout.shown {
        validate_matched(paths, &output_root.join(&parsed.files[*index].path))?;
    }
    let mut outlines = Outlines::new();
    let unverified = verify_shown(
        query,
        parsed,
        &layout,
        output_root,
        expected_digest,
        &mut redacted,
        &mut outlines,
        security,
        cancel,
    )?;
    if std::env::var_os("OCTOCODE_LS_TIMING").is_some() { eprintln!("TIMING {} {:?}", "verify", __t.elapsed()); } let __t = std::time::Instant::now();
    Ok(Shaped {
        layout,
        redacted,
        redaction,
        unverified,
        outlines,
    })
}

/// The row's status, and whether it ends at a limit nothing continues.
pub(super) fn settle(
    query: &LocalSearchQuery,
    layout: &Layout,
    empty: bool,
    capped: bool,
    leftover_matches: bool,
    coverage_gap: bool,
    next_missing: bool,
) -> (SearchStatus, bool) {
    let page = query.page().max(1);
    let has_more = page < layout.total_pages;
    let (status, terminal) = classify_search(
        empty,
        capped,
        has_more,
        leftover_matches,
        coverage_gap,
        next_missing,
    );
    let at_ceiling = (has_more && page as usize >= PAGE_MAXIMUM)
        || (leftover_matches && query.match_page().max(1) as usize >= MATCH_PAGE_MAXIMUM);
    (status, terminal || at_ceiling)
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
    pub(super) stored_redaction: Option<Redaction>,
    /// A fresh scan small enough to store for this snapshot's later pages.
    pub(super) reusable: Option<octocode_engine::types::TextSearchResult>,
    /// The walk an empty result re-runs over what the defaults leave out.
    pub(super) probe_options: Option<TextSearchOptions>,
    pub(super) policy_key: String,
    /// What the walk left out: policy-withheld and default-excluded files.
    pub(super) skipped: crate::policy::discovery::WalkSkips,
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
    let validated = search_root(query, paths)?;
    let options = search_options(query, &validated.canonical, walk_threads);
    check_pattern(query)?;
    // An empty result re-walks with ignored, hidden and default-excluded
    // entries included (files only, time-bounded) to say whether they hold
    // the text.
    let probe_options = (query.no_ignore != Some(true)
        || query.hidden != Some(true)
        || query.default_excludes.defaults())
    .then(|| options.clone());
    let policy_key = paths.identity();
    let __t = std::time::Instant::now();
    let ScanOutput {
        parsed,
        digests,
        skipped,
        stored,
    } = scan(query, paths, options, &policy_key, cancel)?;
    if std::env::var_os("OCTOCODE_LS_TIMING").is_some() { eprintln!("TIMING {} {:?}", "scan", __t.elapsed()); } let __t = std::time::Instant::now();
    // Pages are cut from serialized sizes so a default-layout page, with
    // the row around it, fits one response window. Sized by the canonical
    // root, not its spelling: a continuation names the same root relative
    // to the workspace and must cut the same pages.
    let mut sized = normalized_query(query, streamed_layout(query));
    sized["path"] = json!(validated.canonical.to_string_lossy());
    let page_budget = crate::tools::stream_page::page_chars(
        response_window,
        crate::tools::stream_page::reserve_chars(&sized, ROW_QUERY_COPIES),
    );
    // A stored scan was stored under the snapshot derived from its query
    // key and its unchanged collected result: a query with the same key
    // derives that snapshot again, so only another key needs the digest.
    let query_key = query_key(query, &validated.canonical, page_budget);
    let (identity, stored_redaction) = match (stored, query.snapshot()) {
        (Some(stored), Some(snapshot)) if stored.query_key == query_key => {
            (snapshot.to_owned(), stored.redaction)
        }
        _ => (fingerprint(&query_key, &parsed.files, &parsed.stats), None),
    };
    // A cached scan is checked too: its identity comes from the submitted
    // query's key, so a cursor reused with different search semantics never
    // serves the old query's matches.
    if query
        .snapshot()
        .is_some_and(|expected| expected != identity)
    {
        return Err(stale_snapshot(query));
    }
    let reusable =
        (digests.is_none() && super::manifest::fits(&parsed).is_some()).then(|| parsed.clone());
    Ok(Scanned {
        validated,
        parsed,
        digests,
        page_budget,
        identity,
        query_key,
        stored_redaction,
        reusable,
        probe_options,
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
    paths.validate(query.path.as_str()).map_err(|error| {
        ToolError::root_policy(error, query.path.as_str(), paths, "fileAccessFailed")
    })
}

/// The engine walk the query asks for.
pub(super) fn search_options(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    walk_threads: Option<u32>,
) -> TextSearchOptions {
    let view = query.result_view;
    let requested_sort = query.sort;
    let path_sort = matches!(
        requested_sort,
        LocalSearchQuerySort::Modified
            | LocalSearchQuerySort::Accessed
            | LocalSearchQuerySort::Created
            | LocalSearchQuerySort::Path
    );
    // Match-count order must choose the collection cap's survivors by match
    // count across every searched file; path-list views rank by path.
    let count_sort = requested_sort == LocalSearchQuerySort::MatchCount
        && !matches!(
            view,
            LocalSearchQueryResultView::Files | LocalSearchQueryResultView::FilesWithout
        );
    let (case, regex, multiline) = (query.case_mode, query.regex_mode(), query.multiline);
    TextSearchOptions {
        path: root.to_string_lossy().into_owned(),
        pattern: query.match_string.to_string(),
        fixed_string: Some(regex == LocalSearchQueryRegex::Literal),
        perl_regex: Some(regex == LocalSearchQueryRegex::Pcre2),
        case_sensitive: Some(case == LocalSearchQueryCaseMode::Sensitive),
        case_insensitive: Some(case == LocalSearchQueryCaseMode::Insensitive),
        whole_word: query.whole_word,
        invert_match: query.invert_match,
        multiline: Some(multiline != LocalSearchQueryMultiline::Off),
        multiline_dotall: Some(multiline == LocalSearchQueryMultiline::Dotall),
        files_only: Some(matches!(view, LocalSearchQueryResultView::Files)),
        files_without_match: Some(view == LocalSearchQueryResultView::FilesWithout),
        count_lines_per_file: Some(view == LocalSearchQueryResultView::CountLines),
        count_matches_per_file: Some(view == LocalSearchQueryResultView::CountMatches),
        context_lines: Some(
            query
                .context_lines()
                .unwrap_or_else(|| default_context_lines(view)),
        ),
        lang_type: query.language.clone(),
        include: Some(crate::policy::include::include_globs(&query.include))
            .filter(|include| !include.is_empty()),
        exclude: Some(
            query
                .exclude
                .clone()
                .into_iter()
                // Credential-shaped names are never searched; generated
                // files are skipped (and counted) by the walk's path filter
                // unless `defaultExcludes:false`.
                .chain(CREDENTIAL_FILE_NAMES.iter().map(|s| (*s).to_owned()))
                .chain(CREDENTIAL_FILE_EXTENSIONS.iter().map(|s| format!("*{s}")))
                .collect(),
        ),
        exclude_dir: Some(PruneMode::SearchSafe.directories(query.default_excludes.defaults())),
        no_ignore: query.no_ignore,
        // `noIgnore` is the one "search everything" switch: it
        // walks hidden files too unless `hidden` says otherwise.
        hidden: query.hidden.or(query.no_ignore.filter(|all| *all)),
        max_depth: query.max_depth(),
        // The engine owns the relevance order (for a bare-identifier search,
        // source files declaring the name first; then count, then source
        // before test/generated paths, then declaration > code >
        // comment/string hits, then path) so its top-k and cap keep the same
        // survivors.
        sort: Some(match requested_sort {
            LocalSearchQuerySort::Traversal => "traversal".into(),
            LocalSearchQuerySort::Relevance => "relevance".into(),
            _ if path_sort => format!("{requested_sort:?}").to_lowercase(),
            _ if count_sort => "matchCount".into(),
            _ => "path".into(),
        }),
        sort_reverse: query.reverse,
        max_snippet_chars: Some(effective_match_content_length(query)),
        classify_matches: Some(false),
        only_matching: Some(view == LocalSearchQueryResultView::MatchOnly),
        unique: Some(matches!(
            query.unique,
            LocalSearchQueryUnique::List | LocalSearchQueryUnique::Count
        )),
        count_unique: Some(query.unique == LocalSearchQueryUnique::Count),
        // The engine keeps the first 10k matched files in the engine sort order
        // above (by match count for relevance/matchCount), chosen across every
        // searched file; stats totals still count all matched files and the
        // cap surfaces as capReason "maxCollectedFiles".
        max_collected_files: Some(10_000),
        // Use the engine default per-file byte ceiling (skips pathological
        // multi-GB files, surfaced as a maxFileSize diagnostic).
        max_file_bytes: None,
        // The batch's share of the cores (see `BatchBudget`); `None` walks
        // on every core.
        walk_threads,
        // A scan small enough to store for later pages keeps the digest of
        // each matched file from the read that searched it (see `manifest`).
        digest_max_bytes: Some(super::manifest::MAX_SOURCE_BYTES),
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
    pub(super) skipped: crate::policy::discovery::WalkSkips,
    /// What was stored with a reused scan: its query key and redaction.
    pub(super) stored: Option<StoredDerived>,
}

/// What a stored scan's fresh page derived from it.
pub(super) struct StoredDerived {
    pub(super) query_key: String,
    pub(super) redaction: Option<Redaction>,
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
    let (parsed, digests, withheld, derived) = if let Some(snapshot) = query.snapshot()
        && let Some(stored) = super::manifest::get(snapshot, policy_key)
    {
        let derived = StoredDerived {
            query_key: stored.query_key,
            redaction: stored.redaction,
        };
        (
            stored.value,
            Some(stored.digests),
            stored.skipped,
            Some(derived),
        )
    } else {
        let walk = Arc::new(crate::policy::discovery::SearchWalk::new(
            paths.clone(),
            std::path::PathBuf::from(&options.path),
            query.default_excludes.defaults(),
        ));
        let parsed = search_text_cancellable(options, walk.clone(), &|| cancel.check().is_err())
            .map_err(|error| {
                let message = error.to_string();
                // Glob and file-type failures carry no typed kind from the engine yet.
                let bad_filter =
                    message.contains("glob") || message.contains("unrecognized file type");
                ToolError::new(
                    if bad_filter {
                        "invalidInput"
                    } else {
                        "executionFailed"
                    },
                    message,
                )
            })?;
        (parsed, None, walk.skipped(), None)
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
        // comment, string) among equal counts.
        LocalSearchQuerySort::Relevance if is_count_view(query) => {
            files.sort_by_key(|file| std::cmp::Reverse(file.match_count));
        }
        _ => {}
    }
    if query.reverse.unwrap_or(false)
        && !matches!(
            sort,
            LocalSearchQuerySort::Modified
                | LocalSearchQuerySort::Accessed
                | LocalSearchQuerySort::Created
                | LocalSearchQuerySort::Relevance
        )
    {
        files.reverse();
    }
}

/// The response's totals from the scan.
pub(super) fn search_stats(
    scanned: &octocode_engine::types::TextSearchStats,
    total_files: u32,
) -> SearchStats {
    SearchStats {
        total_occurrences: scanned.match_count.unwrap_or(0),
        matched_lines: scanned.matched_lines.unwrap_or(0),
        files_matched: scanned.files_matched.unwrap_or(total_files),
        files_searched: scanned.files_searched.unwrap_or(0),
        bytes_searched: scanned.bytes_searched,
        search_time: None,
        // A binary cut (capReason `binaryQuit`) stays `capped`: coverage ended
        // early, and the result is marked partial/terminal.
        capped: scanned.capped,
        cap_reason: scanned.cap_reason.clone(),
        error_count: scanned.error_count.filter(|n| *n > 0),
        first_error: scanned.first_error.clone(),
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
    let match_page = query.match_page().max(1);
    let mut shown_redacted = 0usize;
    let mut slots = scanned.into_iter().map(Some).collect::<Vec<_>>();
    let files = layout
        .shown
        .iter()
        .filter_map(|(index, rows)| Some((slots.get_mut(*index)?.take()?, rows.clone())))
        .map(|(f, rows)| {
            let total = f.matches.len() as u32;
            let end = rows.end.min(f.matches.len());
            let start = rows.start.min(end);
            let mut page_rows = f.matches[start..end].iter().collect::<Vec<_>>();
            page_rows.sort_by_key(|m| (m.line, m.column));
            let shown = page_rows
                .into_iter()
                .inspect(|m| {
                    if redacted.contains(&(f.path.clone(), m.line, m.column)) {
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
                Some(context) => merge_context_windows(
                    shown,
                    context,
                    effective_match_content_length(query) as usize,
                ),
                None => shown,
            };
            SearchFile {
                matches: (!layout.list).then_some(shown),
                total_occurrences: (query.result_view == LocalSearchQueryResultView::CountMatches)
                    .then_some(f.match_count),
                total_matched_lines: (query.result_view == LocalSearchQueryResultView::CountLines)
                    .then_some(f.match_count),
                pagination: file_pagination(layout, total, start, end, &later, match_page),
                path: f.path,
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
    ToolError {
        hints: vec![unreadable_hint(count)],
        ..ToolError::new(
            "fileAccessFailed",
            format!(
                "No file under the search path could be read ({count} failure(s); first: {first})."
            ),
        )
    }
}

/// Default ±context window per match row: the hit line alone, except the
/// `detailed` view, which exists to show surrounding code.
/// Per-hit character budget. An omitted matchContentLength scales with the
/// effective context window so requested context is not silently clipped;
/// continuations and snapshot identity must use this same value.
pub(super) fn effective_match_content_length(q: &LocalSearchQuery) -> u32 {
    q.match_content_length().unwrap_or_else(|| {
        let view = q.result_view;
        let context = if uses_context(view) {
            q.context_lines()
                .unwrap_or_else(|| default_context_lines(view))
        } else {
            0
        };
        DEFAULT_MATCH_CONTENT_LENGTH
            .saturating_mul(context.saturating_mul(2).saturating_add(1))
            .min(MAX_DEFAULT_MATCH_CONTENT_LENGTH)
    })
}

pub(super) fn default_context_lines(view: LocalSearchQueryResultView) -> u32 {
    if view == LocalSearchQueryResultView::Detailed {
        3
    } else {
        0
    }
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
    matches!(
        view,
        LocalSearchQueryResultView::Paginated
            | LocalSearchQueryResultView::Content
            | LocalSearchQueryResultView::Detailed
    )
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
    let terminal = (partial || has_more) && next_missing;
    (status, terminal)
}
