use super::types::*;
use crate::canonical_json::canonicalize;
use crate::policy::discovery::{DISCOVERY_IGNORED_FILE_EXTENSIONS, DISCOVERY_IGNORED_FILE_NAMES};
use crate::policy::path::PathPolicy;
use crate::policy::prune::DefaultsFlag;
use crate::policy::prune::PruneMode;
use crate::security::ContentSecurity;
use crate::tools::cancel::CancellationCheck;
use crate::tools::id::ToolId;
use octocode_engine::{
    portable::{RipgrepPathFilter, search_ripgrep_cancellable},
    types::RipgrepSearchOptions,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Upper bound (bytes) on a file re-read for the private-key block scan.
/// Matches the secret scanner's own content cap; larger files fall back to the
/// per-match window sanitizer rather than pay an unbounded read.
const MAX_KEY_SCAN_BYTES: u64 = 10 * 1024 * 1024;

/// Soft budget (in value chars) for a single localSearch response body.
/// Distributed across the matches shown on a page so a pathological giant line
/// or a raised `matchContentLength`/`maxMatchesPerFile` cannot emit a multi-MB
/// body. It bounds displayed value size only — every match row and its line
/// anchor are preserved, so no continuation cursor is required.
const RESPONSE_VALUE_CHAR_BUDGET: usize = 1_000_000;

/// Floor on the per-match display cap the budget may impose, so a page with many
/// matches still shows a useful slice of each rather than a few characters.
const MIN_MATCH_VALUE_CHARS: usize = 40;

/// Lean agent-facing defaults (caller values always win). A/B over realistic
/// locate-the-code queries kept every target file+line on page 1 while cutting
/// default response bytes ~54% versus the old 100 files / 20 rows / 500 chars /
/// ±2 lines: one clipped-around-the-hit line per row is enough to pick the
/// file+line, and `detailed`/`contextLines` or localFetch add surrounding code.
const DEFAULT_MATCH_CONTENT_LENGTH: u32 = 200;
/// Cap on the context-scaled default; an explicit matchContentLength may exceed it.
const MAX_DEFAULT_MATCH_CONTENT_LENGTH: u32 = 4000;
/// Rows per file when a result is too large to show whole.
const DEFAULT_MAX_MATCHES_PER_FILE: u32 = 10;
/// Row continuations that copy the query: `next.nextPage` and the clasify
/// handoff's search resource.
const ROW_QUERY_COPIES: usize = 2;
/// Files per page for snippet views (the first page of a default-layout
/// result larger than the budget); path-only list views stay at 100.
const DEFAULT_SNIPPET_PAGE_SIZE: u32 = 20;
const DEFAULT_LIST_PAGE_SIZE: u32 = 100;

pub fn execute_local_search(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
    walk_threads: Option<u32>,
    response_window: Option<usize>,
) -> Result<LocalSearchResult, LocalSearchError> {
    cancel.check().map_err(cancelled)?;
    if query.search_text.is_empty() {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "`searchText` is required".into(),
            hints: vec![],
            next: None,
        });
    }
    if query.path.is_empty() {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "Path is required for search".into(),
            hints: vec![],
            next: None,
        });
    }
    let view = query.result_view;
    if query.match_window.is_some() && view != LocalSearchQueryResultView::MatchOnly {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "`matchWindow` requires resultView:\"matchOnly\"".into(),
            hints: vec![],
            next: None,
        });
    }
    if query.unique != LocalSearchQueryUnique::Off && view != LocalSearchQueryResultView::MatchOnly
    {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "`unique` requires resultView:\"matchOnly\"".into(),
            hints: vec![],
            next: None,
        });
    }
    let validated = paths
        .validate(query.path.as_str())
        .map_err(|error| LocalSearchError {
            // A missing search root is not-found (exit 3), not an I/O failure.
            code: if error.code == crate::policy::PolicyErrorCode::NotFound {
                "pathNotFound"
            } else {
                error.local_error_code("fileAccessFailed")
            },
            message: error.message,
            hints: vec![],
            next: None,
        })?;
    let case = query.case_mode;
    let regex = query.regex;
    let multiline = query.multiline;
    let requested_sort = query.sort;
    let context_lines = query
        .context_lines()
        .unwrap_or_else(|| default_context_lines(view));
    let path_sort = matches!(
        query.sort,
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
    let options = RipgrepSearchOptions {
        path: validated.canonical.to_string_lossy().into_owned(),
        pattern: query.search_text.to_string(),
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
        context_lines: Some(context_lines),
        lang_type: query.lang_type.clone(),
        include: Some(query.include.clone()).filter(|include| !include.is_empty()),
        exclude: Some(
            query
                .exclude
                .clone()
                .into_iter()
                .chain(DISCOVERY_IGNORED_FILE_NAMES.iter().map(|s| (*s).to_owned()))
                .chain(
                    DISCOVERY_IGNORED_FILE_EXTENSIONS
                        .iter()
                        .map(|s| format!("*{s}")),
                )
                .collect(),
        ),
        exclude_dir: Some(
            PruneMode::SearchSafe
                .directories(&query.exclude_dir, query.default_excludes.defaults()),
        ),
        no_ignore: query.no_ignore,
        hidden: query.hidden,
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
        match_window: query.match_window(),
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
    };
    // Classify an invalid pattern from the engine's typed validation result
    // (not from search error text) before walking the tree.
    if regex != LocalSearchQueryRegex::Literal {
        let checked = octocode_engine::portable::validate_ripgrep_pattern(
            &query.search_text,
            false,
            regex == LocalSearchQueryRegex::Pcre2,
        );
        if !checked.valid {
            return Err(invalid_regex(
                query,
                regex_error_message(&query.search_text, checked.error.as_deref()),
            ));
        }
    }
    // A continuation reuses its page-1 scan instead of walking the tree
    // again. A stored scan is reused only under the path policy that
    // produced it; the snapshot comparison below then proves it answers
    // this query.
    let policy_key = policy_identity(paths);
    // Digests of a stored scan's matched files: every re-read below that
    // decides what the stored values may show must read these same bytes.
    let (mut parsed, stored_digests) = if let Some(snapshot) = query.snapshot()
        && let Some(stored) = super::manifest::get(snapshot, &policy_key)
    {
        (stored.value, Some(stored.digests))
    } else {
        (
            search_ripgrep_cancellable(options, Arc::new(PolicyFilter(paths.clone())), &|| {
                cancel.check().is_err()
            })
            .map_err(|error| {
                let message = error.to_string();
                // Glob and file-type failures carry no typed kind from the engine yet.
                let bad_filter =
                    message.contains("glob") || message.contains("unrecognized file type");
                LocalSearchError {
                    code: if bad_filter {
                        "invalidQuery"
                    } else {
                        "toolExecutionFailed"
                    },
                    message,
                    hints: vec![],
                    next: None,
                }
            })?,
            None,
        )
    };
    let from_manifest = stored_digests.is_some();
    // The bytes a stored scan's secret check must read, or `None` when the
    // scan is fresh; a stored file whose digest is missing reads as changed.
    let expected_digest = |source: &std::path::Path| -> Option<Option<super::manifest::Digest>> {
        stored_digests
            .as_ref()
            .map(|digests| digests.get(source).copied())
    };
    cancel.check().map_err(cancelled)?;
    if parsed.files.is_empty()
        && parsed.stats.files_searched.unwrap_or(0) == 0
        && parsed.stats.error_count.unwrap_or(0) > 0
    {
        return Err(unreadable_scope(&parsed.stats));
    }
    for file in &parsed.files {
        paths
            .validate_read(&file.path)
            .map_err(|error| LocalSearchError {
                code: error.local_error_code("fileAccessFailed"),
                message: "Search encountered a path denied by the active path policy".into(),
                hints: vec![],
                next: None,
            })?;
    }
    // Pages are cut from serialized sizes so a default-layout page, with
    // the row around it, fits one response window.
    let streamed = streamed_layout(query);
    let page_budget = crate::tools::stream_page::page_chars(
        response_window,
        crate::tools::stream_page::reserve_chars(
            &normalized_query(query, streamed),
            ROW_QUERY_COPIES,
        ),
    );
    let result_identity = fingerprint(
        query,
        &validated.canonical,
        &parsed.files,
        &parsed.stats,
        page_budget,
    );
    // Kept only when the response will offer a continuation (below).
    let reusable =
        (!from_manifest && super::manifest::fits(&parsed).is_some()).then(|| parsed.clone());
    // A cached scan is checked too: its identity is recomputed from the
    // submitted query, so a cursor reused with different search semantics
    // never serves the old query's matches.
    if query
        .snapshot()
        .is_some_and(|expected| expected != result_identity)
    {
        return Err(stale_snapshot(query));
    }
    let root = &validated.canonical;
    let output_root = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root.as_path()
    };
    // (path, line, column) of every match whose value was changed by secret
    // redaction: the returned text is then not verbatim source, and the row
    // says so via a `redactedMatches` warning.
    let mut redacted = std::collections::HashSet::<(String, u32, u32)>::new();
    for file in &mut parsed.files {
        if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
            file.path = relative.to_string_lossy().into_owned();
        }
        let source_path = output_root.join(&file.path);
        // A match on an interior base64 body line of a private key would
        // leak the key even though the match view holds no BEGIN/END marker (the
        // anchored built-in patterns need a complete block). Only when a snippet
        // actually looks like key material do we scan the full file for private-
        // key block ranges, then redact matches whose window intersects a block.
        // Innocent base64 triggers a scan that finds no block and redacts nothing.
        let key_ranges = if file
            .matches
            .iter()
            .any(|m| crate::security::snippet_may_hold_key_material(&m.value))
        {
            let bytes = std::fs::metadata(&source_path)
                .ok()
                .filter(|meta| meta.len() <= MAX_KEY_SCAN_BYTES)
                .and_then(|_| std::fs::read(&source_path).ok());
            if let (Some(bytes), Some(expected)) = (&bytes, expected_digest(&source_path))
                && expected != Some(<[u8; 32]>::from(Sha256::digest(bytes)))
            {
                return Err(changed_since_scan(query));
            }
            bytes
                .map(|bytes| {
                    crate::security::private_key_block_line_ranges(&String::from_utf8_lossy(&bytes))
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for matched in &mut file.matches {
            cancel.check().map_err(cancelled)?;
            let changed = if !key_ranges.is_empty()
                && crate::security::match_window_intersects_key_block(
                    matched.line,
                    &matched.value,
                    &key_ranges,
                ) {
                matched.value = crate::security::key_fragment_placeholder();
                true
            } else {
                let sanitized = security.sanitize_text(&matched.value, Some(&source_path));
                let changed = sanitized.content != matched.value;
                matched.value = sanitized.content;
                changed
            };
            if changed {
                redacted.insert((file.path.clone(), matched.line, matched.column));
            }
        }
    }
    match requested_sort {
        LocalSearchQuerySort::Path => parsed.files.sort_by(|a, b| a.path.cmp(&b.path)),
        LocalSearchQuerySort::MatchCount => parsed.files.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.path.cmp(&b.path))
        }),
        _ => {}
    }
    // Engine-side time and relevance sorts already honour `reverse`; every
    // order the runtime (re)establishes — path, matchCount, traversal — is
    // reversed here, as the schema promises ("after sort, before pagination").
    if query.reverse.unwrap_or(false)
        && !matches!(
            requested_sort,
            LocalSearchQuerySort::Modified
                | LocalSearchQuerySort::Accessed
                | LocalSearchQuerySort::Created
                | LocalSearchQuerySort::Relevance
        )
    {
        parsed.files.reverse();
    }
    // A result within the page budget shows every hit on one page: paging
    // metadata and continuations would cost more than the rows they hide. A
    // caller cap always wins; larger results keep the 10-rows-per-file
    // default on their first page.
    let list = matches!(
        view,
        LocalSearchQueryResultView::Files
            | LocalSearchQueryResultView::FilesWithout
            | LocalSearchQueryResultView::CountLines
            | LocalSearchQueryResultView::CountMatches
    );
    // Rows render with workspace-relative paths, so a file entry costs its
    // root prefix too.
    let prefix_chars = match paths.workspace_relative(output_root).as_deref() {
        Some(".") => 0,
        Some(relative) => crate::tools::stream_page::json_text_chars(relative) + 1,
        None => crate::tools::stream_page::json_text_chars(&output_root.to_string_lossy()) + 1,
    };
    // matchOnly rows show their exact span clipped to the display bound.
    let match_only_limit = (view == LocalSearchQueryResultView::MatchOnly)
        .then_some(effective_match_content_length(query) as usize);
    // A clipped row rides its file's `next.expandValues*` read (see
    // [`expand_values`]): the page pays for that read, so it still fits.
    let clipped = |matched: &octocode_engine::types::RipgrepMatch| match match_only_limit {
        Some(limit) => matched.value.chars().nth(limit).is_some(),
        None => matched.original_chars.is_some(),
    };
    let expand_context = if view == LocalSearchQueryResultView::MatchOnly {
        0
    } else {
        context_lines
    };
    let read_chars = expansion_read_chars(query, multiline != LocalSearchQueryMultiline::Off);
    let file_chars = |file: &octocode_engine::types::RipgrepFile| {
        let entry = crate::tools::stream_page::json_text_chars(&file.path)
            + prefix_chars
            + FILE_ENTRY_CHARS;
        if file.matches.iter().any(clipped) {
            entry
                + read_chars
                + crate::tools::stream_page::json_text_chars(&file.path)
                + prefix_chars
        } else {
            entry
        }
    };
    let row_chars = |matched: &octocode_engine::types::RipgrepMatch| {
        let row = row_chars(
            matched,
            match_only_limit,
            view == LocalSearchQueryResultView::MatchOnly,
        );
        if clipped(matched) {
            // Its line range, and a share of the extra read every
            // FETCH_RANGES ranges open.
            let digits = |n: u32| n.checked_ilog10().map_or(1, |log| log as usize + 1);
            row + 2 * digits(matched.line.saturating_add(expand_context))
                + 4
                + read_chars.div_ceil(FETCH_RANGES)
        } else {
            row
        }
    };
    let hits_total: usize = if list {
        0
    } else {
        parsed.files.iter().map(|file| file.matches.len()).sum()
    };
    let show_all = !list
        && query.max_matches_per_file().is_none()
        && fits_within(&parsed.files, page_budget, |file| {
            file_chars(file) + file.matches.iter().map(row_chars).sum::<usize>()
        });
    let matches_per = query
        .max_matches_per_file()
        .unwrap_or(if show_all {
            u32::try_from(hits_total).unwrap_or(u32::MAX)
        } else {
            DEFAULT_MAX_MATCHES_PER_FILE
        })
        .max(1);
    // A file with more hits than one match page shows its deciding rows
    // first: declarations, then assignments/branches/returns, then other
    // code, then comments and strings (stable by line within a rank). A row
    // repeating an earlier row's text adds nothing and follows every distinct
    // row. Pages partition that order; each page is shown in source order.
    if view != LocalSearchQueryResultView::MatchOnly {
        for file in &mut parsed.files {
            if file.matches.len() as u32 > matches_per {
                rank_file_rows(&mut file.matches);
            }
        }
    }
    let page = query.page().max(1);
    let total_files = parsed.files.len() as u32;
    let match_page = query.match_page().max(1);
    // Default layout (no pageSize, maxMatchesPerFile, or later match page):
    // pages are cut from one row stream by the response budget, so a walk
    // never pages a hot file ten rows at a time. A caller layout keeps the
    // file-page x match-page grid.
    let (shown, total_pages, files_per_page) = if streamed {
        let mut pages = stream_pages(
            &parsed.files,
            matches_per as usize,
            show_all,
            page_budget,
            StreamCosts {
                file: &file_chars,
                row: &row_chars,
            },
        );
        let total_pages = u32::try_from(pages.len()).unwrap_or(u32::MAX).max(1);
        let shown = if (page as usize) <= pages.len() {
            pages.swap_remove(page as usize - 1)
        } else {
            Vec::new()
        };
        (shown, total_pages, None)
    } else {
        let page_size = query
            .page_size()
            .unwrap_or_else(|| {
                if show_all {
                    default_page_size(view)
                        .max(u32::try_from(parsed.files.len()).unwrap_or(u32::MAX))
                } else {
                    default_page_size(view)
                }
            })
            .max(1);
        let start = (page - 1).saturating_mul(page_size) as usize;
        let end = start
            .saturating_add(page_size as usize)
            .min(parsed.files.len());
        let skip = (match_page - 1).saturating_mul(matches_per) as usize;
        let shown = (start.min(end)..end)
            .map(|index| (index, skip..skip.saturating_add(matches_per as usize)))
            .collect::<PageRows>();
        (
            shown,
            total_files.div_ceil(page_size).max(1),
            Some(page_size),
        )
    };
    let out_of_range = total_files > 0 && page > total_pages;
    let mut unverified_redactions = false;
    // A page from a stored scan shows each of its files only while that file
    // still hashes to the stored bytes; text views prove it in the secret
    // check below, which reads the same bytes.
    if list && from_manifest {
        for (index, _) in &shown {
            let source = output_root.join(&parsed.files[*index].path);
            let expected = expected_digest(&source).flatten();
            if expected.is_none() || expected != super::manifest::digest_file(&source).ok() {
                return Err(changed_since_scan(query));
            }
        }
    }
    if !list {
        for (index, rows) in &shown {
            let file = &mut parsed.files[*index];
            cancel.check().map_err(cancelled)?;
            let before = file
                .matches
                .iter()
                .map(|matched| matched.value.clone())
                .collect::<Vec<_>>();
            let source = output_root.join(&file.path);
            match guard_clipped_secrets(
                file,
                &source,
                expected_digest(&source),
                rows.clone(),
                security,
                view == LocalSearchQueryResultView::MatchOnly,
                cancel,
            )
            .map_err(cancelled)?
            {
                Verification::Verified => {}
                Verification::Unverified => unverified_redactions = true,
                Verification::Changed => return Err(changed_since_scan(query)),
            }
            for (matched, before) in file.matches.iter().zip(before) {
                if matched.value != before {
                    redacted.insert((file.path.clone(), matched.line, matched.column));
                }
            }
        }
    }
    let total_matches = if list {
        parsed.stats.match_count.unwrap_or(0)
    } else {
        parsed.files.iter().map(|f| f.matches.len() as u32).sum()
    };
    let empty = total_files == 0;
    // A streamed page names its continuation; a grid page also needs the
    // snapshot while a shown file has rows beyond one match page.
    let snapshot = if !empty
        && (query.snapshot.is_some()
            || page < total_pages
            || (!streamed
                && parsed
                    .files
                    .iter()
                    .any(|f| f.matches.len() as u32 > matches_per)))
    {
        Some(result_identity.clone())
    } else {
        None
    };
    if snapshot.is_some()
        && let Some(scan) = reusable
    {
        super::manifest::put(result_identity.clone(), policy_key, scan);
    }
    // Leftover rows only count on the files this page shows: another page's
    // files are reached by `nextPage` (which restarts at matchPage 1). List
    // views emit no match rows, so they have none left to page, and a
    // streamed page's later rows are on its later pages.
    let leftover_matches = !list
        && !streamed
        && shown.iter().any(|(index, _)| {
            parsed.files[*index].matches.len() as u32 > match_page.saturating_mul(matches_per)
        });
    let mut next = build_next(
        query,
        page,
        total_pages,
        leftover_matches,
        match_page,
        snapshot.as_deref(),
        streamed,
    );
    // Keep full values for identity, unique grouping and counts. The engine's
    // match-only path emits exact spans, so apply the public display bound
    // (`match_only_limit`) here.
    // Distribute the response value-char budget across the matches shown
    // on this page. `display_cap` is the tighter of the matchOnly display bound
    // and the budget-derived per-match cap; a giant match is clipped (flagged
    // `truncated`) rather than dropped, so the existing page/match cursors and
    // the returned line anchor + localFetch cover full retrieval unchanged.
    let shown_total: usize = shown
        .iter()
        .map(|(index, rows)| {
            let len = parsed.files[*index].matches.len();
            rows.end.min(len).saturating_sub(rows.start.min(len))
        })
        .sum();
    let budget_cap = (shown_total > 0)
        .then(|| (RESPONSE_VALUE_CHAR_BUDGET / shown_total).max(MIN_MATCH_VALUE_CHARS));
    let display_cap = match (match_only_limit, budget_cap) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    };
    // The budget only "binds" when it is tighter than the caller's own
    // per-match limit; otherwise truncation is plain matchContentLength.
    let budget_binds = budget_cap.is_some_and(|cap| {
        cap < effective_match_content_length(query) as usize && display_cap == Some(cap)
    });
    // Content views emit a ±contextLines window per match row; windows of
    // nearby rows overlap, so merge them into one block per run of lines.
    // matchOnly carries exact spans and multiline rows span several lines, so
    // neither has per-row windows to merge.
    let merge_context = (!list
        && view != LocalSearchQueryResultView::MatchOnly
        && multiline == LocalSearchQueryMultiline::Off
        && context_lines > 0)
        .then_some(context_lines);
    let mut shown_redacted = 0usize;
    let mut slots = parsed.files.into_iter().map(Some).collect::<Vec<_>>();
    let files = shown
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
                .map(|m| project_match(m, display_cap))
                .map(|mut row| {
                    if view != LocalSearchQueryResultView::MatchOnly {
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
            let shown = match merge_context {
                Some(context) => merge_context_windows(
                    shown,
                    context,
                    effective_match_content_length(query) as usize,
                ),
                None => shown,
            };
            // Per-file paging is only reported while it routes somewhere:
            // more rows remain, or the requested match page is past the end.
            let pagination = if list {
                None
            } else if streamed {
                (end < f.matches.len()).then(|| ItemPagination {
                    current_page: None,
                    total_pages: None,
                    total_matches: total,
                    has_more: true,
                    next_match_page: None,
                    more_lines: Some(line_ranges(&later, MAX_MORE_LINE_RANGES)),
                    out_of_range: false,
                })
            } else {
                let total_pages = total.div_ceil(matches_per).max(1);
                let has_more = match_page < total_pages;
                let out_of_range = start >= total as usize && total > 0;
                (has_more || out_of_range).then(|| ItemPagination {
                    current_page: Some(match_page),
                    total_pages: Some(total_pages),
                    total_matches: total,
                    has_more,
                    next_match_page: (has_more && match_page < 1000).then_some(match_page + 1),
                    more_lines: (has_more && !later.is_empty())
                        .then(|| line_ranges(&later, MAX_MORE_LINE_RANGES)),
                    out_of_range,
                })
            };
            SearchFile {
                path: f.path,
                matches: (!list).then_some(shown),
                total_occurrences: (view == LocalSearchQueryResultView::CountMatches)
                    .then_some(f.match_count),
                total_matched_lines: (view == LocalSearchQueryResultView::CountLines)
                    .then_some(f.match_count),
                pagination,
            }
        })
        .collect::<Vec<_>>();
    // A later match page must not re-send files whose rows ended on an
    // earlier page as empty `outOfRange` rows. Keep them only when every file on
    // this page is exhausted, so a forged/stale matchPage still gets its
    // out-of-range diagnostic instead of a silent empty page.
    let exhausted = |file: &SearchFile| file.pagination.as_ref().is_some_and(|p| p.out_of_range);
    let mut files = files;
    if !list && files.iter().any(|file| !exhausted(file)) {
        files.retain(|file| !exhausted(file));
    }
    let binary_files = binary_file_list(
        parsed.stats.binary_files.as_deref().unwrap_or_default(),
        parsed.stats.binary_file_count.unwrap_or(0),
        output_root,
    );
    let stats = SearchStats {
        total_occurrences: parsed.stats.match_count.unwrap_or(0),
        matched_lines: parsed.stats.matched_lines.unwrap_or(0),
        files_matched: parsed.stats.files_matched.unwrap_or(total_files),
        files_searched: parsed.stats.files_searched.unwrap_or(0),
        bytes_searched: parsed.stats.bytes_searched,
        search_time: None,
        // A binary cut (capReason `binaryQuit`) stays `capped`: coverage ended
        // early, and the result is marked partial/terminal below.
        capped: parsed.stats.capped,
        cap_reason: parsed.stats.cap_reason,
        error_count: parsed.stats.error_count.filter(|n| *n > 0),
        first_error: parsed.stats.first_error,
    };
    let mut warnings = vec![];
    let any_truncated = files.iter().any(|file| {
        file.matches
            .as_ref()
            .is_some_and(|matches| matches.iter().any(|matched| matched.truncated))
    });
    if budget_binds && any_truncated {
        warnings.push(
            "Match values were shortened to keep the total response within its size budget. Every match row and its line anchor is preserved; next.expandValues reads the shortened values whole.".into(),
        );
    } else if any_truncated {
        warnings.push(
            "Some match values were truncated to matchContentLength; originalChars and returnedChars describe each shortened value. Counts and row pagination are unchanged. next.expandValues reads them whole.".into(),
        );
    }
    if shown_redacted > 0 && !list {
        warnings.push(format!(
            "redactedMatches: {shown_redacted} returned match value(s) had secret-shaped text replaced by [REDACTED…] placeholders; those values are not verbatim source."
        ));
    }
    if unverified_redactions {
        warnings.push(
            "Some match values were redacted because their source file could not be re-read to check clipped text for secrets. Use localFetch at the returned anchors.".into(),
        );
    }
    let cap_has = |name: &str| {
        stats
            .cap_reason
            .as_deref()
            .is_some_and(|reason| reason.split(", ").any(|r| r == name))
    };
    if cap_has("pcre2Deadline") {
        warnings.push(
            "The PCRE2 (regex:\"pcre2\") search hit its wall-clock deadline and was stopped; results cover only the files finished before it. Narrow the pattern/scope, or use regex:\"literal\" or the default engine.".into(),
        );
    }
    let binary_cut = cap_has("binaryQuit");
    if binary_cut {
        warnings.push(format!(
            "binaryFileSkipped: {binary_files} searched only up to the first NUL byte; no text tool reads past it."
        ));
    }
    let error_count = stats.error_count.unwrap_or(0);
    if error_count > 0 && empty {
        // Hints are shown only on empty/error rows; a partial row keeps the
        // unreadable-path explanation as a warning.
        warnings.push(unreadable_hint(error_count));
    }
    // Unreadable paths or a binary cut leave coverage incomplete: absence is
    // unproven, and prefix matches before a NUL are not the file's full set.
    let coverage_gap = error_count > 0 || binary_cut;
    let has_more = page < total_pages;
    let capped = stats.capped.unwrap_or(false);
    // Only a complete result hands off a read: a partial one keeps its
    // coverage limit visible.
    if next.is_none()
        && !capped
        && !coverage_gap
        && !list
        && view != LocalSearchQueryResultView::MatchOnly
        && context_lines == 0
        && regex != LocalSearchQueryRegex::Pcre2
        && query.invert_match != Some(true)
        && total_files as usize <= READ_HANDOFF_MAX_FILES
        && let Some(read) = files.first().and_then(|top| read_handoff(query, root, top))
    {
        next = Some(json!({ "read": read }));
    }
    let (status, mut terminal_limit) = classify_search(
        empty,
        capped,
        has_more,
        leftover_matches,
        coverage_gap,
        next.is_none(),
    );
    terminal_limit |= (has_more && page >= 1000) || (leftover_matches && match_page >= 1000);
    // Every clipped value on the page stays reachable whole.
    let expansions = expand_values(
        query,
        paths,
        output_root,
        &files,
        if view == LocalSearchQueryResultView::MatchOnly {
            0
        } else {
            context_lines
        },
        multiline != LocalSearchQueryMultiline::Off,
        // A caller-sized (grid) page keeps its rows whatever the value
        // width; a streamed page is cut by serialized size.
        (!streamed).then_some(GridPage {
            value_cap: budget_cap.unwrap_or(RESPONSE_VALUE_CHAR_BUDGET),
        }),
    );
    if !expansions.is_empty()
        && let Some(map) = next.get_or_insert_with(|| json!({})).as_object_mut()
    {
        for (index, expansion) in expansions.into_iter().enumerate() {
            let name = if index == 0 {
                "expandValues".to_owned()
            } else {
                format!("expandValues{}", index + 1)
            };
            map.insert(name, expansion);
        }
    }
    let skip_hint = skipped_target_hint(
        root.is_file(),
        stats.files_searched,
        stats.cap_reason.as_deref(),
    )
    .or_else(|| {
        (root.is_file() && parsed.stats.skipped_binary_count.unwrap_or(0) > 0).then(|| {
            "The target file is binary (NUL byte before any text); it was not searched, and no text tool reads it.".into()
        })
    });
    Ok(LocalSearchResult {
        status,
        stats,
        files,
        // File paging is only reported when it routes somewhere: more file pages,
        // or a requested page past the end. Single-page totals live in `stats`,
        // and match-row continuations carry the snapshot in `next.*.query`.
        pagination: (!empty && (total_pages > 1 || out_of_range)).then_some(FilePagination {
            snapshot: snapshot.clone(),
            current_page: page,
            total_pages,
            files_per_page,
            total_files,
            total_matches: (!matches!(
                view,
                LocalSearchQueryResultView::Files | LocalSearchQueryResultView::FilesWithout
            ))
            .then_some(total_matches),
            has_more,
            // Hard ceiling: never advertise a next page past page 1000. Beyond
            // this, deep file pagination is refused by contract (matched in
            // `build_next`) — narrow the search rather than paging indefinitely.
            next_page: (page < total_pages && page < 1000).then_some(page + 1),
            out_of_range,
        }),
        hints: if empty {
            if error_count > 0 {
                vec![unreadable_hint(error_count)]
            } else if let Some(hint) = skip_hint {
                vec![hint]
            } else if binary_cut {
                vec![
                    "No matches in the searched text, but binary file(s) were searched only up to their first NUL byte; absence is not proven for them.".into(),
                    empty_hint(query),
                ]
            } else {
                vec![empty_hint(query)]
            }
        } else {
            vec![]
        },
        next,
        is_partial: coverage_gap || capped,
        terminal_limit,
        warnings,
        source_snapshot: Some(result_identity),
        source_root: output_root.to_path_buf(),
    })
}

/// Serialized chars of one file entry around its rows and path,
/// `{"path":"","matches":[]},`.
const FILE_ENTRY_CHARS: usize = 25;

/// Serialized chars of one match row as a page shows it, with its separator.
fn row_chars(
    matched: &octocode_engine::types::RipgrepMatch,
    display_cap: Option<usize>,
    span_rows: bool,
) -> usize {
    let mut row = project_match(matched, display_cap);
    if !span_rows {
        row.column = None;
    }
    crate::tools::stream_page::json_chars(&row) + 1
}

/// Most chars a streamed file entry's `pagination` can take: its later rows
/// are named as at most [`MAX_MORE_LINE_RANGES`] line ranges plus a count.
fn pagination_chars(file: &octocode_engine::types::RipgrepFile) -> usize {
    let digits = |n: u64| n.checked_ilog10().map_or(1, |log| log as usize + 1);
    let total = file.matches.len();
    let widest_line = file.matches.iter().map(|m| m.line).max().unwrap_or(0);
    let ranges = total.saturating_sub(1).min(MAX_MORE_LINE_RANGES);
    let more_lines =
        ranges * (2 * digits(u64::from(widest_line)) + 2) + ",+ more".len() + digits(total as u64);
    let empty = ItemPagination {
        current_page: None,
        total_pages: None,
        total_matches: u32::try_from(total).unwrap_or(u32::MAX),
        has_more: true,
        next_match_page: None,
        more_lines: Some(String::new()),
        out_of_range: false,
    };
    ",\"pagination\":".len() + crate::tools::stream_page::json_chars(&empty) + more_lines
}

/// Whether every file entry, with all its rows, fits `budget` chars.
fn fits_within(
    files: &[octocode_engine::types::RipgrepFile],
    budget: usize,
    entry_chars: impl Fn(&octocode_engine::types::RipgrepFile) -> usize,
) -> bool {
    let mut used = 0usize;
    files.iter().all(|file| {
        used = used.saturating_add(entry_chars(file));
        used <= budget
    })
}

/// Serialized sizes a streamed page is cut by.
struct StreamCosts<'a> {
    /// A file entry around its rows.
    file: &'a dyn Fn(&octocode_engine::types::RipgrepFile) -> usize,
    /// One match row.
    row: &'a dyn Fn(&octocode_engine::types::RipgrepMatch) -> usize,
}

/// Rows one page shows: (file index, rank-order row range), in page order.
type PageRows = Vec<(usize, std::ops::Range<usize>)>;

/// The default layout: neither page axis is caller-sized and no later match
/// page is asked for. Path-only list views keep file pages.
fn streamed_layout(q: &LocalSearchQuery) -> bool {
    !matches!(
        q.result_view,
        LocalSearchQueryResultView::Files
            | LocalSearchQueryResultView::FilesWithout
            | LocalSearchQueryResultView::CountLines
            | LocalSearchQueryResultView::CountMatches
    ) && q.page_size().is_none()
        && q.max_matches_per_file().is_none()
        && q.match_page() <= 1
}

/// Default-layout pages, cut from one row stream: each file's first
/// `per_file` ranked rows in file order, then every file's remaining rows.
/// Each page holds as much of the stream as fits `budget` serialized chars
/// (at least one row), counting a file's `pagination` whenever its later
/// rows may fall on a later page; the first holds only first-pass rows of
/// at most [`DEFAULT_SNIPPET_PAGE_SIZE`] files, so a large result opens with
/// a lean overview. `whole` puts the entire result on one page. A file's
/// rows on one page are always one contiguous rank range, and its later
/// rows follow on later pages.
fn stream_pages(
    files: &[octocode_engine::types::RipgrepFile],
    per_file: usize,
    whole: bool,
    budget: usize,
    costs: StreamCosts<'_>,
) -> Vec<PageRows> {
    if whole {
        return vec![
            files
                .iter()
                .enumerate()
                .map(|(index, file)| (index, 0..file.matches.len()))
                .collect(),
        ];
    }
    let first = files
        .iter()
        .enumerate()
        .map(|(index, file)| (index, 0..file.matches.len().min(per_file), true));
    let rest = files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.matches.len() > per_file)
        .map(|(index, file)| (index, per_file..file.matches.len(), false));
    let first_page_files = DEFAULT_SNIPPET_PAGE_SIZE as usize;
    let mut pages: Vec<PageRows> = Vec::new();
    let mut page: PageRows = Vec::new();
    let mut slot = std::collections::HashMap::<usize, usize>::new();
    // Files whose `pagination` this page already counts.
    let mut paged = std::collections::HashSet::<usize>::new();
    let mut used = 0usize;
    for (index, rows, overview) in first.chain(rest) {
        let file = &files[index];
        // A row-less file still takes its place as one entry.
        let entries: Vec<Option<usize>> = if rows.is_empty() {
            vec![None]
        } else {
            rows.map(Some).collect()
        };
        for row in entries {
            let more_after = row.is_some_and(|row| row + 1 < file.matches.len());
            let cost = |page_has_file: bool, page_counts_pagination: bool| {
                row.map_or(0, |row| (costs.row)(&file.matches[row]))
                    + if page_has_file { 0 } else { (costs.file)(file) }
                    + if more_after && !page_counts_pagination {
                        pagination_chars(file)
                    } else {
                        0
                    }
            };
            let has_file = slot.contains_key(&index);
            let over_budget = used + cost(has_file, paged.contains(&index)) > budget;
            let first_page_full =
                pages.is_empty() && (!overview || (!has_file && page.len() >= first_page_files));
            if !page.is_empty() && (over_budget || first_page_full) {
                pages.push(std::mem::take(&mut page));
                slot.clear();
                paged.clear();
                used = 0;
            }
            let has_file = slot.contains_key(&index);
            used += cost(has_file, paged.contains(&index));
            if more_after {
                paged.insert(index);
            }
            let span = row.map_or(0..0, |row| row..row + 1);
            match slot.get(&index) {
                Some(&position) => {
                    let range = &mut page[position].1;
                    *range = range.start.min(span.start)..range.end.max(span.end);
                }
                None => {
                    slot.insert(index, page.len());
                    page.push((index, span));
                }
            }
        }
    }
    if !page.is_empty() {
        pages.push(page);
    }
    pages
}

/// A complete result over at most this many files hands off a read of its
/// top file.
const READ_HANDOFF_MAX_FILES: usize = 3;
/// Hits a handed-off read may cover (each opens a ±6-line window).
const READ_HANDOFF_MAX_HITS: usize = 20;

/// localFetch read of the top file's hits in context: the search text as
/// `matchString`, ±6 lines. Paths join the caller's own `path`, so the read
/// resolves wherever the search did.
fn read_handoff(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    top: &SearchFile,
) -> Option<Value> {
    let hits = top.matches.as_ref()?;
    if hits.is_empty() || hits.len() > READ_HANDOFF_MAX_HITS {
        return None;
    }
    let path = if root.is_file() {
        query.path.to_string()
    } else {
        std::path::Path::new(query.path.as_str())
            .join(&top.path)
            .to_string_lossy()
            .into_owned()
    };
    let mut read = json!({
        "path": path,
        "matchString": query.search_text.as_str(),
        "contextLines": 6,
    });
    if query.regex != LocalSearchQueryRegex::Literal
        && regex::escape(&query.search_text) != query.search_text.as_str()
    {
        read["matchStringIsRegex"] = json!(true);
    }
    if query.case_mode == LocalSearchQueryCaseMode::Sensitive {
        read["matchStringCaseSensitive"] = json!(true);
    }
    Some(json!({
        "tool": ToolId::LocalFetch.as_str(),
        "query": read,
        "why": "Read the top file's hits in context.",
        "confidence": "high",
    }))
}

/// Line ranges one localFetch `ranges` read holds (the contract's
/// `maxItems`).
const FETCH_RANGES: usize = 10;
/// The contract maximum of `matchContentLength`.
const MAX_MATCH_CONTENT_LENGTH: usize = 100_000;

/// Serialized chars of one `expandValuesNN` read without its path (and its
/// ranges, which each row pays for).
fn expansion_read_chars(query: &LocalSearchQuery, multiline: bool) -> usize {
    let read = if multiline {
        json!({"expandValues99": {
            "tool": ToolId::LocalSearch.as_str(),
            "query": normalized_query(query, true),
            "why": MULTILINE_READ_WHY,
            "confidence": "exact",
        }})
    } else {
        json!({"expandValues99": {
            "tool": ToolId::LocalFetch.as_str(),
            "query": {"path": "", "ranges": []},
            "why": LINES_READ_WHY,
            "confidence": "exact",
        }})
    };
    crate::tools::stream_page::json_chars(&read)
}
const LINES_READ_WHY: &str = "Read the clipped values' source lines whole.";
const MULTILINE_READ_WHY: &str = "Search this file alone with room for its clipped values.";

/// A caller-sized page: the same `page`/`matchPage` show the same rows at
/// any `matchContentLength`, so one widened query reaches every clipped
/// value while `value_cap` (the response budget's per-row share) still
/// holds them.
struct GridPage {
    value_cap: usize,
}

/// Reads that return every clipped value of a page whole. A grid page whose
/// longest clipped value fits both the contract maximum and the per-row
/// budget gets one widened copy of the query. Otherwise each file gets its
/// own read: a line row's value is its hit line with up to `context` lines
/// on each side, so a localFetch of those source lines (`ranges`, merged, at
/// most [`FETCH_RANGES`] per read) holds it whole, and localFetch pages a
/// long read itself. A clipped multiline row hides how many lines its match
/// spans, so its file is searched again alone with `matchContentLength`
/// raised to its longest clipped value (past the contract maximum, a
/// localFetch from its first line).
fn expand_values(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    root: &std::path::Path,
    files: &[SearchFile],
    context: u32,
    multiline: bool,
    grid: Option<GridPage>,
) -> Vec<Value> {
    let longest = files
        .iter()
        .flat_map(|file| file.matches.iter().flatten())
        .filter(|matched| matched.truncated)
        .filter_map(|matched| matched.original_chars)
        .max();
    let Some(longest) = longest else {
        return Vec::new();
    };
    if let Some(grid) = grid
        && longest <= MAX_MATCH_CONTENT_LENGTH.min(grid.value_cap)
        && longest > effective_match_content_length(query) as usize
    {
        // The snapshot names this value width, so the widened page is a
        // fresh run of the same page (same rows while the source is unchanged).
        let mut widened = normalized_query(query, false);
        widened["matchContentLength"] = json!(longest);
        if let Some(fields) = widened.as_object_mut() {
            fields.remove("snapshot");
        }
        return vec![json!({
            "tool": ToolId::LocalSearch.as_str(),
            "query": widened,
            "why": "The same page with room for every clipped value.",
            "confidence": "exact",
        })];
    }
    let mut reads = Vec::new();
    for file in files {
        let clipped = file
            .matches
            .iter()
            .flatten()
            .filter(|matched| matched.truncated)
            .collect::<Vec<_>>();
        if clipped.is_empty() {
            continue;
        }
        let source = root.join(&file.path);
        let path = paths
            .workspace_relative(&source)
            .unwrap_or_else(|| source.to_string_lossy().into_owned());
        if multiline {
            let longest = clipped
                .iter()
                .filter_map(|matched| matched.original_chars)
                .max()
                .unwrap_or(0)
                .min(MAX_MATCH_CONTENT_LENGTH);
            if longest <= effective_match_content_length(query) as usize {
                // Already at the widest view: read from the first clipped
                // line; localFetch pages the rest of the file.
                let first = clipped
                    .iter()
                    .map(|matched| matched.line)
                    .min()
                    .unwrap_or(1);
                reads.push(json!({
                    "tool": ToolId::LocalFetch.as_str(),
                    "query": {"path": path, "startLine": first.saturating_sub(context).max(1), "endLine": crate::contracts::query_schema_max(ToolId::LocalFetch, None, "endLine")},
                    "why": "Read from the clipped multiline value on; the read pages.",
                    "confidence": "exact",
                }));
                continue;
            }
            let mut search = normalized_query(query, true);
            if let Some(fields) = search.as_object_mut() {
                for key in ["page", "matchPage", "snapshot", "pageSize"] {
                    fields.remove(key);
                }
                fields.insert("path".into(), json!(path));
                fields.insert("matchContentLength".into(), json!(longest));
            }
            reads.push(json!({
                "tool": ToolId::LocalSearch.as_str(),
                "query": search,
                "why": MULTILINE_READ_WHY,
                "confidence": "exact",
            }));
            continue;
        }
        let mut spans = clipped
            .iter()
            .map(|matched| {
                (
                    matched.line.saturating_sub(context).max(1),
                    matched.line.saturating_add(context),
                )
            })
            .collect::<Vec<_>>();
        spans.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (start, end) in spans {
            match merged.last_mut() {
                Some(last) if start <= last.1.saturating_add(1) => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        for chunk in merged.chunks(FETCH_RANGES) {
            let ranges = chunk
                .iter()
                .map(|(start, end)| format!("{start}-{end}"))
                .collect::<Vec<_>>();
            reads.push(json!({
                "tool": ToolId::LocalFetch.as_str(),
                "query": {"path": path, "ranges": ranges},
                "why": LINES_READ_WHY,
                "confidence": "exact",
            }));
        }
    }
    reads
}

/// A query that explicitly targets a single file which the engine then skips
/// (e.g. over the per-file byte ceiling, surfaced as `capReason:"maxFileSize"`
/// with `filesSearched:0`) is a silent false negative without an explanation:
/// nothing was searched, so "no matches" would be misleading.
fn skipped_target_hint(
    single_file: bool,
    files_searched: u32,
    cap_reason: Option<&str>,
) -> Option<String> {
    let reason = cap_reason?;
    if single_file && reason.contains("binaryQuit") {
        return Some(
            "The target file is binary (NUL byte found); it was not searched past that point, and no text tool reads past it."
                .into(),
        );
    }
    (single_file && files_searched == 0).then(|| {
        format!(
            "The target file was skipped ({reason}): nothing was searched. Raise limits or read it with localFetch chunks."
        )
    })
}

/// A continuation whose snapshot no longer describes the source; `next.restart`
/// reruns page 1 without it.
fn stale_snapshot(query: &LocalSearchQuery) -> LocalSearchError {
    let mut restart = normalized_query(query, streamed_layout(query));
    if let Some(object) = restart.as_object_mut() {
        object.remove("snapshot");
    }
    restart["page"] = json!(1);
    restart["matchPage"] = json!(1);
    LocalSearchError {
        code: "staleSnapshot",
        message: "Search snapshot cannot be continued (resultsChanged); restart the search.".into(),
        hints: vec![],
        next: Some(Box::new(json!({
            "restart": {
                "tool": ToolId::LocalSearch.as_str(),
                "query": restart,
                "why": "Start a new search against the current source.",
                "confidence": "exact"
            }
        }))),
    }
}

/// A stored scan's file no longer hashes to the bytes its values came from:
/// drop the scan and restart rather than show or check them against new text.
fn changed_since_scan(query: &LocalSearchQuery) -> LocalSearchError {
    if let Some(snapshot) = query.snapshot() {
        super::manifest::evict(snapshot);
    }
    stale_snapshot(query)
}

/// Every candidate failed before it could be searched: there is no evidence,
/// so this is an execution failure, not an empty result.
fn unreadable_scope(stats: &octocode_engine::types::RipgrepStats) -> LocalSearchError {
    let count = stats.error_count.unwrap_or(0);
    let first = stats.first_error.as_deref().unwrap_or("unknown error");
    LocalSearchError {
        code: "fileAccessFailed",
        message: format!(
            "No file under the search path could be read ({count} failure(s); first: {first})."
        ),
        hints: vec![unreadable_hint(count)],
        next: None,
    }
}

fn unreadable_hint(count: u32) -> String {
    format!(
        "{count} path(s) could not be read (see stats.firstError), so absence is not proven. Check permissions, or narrow path to readable directories."
    )
}

fn empty_hint(query: &LocalSearchQuery) -> String {
    let mut tips = Vec::new();
    if query.case_mode != LocalSearchQueryCaseMode::Insensitive {
        tips.push("caseMode:\"insensitive\"");
    }
    tips.push("a shorter term");
    if query.regex == LocalSearchQueryRegex::Literal {
        tips.push("regex:\"rust\"");
    } else {
        tips.push("regex:\"literal\" if searchText has metacharacters");
    }
    format!("No matches. Try {}.", tips.join(", "))
}

/// Remove `…` window markers and `...` truncation suffixes from a value line.
pub(super) fn strip_clip_markers(line: &str) -> &str {
    let line = line.strip_prefix('…').unwrap_or(line);
    let line = line.strip_suffix("...").unwrap_or(line);
    line.strip_suffix('…').unwrap_or(line)
}

/// Security: the engine clips values (matchOnly spans, matchWindow,
/// matchContentLength, long-line windows) *before* sanitization, so a clipped
/// secret no longer matches any secret pattern and leaks verbatim. For each
/// shown match, sanitize the full source lines the value was cut from; a value
/// line not literally present in that sanitized text overlapped a redaction and
/// is replaced (placeholder for spans, the sanitized match line otherwise).
/// Private-key blocks are detected from the whole file prefix, streamed once
/// without retaining it; only the shown matches' neighborhoods are kept.
///
/// Fails closed: when the source cannot be re-read, no longer holds a shown
/// line, or its neighborhood exceeds the retained-byte budget, the value is
/// replaced by a placeholder and the result is `Unverified`. With `expected`
/// (values from a stored scan), the whole source is hashed while it is read;
/// bytes that differ from the stored digest, or a missing digest, return
/// `Changed` and the values must not be shown. Cancellation stops the read
/// and returns the reason.
pub(super) fn guard_clipped_secrets(
    file: &mut octocode_engine::types::RipgrepFile,
    source: &std::path::Path,
    expected: Option<Option<super::manifest::Digest>>,
    shown: std::ops::Range<usize>,
    security: &ContentSecurity,
    match_only: bool,
    cancel: &impl CancellationCheck,
) -> Result<Verification, String> {
    if expected == Some(None) {
        return Ok(Verification::Changed);
    }
    let end = shown.end.min(file.matches.len());
    let start = shown.start.min(end);
    let shown = &mut file.matches[start..end];
    // 1-based inclusive source lines each shown value needs re-sanitized.
    let windows = shown
        .iter()
        .map(|m| {
            let span = m.value.lines().count().max(1);
            let line = m.line as usize;
            (line.saturating_sub(span).max(1), line + span)
        })
        .collect::<Vec<_>>();
    let Some(last_line) = windows.iter().map(|&(_, hi)| hi).max() else {
        return Ok(Verification::Verified);
    };
    let read = std::fs::File::open(source)
        .map_err(VerifyReadError::from)
        .and_then(|opened| {
            let mut reader = std::io::BufReader::new(HashingReader {
                inner: opened,
                hasher: expected.map(|_| Sha256::new()),
            });
            let read = read_verification_lines(&mut reader, &windows, last_line, cancel)?;
            let digest = drain_digest(reader, cancel)?;
            Ok((read, digest))
        });
    let read = match read {
        Ok((_, Some(digest))) if expected != Some(Some(digest)) => {
            return Ok(Verification::Changed);
        }
        Ok((read, _)) if !read.unclassified => read,
        Ok(_) | Err(VerifyReadError::Io) => {
            for matched in shown.iter_mut() {
                matched.value = UNVERIFIED_PLACEHOLDER.to_owned();
            }
            return Ok(Verification::Unverified);
        }
        Err(VerifyReadError::Cancelled(reason)) => return Err(reason),
    };
    let mut verified = Verification::Verified;
    for (matched, &(lo, hi)) in shown.iter_mut().zip(&windows) {
        if !read.key_ranges.is_empty()
            && crate::security::match_window_intersects_key_block(
                matched.line,
                &matched.value,
                &read.key_ranges,
            )
        {
            matched.value = crate::security::key_fragment_placeholder();
            continue;
        }
        let line = matched.line as usize;
        if line == 0 {
            continue;
        }
        if line > read.lines_read {
            // The source shrank or was replaced since the search.
            matched.value = UNVERIFIED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        }
        let hi = hi.min(read.lines_read);
        let Some(window) = (lo..=hi)
            .map(|n| read.retained.get(&n).map(String::as_str))
            .collect::<Option<Vec<_>>>()
        else {
            matched.value = OVERSIZED_PLACEHOLDER.to_owned();
            verified = Verification::Unverified;
            continue;
        };
        let sanitized = security.sanitize_text(&window.join("\n"), Some(source));
        if !sanitized.has_secrets {
            continue;
        }
        let exposed = matched.value.lines().any(|value_line| {
            let core = strip_clip_markers(value_line);
            !core.is_empty() && !sanitized.content.contains(core)
        });
        if exposed {
            matched.value = if match_only {
                "[REDACTED]".to_owned()
            } else {
                security
                    .sanitize_text(window[line - lo], Some(source))
                    .content
            };
        }
    }
    Ok(verified)
}

/// Outcome of [`guard_clipped_secrets`] for one file's shown matches.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Verification {
    /// Every shown value was checked against the source.
    Verified,
    /// Some value was replaced by a placeholder because it could not be checked.
    Unverified,
    /// The source is not the stored scan's bytes; nothing may be shown.
    Changed,
}

/// Passes reads through, hashing them when a digest is wanted.
struct HashingReader<R> {
    inner: R,
    hasher: Option<Sha256>,
}

impl<R: std::io::Read> std::io::Read for HashingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buf)?;
        if let Some(hasher) = &mut self.hasher {
            hasher.update(&buf[..read]);
        }
        Ok(read)
    }
}

/// Read the rest of a hashed source and return its digest; `None` when the
/// reader was not hashing.
fn drain_digest<R: std::io::Read>(
    mut reader: std::io::BufReader<HashingReader<R>>,
    cancel: &impl CancellationCheck,
) -> Result<Option<super::manifest::Digest>, VerifyReadError> {
    use std::io::BufRead;
    if reader.get_ref().hasher.is_none() {
        return Ok(None);
    }
    let mut since_check = 0usize;
    loop {
        let len = reader.fill_buf()?.len();
        if len == 0 {
            break;
        }
        reader.consume(len);
        since_check += len;
        if since_check >= VERIFY_CHUNK_BYTES {
            since_check = 0;
            cancel.check().map_err(VerifyReadError::Cancelled)?;
        }
    }
    Ok(reader
        .into_inner()
        .hasher
        .map(|hasher| hasher.finalize().into()))
}

/// Value shown in place of a match whose source could not be re-read for the
/// clipped-secret check.
const UNVERIFIED_PLACEHOLDER: &str = "[REDACTED: source unreadable for secret check]";
/// Value shown in place of a match whose source neighborhood exceeds
/// [`MAX_VERIFY_RETAINED_BYTES`].
const OVERSIZED_PLACEHOLDER: &str = "[REDACTED: source lines too large for secret check]";
/// Source bytes one file's verification may retain for the shown matches'
/// neighborhoods; the rest of the prefix is streamed for key-block state only.
const MAX_VERIFY_RETAINED_BYTES: usize = 16 * 1024 * 1024;
/// Bytes of a line outside every neighborhood buffered to classify it; a
/// longer line is consumed in chunks without being stored.
const MAX_PROBE_LINE_BYTES: usize = 64 * 1024;
/// Lines (or 1 MiB chunks of one long line) between cancellation checks.
const VERIFY_CANCEL_EVERY: usize = 4096;
const VERIFY_CHUNK_BYTES: usize = 1024 * 1024;

enum VerifyReadError {
    /// The source could not be opened or read.
    Io,
    Cancelled(String),
}

impl From<std::io::Error> for VerifyReadError {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}

/// One streamed pass over a source prefix for the clipped-secret check.
struct VerificationLines {
    /// Private-key block ranges over every line read.
    key_ranges: Vec<(u32, u32)>,
    /// Neighborhood lines (1-based) that fit the retained-byte budget.
    retained: std::collections::BTreeMap<usize, String>,
    retained_bytes: usize,
    lines_read: usize,
    /// A line too long to buffer mentions `PRIVATE KEY`, so key-block state
    /// is unknown and nothing read from this source can be trusted.
    unclassified: bool,
}

/// Stream up to `limit` lines of `reader`, tracking private-key blocks over
/// all of them and keeping only lines inside `windows` (1-based inclusive),
/// within [`MAX_VERIFY_RETAINED_BYTES`]. Any read failure is an error (a
/// short source just yields fewer lines).
fn read_verification_lines(
    reader: &mut impl std::io::BufRead,
    windows: &[(usize, usize)],
    limit: usize,
    cancel: &impl CancellationCheck,
) -> Result<VerificationLines, VerifyReadError> {
    let mut tracker = crate::security::KeyBlockTracker::default();
    let mut read = VerificationLines {
        key_ranges: Vec::new(),
        retained: std::collections::BTreeMap::new(),
        retained_bytes: 0,
        lines_read: 0,
        unclassified: false,
    };
    let mut buf = Vec::new();
    while read.lines_read < limit {
        let number = read.lines_read + 1;
        if number.is_multiple_of(VERIFY_CANCEL_EVERY) {
            cancel.check().map_err(VerifyReadError::Cancelled)?;
        }
        let wanted = windows.iter().any(|&(lo, hi)| lo <= number && number <= hi);
        let cap = if wanted {
            MAX_VERIFY_RETAINED_BYTES.saturating_sub(read.retained_bytes)
        } else {
            MAX_PROBE_LINE_BYTES
        };
        buf.clear();
        let Some(line) = read_bounded_line(reader, &mut buf, cap, cancel)? else {
            break;
        };
        read.lines_read = number;
        match line {
            BoundedLine::Whole => {
                let text = String::from_utf8_lossy(&buf);
                let text = text.trim_end_matches(['\n', '\r']);
                tracker.push(text);
                if wanted {
                    read.retained_bytes += text.len();
                    read.retained.insert(number, text.to_owned());
                }
            }
            BoundedLine::Overflow { mentions_key } => {
                // Every key boundary contains `PRIVATE KEY`; a long line
                // without it is an ordinary line for the block state.
                read.unclassified |= mentions_key;
                tracker.push("");
            }
        }
    }
    read.key_ranges = tracker.finish();
    Ok(read)
}

enum BoundedLine {
    /// The whole line (with its terminator) is in the buffer.
    Whole,
    /// The line exceeded the cap and was consumed without being kept.
    Overflow { mentions_key: bool },
}

/// Read one line into `buf` if it fits `cap` bytes; otherwise consume it in
/// chunks, noting whether it mentions `PRIVATE KEY`. `None` at end of file.
fn read_bounded_line(
    reader: &mut impl std::io::BufRead,
    buf: &mut Vec<u8>,
    cap: usize,
    cancel: &impl CancellationCheck,
) -> Result<Option<BoundedLine>, VerifyReadError> {
    const NEEDLE: &[u8] = b"PRIVATE KEY";
    let mut overflow = false;
    let mut mentions_key = false;
    let mut consumed = 0usize;
    let mut since_check = 0usize;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            break;
        }
        let (take, done) = match chunk.iter().position(|&byte| byte == b'\n') {
            Some(at) => (at + 1, true),
            None => (chunk.len(), false),
        };
        let part = &chunk[..take];
        if !overflow && buf.len() + take > cap {
            overflow = true;
        }
        if overflow {
            // Keep a needle-length tail so a mention split across chunks
            // is still seen.
            buf.extend_from_slice(part);
            mentions_key |= buf.windows(NEEDLE.len()).any(|w| w == NEEDLE);
            let keep = buf.len().min(NEEDLE.len() - 1);
            buf.drain(..buf.len() - keep);
        } else {
            buf.extend_from_slice(part);
        }
        reader.consume(take);
        consumed += take;
        since_check += take;
        if since_check >= VERIFY_CHUNK_BYTES {
            since_check = 0;
            cancel.check().map_err(VerifyReadError::Cancelled)?;
        }
        if done {
            break;
        }
    }
    if consumed == 0 {
        return Ok(None);
    }
    Ok(Some(if overflow {
        buf.clear();
        BoundedLine::Overflow { mentions_key }
    } else {
        BoundedLine::Whole
    }))
}

/// Order one clipped file's rows for paging: by lexical hit rank, then
/// distinct text before repeats, stable by line.
fn rank_file_rows(matches: &mut Vec<octocode_engine::types::RipgrepMatch>) {
    matches.sort_by_key(|matched| std::cmp::Reverse(matched.rank.unwrap_or(1)));
    let mut seen = std::collections::HashSet::new();
    let (distinct, repeats): (Vec<_>, Vec<_>) = std::mem::take(matches)
        .into_iter()
        .partition(|matched| seen.insert(matched.value.trim().to_owned()));
    matches.extend(distinct);
    matches.extend(repeats);
}

fn project_match(
    matched: &octocode_engine::types::RipgrepMatch,
    max_chars: Option<usize>,
) -> SearchMatch {
    // matchOnly display cap: clip the exact span to the public display bound.
    let cut = max_chars.and_then(|limit| {
        matched
            .value
            .char_indices()
            .nth(limit)
            .map(|(byte, _)| (byte, limit))
    });
    if let Some((byte, chars)) = cut {
        return SearchMatch {
            line: matched.line,
            column: Some(matched.column),
            value: matched.value[..byte].into(),
            match_lines: None,
            count: matched.count,
            truncated: true,
            original_chars: Some(matched.value.chars().count()),
            returned_chars: Some(chars),
        };
    }
    // Content-view path: the engine already clipped the assembled snippet to
    // maxSnippetChars and reported the pre-truncation length via
    // `original_chars`; surface it as the same truncation indicator.
    let truncated = matched.original_chars.is_some();
    SearchMatch {
        line: matched.line,
        column: Some(matched.column),
        value: matched.value.clone(),
        match_lines: None,
        count: matched.count,
        truncated,
        original_chars: matched.original_chars.map(|chars| chars as usize),
        returned_chars: truncated.then(|| matched.value.chars().count()),
    }
}

/// Source line range and lines of a content-view row's ±`context` window, or
/// `None` when the value is not a plain, untruncated window (clipped,
/// redacted, grouped, or a shape the window arithmetic cannot account for).
/// The engine joins up to `context` contiguous lines on each side of the match
/// line, clamped at the file start/end.
fn context_window(matched: &SearchMatch, context: u32) -> Option<(u32, Vec<&str>)> {
    if matched.truncated || matched.count.is_some() || matched.line == 0 {
        return None;
    }
    let lines: Vec<&str> = matched.value.split('\n').collect();
    let before = context.min(matched.line - 1);
    let after = u32::try_from(lines.len()).ok()?.checked_sub(before + 1)?;
    (after <= context).then_some((matched.line - before, lines))
}

/// Merge rows whose context windows overlap or touch into one block, so each
/// source line is emitted once, numbered. A merged block keeps the first row's
/// `line`/`column`, and `matchLines` lists every matched line it holds. Rows
/// merge only when both windows are plain and their shared lines are
/// byte-identical, so a clipped or redacted window is never spliced.
/// Merges overlapping windows while the joined block stays within the
/// `max_chars` (`matchContentLength`) budgets of the rows it joins, so a
/// block is never larger than those rows returned separately.
fn merge_context_windows(
    rows: Vec<SearchMatch>,
    context: u32,
    max_chars: usize,
) -> Vec<SearchMatch> {
    struct Block {
        head: SearchMatch,
        start: u32,
        lines: Vec<String>,
        match_lines: Vec<u32>,
    }
    // Every window is numbered `<line>\t<text>`, so a cited
    // line never has to be counted from `line`.
    fn flush(block: Block) -> SearchMatch {
        let mut head = block.head;
        // One number per window line, blank lines included (a window can end
        // on an empty line, which a split-based numberer would drop).
        head.value = block
            .lines
            .iter()
            .zip(block.start..)
            .map(|(line, number)| format!("{number}{}{line}", crate::runtime::numbered::SEPARATOR))
            .collect::<Vec<_>>()
            .join("\n");
        if block.match_lines.len() > 1 {
            head.match_lines = Some(block.match_lines);
        }
        head
    }
    let mut out = Vec::with_capacity(rows.len());
    let mut open: Option<Block> = None;
    for row in rows {
        let Some((start, lines)) = context_window(&row, context) else {
            out.extend(open.take().map(flush));
            out.push(row);
            continue;
        };
        if let Some(block) = open.as_mut() {
            let end = block.start + block.lines.len() as u32; // exclusive
            let last_match = block.match_lines.last().copied().unwrap_or(0);
            let shared_agrees = (start..end.min(start + lines.len() as u32))
                .all(|n| block.lines[(n - block.start) as usize] == lines[(n - start) as usize]);
            let fresh = (end.saturating_sub(start) as usize).min(lines.len());
            let grown: usize = block
                .lines
                .iter()
                .map(|l| l.chars().count() + 1)
                .sum::<usize>()
                + lines
                    .iter()
                    .skip(fresh)
                    .map(|l| l.chars().count() + 1)
                    .sum::<usize>();
            if row.line > last_match
                && start >= block.start
                && start <= end
                && shared_agrees
                && grown.saturating_sub(1) <= max_chars.saturating_mul(block.match_lines.len() + 1)
            {
                block
                    .lines
                    .extend(lines.iter().skip(fresh).map(|l| (*l).to_owned()));
                block.match_lines.push(row.line);
                continue;
            }
        }
        let lines = lines.into_iter().map(str::to_owned).collect();
        let line = row.line;
        out.extend(
            open.replace(Block {
                head: row,
                start,
                lines,
                match_lines: vec![line],
            })
            .map(flush),
        );
    }
    out.extend(open.map(flush));
    out
}

pub(crate) struct PolicyFilter(pub(crate) PathPolicy);
impl RipgrepPathFilter for PolicyFilter {
    fn allows(&self, path: &std::path::Path, is_dir: bool) -> bool {
        if is_dir {
            self.0.validate(path).is_ok()
        } else {
            self.0.validate_read(path).is_ok()
        }
    }
}

/// The engine compiles `searchText` inside its own wrapper group, so the raw
/// parse error echoes that internal pattern. Report the caller's pattern and
/// the parser's reason instead.
fn regex_error_message(search_text: &str, raw: Option<&str>) -> String {
    let reason = raw
        .and_then(|raw| {
            raw.lines()
                .rev()
                .find_map(|line| line.trim().strip_prefix("error:"))
                .map(str::trim)
                .or_else(|| Some(raw.trim()))
        })
        .filter(|reason| !reason.is_empty())
        .unwrap_or("invalid regex pattern");
    format!("Invalid regex searchText `{search_text}`: {reason}")
}

/// `invalidRegex` with a literal-search repair continuation.
fn invalid_regex(query: &LocalSearchQuery, message: String) -> LocalSearchError {
    // The caller's own fields, not the runtime-normalized ones: a fresh
    // page-1 search re-derives the same view defaults (contextLines,
    // matchContentLength, pageSize), so spelling them out only adds bytes.
    let mut repaired = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    if let Some(object) = repaired.as_object_mut() {
        object.retain(|_, value| !value.is_null());
        for cursor in ["snapshot", "page", "matchPage"] {
            object.remove(cursor);
        }
    }
    let pcre2 = query.regex == LocalSearchQueryRegex::Pcre2;
    let mut hints = vec![
        "Use regex:\"literal\" for exact text, or escape metacharacters ( [ . per alternative to keep regex matching.".to_owned(),
    ];
    // `poll_(proceed|budget` means the group `poll_(proceed|budget)`: closing
    // it keeps every alternative anchored, where escaping the `(` would turn
    // `budget` into a bare, much broader alternative. Otherwise an alternation
    // stays a regex: a literal search for `a(|b(` matches nothing, while each
    // alternative escaped finds every anchor.
    let why = if let Some(text) = close_unclosed_group(&query.search_text, pcre2) {
        hints.push(format!(
            "The repair reads searchText as `{text}`; send regex:\"literal\" to match the text exactly instead."
        ));
        repaired["searchText"] = json!(text);
        "Close the unclosed group so its alternatives stay inside it."
    } else if let Some(text) = repair_alternation(&query.search_text, pcre2) {
        repaired["searchText"] = json!(text);
        "Search each alternative with its metacharacters escaped."
    } else {
        repaired["regex"] = json!("literal");
        "Search searchText as literal text."
    };
    LocalSearchError {
        code: "invalidRegex",
        message,
        hints,
        next: Some(Box::new(json!({"repair":{
            "tool":ToolId::LocalSearch.as_str(),
            "query":repaired,
            "why":why
        }}))),
    }
}

/// `x_(a|b` → `x_(a|b)`: exactly one group is left open and everything after
/// it is two or more bare word alternatives. `None` otherwise, so call syntax
/// such as `f("a"|g(` keeps the per-alternative escape.
fn close_unclosed_group(text: &str, pcre2: bool) -> Option<String> {
    let mut open = Vec::new();
    let mut in_class = false;
    let mut chars = text.char_indices();
    while let Some((index, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '[' if !in_class => in_class = true,
            ']' if in_class => in_class = false,
            '(' if !in_class => open.push(index),
            ')' if !in_class => {
                open.pop()?;
            }
            _ => {}
        }
    }
    let [start] = open.as_slice() else {
        return None;
    };
    let alternatives = text[start + 1..].split('|').collect::<Vec<_>>();
    let bare = |part: &&str| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-'))
    };
    if alternatives.len() < 2 || !alternatives.iter().all(bare) {
        return None;
    }
    let closed = format!("{text})");
    octocode_engine::portable::validate_ripgrep_pattern(&closed, false, pcre2)
        .valid
        .then_some(closed)
}

/// `a(|b|c[` → `a\(|b|c\[`: split on unescaped `|`, keep alternatives that
/// parse on their own, escape the rest. `None` for a single alternative or
/// when the joined result still fails to parse.
fn repair_alternation(text: &str, pcre2: bool) -> Option<String> {
    let mut parts = vec![String::new()];
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let part = parts.last_mut()?;
                part.push(c);
                if let Some(next) = chars.next() {
                    part.push(next);
                }
            }
            '|' => parts.push(String::new()),
            _ => parts.last_mut()?.push(c),
        }
    }
    if parts.len() < 2 {
        return None;
    }
    let valid = |pattern: &str| {
        octocode_engine::portable::validate_ripgrep_pattern(pattern, false, pcre2).valid
    };
    let repaired = parts
        .iter()
        .map(|part| {
            if !part.is_empty() && valid(part) {
                part.clone()
            } else {
                regex::escape(part)
            }
        })
        .collect::<Vec<_>>()
        .join("|");
    valid(&repaired).then_some(repaired)
}

fn cancelled(message: String) -> LocalSearchError {
    LocalSearchError {
        code: "cancelled",
        message,
        hints: vec![],
        next: None,
    }
}

/// Default ±context window per match row: the hit line alone, except the
/// `detailed` view, which exists to show surrounding code.
/// Per-hit character budget. An omitted matchContentLength scales with the
/// effective context window so requested context is not silently clipped;
/// continuations and snapshot identity must use this same value.
fn effective_match_content_length(q: &LocalSearchQuery) -> u32 {
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

fn default_context_lines(view: LocalSearchQueryResultView) -> u32 {
    if view == LocalSearchQueryResultView::Detailed {
        3
    } else {
        0
    }
}

/// Default files per page: snippet views stay lean; path-only views are cheap.
fn default_page_size(view: LocalSearchQueryResultView) -> u32 {
    match view {
        LocalSearchQueryResultView::Files
        | LocalSearchQueryResultView::FilesWithout
        | LocalSearchQueryResultView::CountLines
        | LocalSearchQueryResultView::CountMatches => DEFAULT_LIST_PAGE_SIZE,
        _ => DEFAULT_SNIPPET_PAGE_SIZE,
    }
}

/// Views whose match rows carry a context window (so `contextLines` matters).
fn uses_context(view: LocalSearchQueryResultView) -> bool {
    matches!(
        view,
        LocalSearchQueryResultView::Paginated
            | LocalSearchQueryResultView::Content
            | LocalSearchQueryResultView::Detailed
    )
}

/// The query with its effective defaults, as continuations carry it. A
/// streamed layout carries no `pageSize`: its pages are cut by the budget.
fn normalized_query(q: &LocalSearchQuery, streamed: bool) -> Value {
    let mut value = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    // `LocalSearchQuery` serializes to a JSON object.
    #[allow(clippy::expect_used)]
    let o = value.as_object_mut().expect("request object");
    o.retain(|_, v| !v.is_null());
    o.entry("regex").or_insert(json!("rust"));
    o.entry("caseMode").or_insert(json!("smart"));
    let view = q.result_view;
    if uses_context(view) {
        o.entry("contextLines")
            .or_insert(json!(default_context_lines(view)));
    }
    o.entry("matchContentLength")
        .or_insert(json!(effective_match_content_length(q)));
    o.entry("multiline").or_insert(json!("off"));
    o.entry("sort").or_insert(json!("relevance"));
    o.entry("unique").or_insert(json!("off"));
    o.entry("matchPage").or_insert(json!(1));
    o.entry("page").or_insert(json!(1));
    o.entry("resultView").or_insert(json!("paginated"));
    if !streamed {
        o.entry("pageSize")
            .or_insert(json!(default_page_size(view)));
    }
    value
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

fn build_next(
    q: &LocalSearchQuery,
    page: u32,
    total_pages: u32,
    leftover_matches: bool,
    match_page: u32,
    snapshot: Option<&str>,
    streamed: bool,
) -> Option<Value> {
    let mut map = serde_json::Map::new();
    let base = normalized_query(q, streamed);
    // Hard 1000-page ceiling (mirrors the pagination `next_page` guard above): a
    // `nextPage` continuation is never emitted past page 1000, so file paging is
    // bounded by contract. Callers must narrow the query to reach later results.
    if page < total_pages && page < 1000 {
        let mut n = base.clone();
        n["page"] = json!(page + 1);
        // A new file page starts at each file's first match row.
        n["matchPage"] = json!(1);
        if let Some(s) = snapshot {
            n["snapshot"] = json!(s)
        };
        map.insert(
            "nextPage".into(),
            json!({"tool":ToolId::LocalSearch.as_str(),"query":n,"confidence":"exact"}),
        );
    }
    if leftover_matches && match_page < 1000 {
        let mut n = base;
        n["matchPage"] = json!(match_page + 1);
        if let Some(s) = snapshot {
            n["snapshot"] = json!(s)
        };
        map.insert(
            "nextMatchPage".into(),
            json!({"tool":ToolId::LocalSearch.as_str(),"query":n,"confidence":"exact"}),
        );
    }
    (!map.is_empty()).then_some(Value::Object(map))
}
fn policy_identity(paths: &PathPolicy) -> String {
    let roots = paths
        .allowed_roots()
        .iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    hex::encode(Sha256::digest(
        serde_json::to_vec(&roots).unwrap_or_default(),
    ))
}

/// Snapshot identity: the validated root plus every field that changes which
/// matches are collected or how their values read, with defaults applied,
/// then the collected result itself. Pagination fields (`page`, `matchPage`,
/// `pageSize`, `maxMatchesPerFile`) only select from that result and stay
/// out, so a continuation may change them while keeping its snapshot.
fn fingerprint(
    q: &LocalSearchQuery,
    root: &std::path::Path,
    files: &[octocode_engine::types::RipgrepFile],
    stats: &octocode_engine::types::RipgrepStats,
    page_budget: usize,
) -> String {
    let mut identity = serde_json::Map::new();
    // The page cut: a continuation served under another response window
    // would cut other pages, so it restarts instead of skipping rows.
    identity.insert("pageChars".into(), json!(page_budget));
    identity.insert(
        "defaultExcludes".into(),
        json!(q.default_excludes.defaults()),
    );
    identity.insert("searchText".into(), json!(q.search_text));
    identity.insert(
        "mode".into(),
        json!(match q.result_view {
            LocalSearchQueryResultView::Detailed => "detailed",
            _ => "paginated",
        }),
    );
    identity.insert(
        "regex".into(),
        json!(match q.regex {
            LocalSearchQueryRegex::Literal => "fixed",
            LocalSearchQueryRegex::Pcre2 => "perl",
            LocalSearchQueryRegex::Rust => "smart",
        }),
    );
    identity.insert(
        "caseMode".into(),
        json!(match q.case_mode {
            LocalSearchQueryCaseMode::Sensitive => "sensitive",
            LocalSearchQueryCaseMode::Insensitive => "insensitive",
            LocalSearchQueryCaseMode::Smart => "smart",
        }),
    );
    identity.insert(
        "contextLines".into(),
        json!(
            q.context_lines()
                .unwrap_or_else(|| default_context_lines(q.result_view))
        ),
    );
    identity.insert(
        "matchContentLength".into(),
        json!(effective_match_content_length(q)),
    );
    identity.insert(
        "multiline".into(),
        json!(match q.multiline {
            LocalSearchQueryMultiline::Off => "off",
            LocalSearchQueryMultiline::On => "on",
            LocalSearchQueryMultiline::Dotall => "dotall",
        }),
    );
    identity.insert(
        "sort".into(),
        json!(match q.sort {
            LocalSearchQuerySort::Relevance => "relevance",
            LocalSearchQuerySort::Traversal => "traversal",
            LocalSearchQuerySort::MatchCount => "matchCount",
            LocalSearchQuerySort::Path => "path",
            LocalSearchQuerySort::Modified => "modified",
            LocalSearchQuerySort::Accessed => "accessed",
            LocalSearchQuerySort::Created => "created",
        }),
    );
    identity.insert(
        "output".into(),
        json!(match q.result_view {
            LocalSearchQueryResultView::MatchOnly => "matchOnly",
            LocalSearchQueryResultView::Files => "files",
            LocalSearchQueryResultView::FilesWithout => "filesWithout",
            LocalSearchQueryResultView::CountLines => "countLines",
            LocalSearchQueryResultView::CountMatches => "countMatches",
            _ => "content",
        }),
    );
    identity.insert(
        "unique".into(),
        json!(match q.unique {
            LocalSearchQueryUnique::Off => "off",
            LocalSearchQueryUnique::List => "list",
            LocalSearchQueryUnique::Count => "count",
        }),
    );
    macro_rules! opt {
        ($name:literal,$value:expr) => {
            if let Some(v) = $value {
                identity.insert($name.into(), json!(v));
            }
        };
    }
    opt!("wholeWord", q.whole_word);
    opt!("invertMatch", q.invert_match);
    opt!("include", (!q.include.is_empty()).then_some(&q.include));
    opt!("exclude", (!q.exclude.is_empty()).then_some(&q.exclude));
    opt!(
        "excludeDir",
        (!q.exclude_dir.is_empty()).then_some(&q.exclude_dir)
    );
    opt!("noIgnore", q.no_ignore);
    opt!("hidden", q.hidden);
    opt!("maxDepth", q.max_depth);
    opt!("langType", q.lang_type.as_ref());
    opt!("matchWindow", q.match_window);
    opt!("sortReverse", q.reverse);
    let mut entries = identity.into_iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let query_key = hex::encode(Sha256::digest(
        serde_json::to_vec(&json!([root.to_string_lossy(), entries])).unwrap_or_default(),
    ));
    let file_values = files
        .iter()
        .map(|f| {
            let matches = f
                .matches
                .iter()
                .map(|m| {
                    let mut o = serde_json::Map::new();
                    o.insert("line".into(), json!(m.line));
                    o.insert("column".into(), json!(m.column));
                    o.insert("value".into(), json!(m.value));
                    if let Some(c) = m.count {
                        o.insert("count".into(), json!(c));
                    }
                    Value::Object(o)
                })
                .collect::<Vec<_>>();
            json!({"path":f.path,"matchCount":f.match_count,"matches":matches})
        })
        .collect::<Vec<_>>();
    let canonical = canonicalize(
        json!([query_key,file_values,{"totalOccurrences":stats.match_count.unwrap_or(0),"matchedLines":stats.matched_lines.unwrap_or(0),"filesMatched":stats.files_matched.unwrap_or(files.len() as u32),"filesSearched":stats.files_searched.unwrap_or(0),"capped":stats.capped.unwrap_or(false),"capReason":stats.cap_reason,"errorCount":stats.error_count.filter(|n|*n>0),"firstError":stats.first_error} ]),
    );
    format!(
        "lexical-live-v1:{}",
        hex::encode(Sha256::digest(
            serde_json::to_vec(&canonical).unwrap_or_default()
        ))
    )
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    fn row(line: u32, value: &str) -> SearchMatch {
        SearchMatch {
            line,
            column: None,
            value: value.into(),
            match_lines: None,
            count: None,
            truncated: false,
            original_chars: None,
            returned_chars: None,
        }
    }

    #[test]
    fn splices_only_when_shared_lines_agree() {
        // Rows 5 and 6 with ±1 context share lines 5..=6.
        let merged =
            merge_context_windows(vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")], 1, 500);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].value, "4\tl4\n5\tl5\n6\tl6\n7\tl7");
        assert_eq!(merged[0].match_lines, Some(vec![5, 6]));
        // A window ending on a blank line keeps that line numbered.
        let blank = merge_context_windows(vec![row(5, "l4\nl5\n")], 1, 500);
        assert_eq!(blank[0].value, "4\tl4\n5\tl5\n6\t");
        // A redacted/rewritten shared line keeps both rows verbatim.
        let apart = merge_context_windows(
            vec![row(5, "l4\nl5\nl6"), row(6, "l5\n[REDACTED]\nl7")],
            1,
            500,
        );
        assert_eq!(apart.len(), 2);
        assert!(apart.iter().all(|m| m.match_lines.is_none()));
        // A value whose line count cannot be a ±1 window is never spliced.
        let odd = merge_context_windows(vec![row(5, "l4\nl5\nl6"), row(6, "one line")], 1, 500);
        assert_eq!(odd.len(), 2);
        assert_eq!(odd[1].value, "one line");
        // Disjoint windows (gap at line 7..) stay separate rows.
        let gap = merge_context_windows(vec![row(2, "l1\nl2\nl3"), row(9, "l8\nl9\nl10")], 1, 500);
        assert_eq!(gap.len(), 2);
        // A merged block may use the budgets of the rows it replaces (here
        // 2 × 8 chars), so it is never larger than those rows unmerged.
        let within = merge_context_windows(vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")], 1, 8);
        assert_eq!(within.len(), 1, "two 8-char rows merge into 11 chars");
        let capped = merge_context_windows(vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")], 1, 5);
        assert_eq!(
            capped.len(),
            2,
            "a merge must not exceed the merged rows' matchContentLength budgets"
        );
    }

    #[test]
    fn wide_overlapping_windows_merge_up_to_the_rows_combined_budget() {
        // Three ±3 windows over 6-char lines (48 chars each): any two merged
        // exceed one row's 60-char cap, all three fit their combined budget.
        let lines: Vec<String> = (1..=20).map(|n| format!("line{n:02}")).collect();
        let window = |line: u32| {
            let lo = line.saturating_sub(3).max(1) as usize;
            let hi = (line + 3).min(20) as usize;
            lines[lo - 1..hi].join("\n")
        };
        let rows = vec![row(5, &window(5)), row(7, &window(7)), row(9, &window(9))];
        let merged = merge_context_windows(rows, 3, 60);
        assert_eq!(merged.len(), 1, "{merged:?}");
        let numbered = (2..=12)
            .map(|n| format!("{n}\tline{n:02}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(merged[0].value, numbered);
        assert_eq!(merged[0].match_lines, Some(vec![5, 7, 9]));
    }
}

#[cfg(test)]
mod repair_tests {
    use super::*;

    #[test]
    fn invalid_regex_message_shows_the_callers_pattern_not_the_wrapper() {
        let message = regex_error_message(
            "(",
            Some("regex parse error:\n    (?:()\n    ^\nerror: unclosed group"),
        );
        assert_eq!(message, "Invalid regex searchText `(`: unclosed group");
        assert!(!message.contains("(?:"));
        assert_eq!(
            regex_error_message("[", None),
            "Invalid regex searchText `[`: invalid regex pattern"
        );
    }

    #[test]
    fn invalid_regex_repair_keeps_only_caller_fields() {
        let query: LocalSearchQuery = serde_json::from_value(json!({
            "path":"/tmp","searchText":"(unclosed","goal": "test", "reasoning":"r","page":3,"pageSize":5
        }))
        .expect("query");
        let error = invalid_regex(&query, "unclosed group".into());
        let next = error.next.expect("repair");
        let repair = &next["repair"]["query"];
        assert_eq!(repair["regex"], "literal");
        assert_eq!(repair["pageSize"], 5, "caller fields survive: {repair}");
        for derived in ["contextLines", "matchContentLength", "page", "matchPage"] {
            assert!(repair.get(derived).is_none(), "{derived} leaked: {repair}");
        }
        crate::contracts::validate_query("localSearch", repair.clone())
            .expect("repair query is contract-valid");
    }

    #[test]
    fn an_invalid_alternation_is_repaired_per_alternative_not_as_one_literal() {
        let query: LocalSearchQuery = serde_json::from_value(json!({
            "path":"/tmp","goal":"g","reasoning":"r",
            "searchText":"EndProcessProperty(|SetPropertyPresence(|PropertyPresence\\.None"
        }))
        .expect("query");
        let error = invalid_regex(&query, "unclosed group".into());
        let repair = &error.next.expect("repair")["repair"]["query"];
        assert_eq!(
            repair["searchText"],
            "EndProcessProperty\\(|SetPropertyPresence\\(|PropertyPresence\\.None",
            "broken alternatives are escaped, valid ones kept: {repair}"
        );
        assert!(
            repair.get("regex").is_none_or(|mode| mode == "rust"),
            "{repair}"
        );
        let text = repair["searchText"].as_str().expect("text");
        assert!(octocode_engine::portable::validate_ripgrep_pattern(text, false, false).valid);
        crate::contracts::validate_query("localSearch", repair.clone())
            .expect("repair query is contract-valid");
        // A single anchor keeps the literal repair.
        let single: LocalSearchQuery = serde_json::from_value(json!({
            "path":"/tmp","goal":"g","reasoning":"r","searchText":"call("
        }))
        .expect("query");
        let error = invalid_regex(&single, "unclosed group".into());
        assert_eq!(
            error.next.expect("repair")["repair"]["query"]["regex"],
            "literal"
        );
    }

    #[test]
    fn an_unclosed_group_of_bare_alternatives_is_closed_not_widened() {
        let query: LocalSearchQuery = serde_json::from_value(json!({
            "path":"/tmp","goal":"g","reasoning":"r","searchText":"poll_(proceed|budget","regex":"rust"
        }))
        .expect("query");
        let error = invalid_regex(&query, "unclosed group".into());
        let repair = &error.next.expect("repair")["repair"]["query"];
        assert_eq!(
            repair["searchText"], "poll_(proceed|budget)",
            "the group closes; `budget` never becomes a bare alternative: {repair}"
        );
        assert!(
            error
                .hints
                .iter()
                .any(|hint| hint.contains("regex:\"literal\"")),
            "the literal reading stays one hop away: {:?}",
            error.hints
        );
        crate::contracts::validate_query("localSearch", repair.clone())
            .expect("repair query is contract-valid");
        // Alternatives that carry call syntax keep the per-alternative escape.
        for (text, expected) in [
            (
                "capture_limited|hydrate_candidate|capture_resource(",
                "capture_limited|hydrate_candidate|capture_resource\\(",
            ),
            (
                "insert_header(\"retry-after\"|append_header(\"retry-after\"|set_body_json",
                "insert_header\\(\"retry\\-after\"|append_header\\(\"retry\\-after\"|set_body_json",
            ),
        ] {
            assert_eq!(close_unclosed_group(text, false), None, "{text}");
            assert_eq!(repair_alternation(text, false).as_deref(), Some(expected));
        }
    }

    #[test]
    fn escaped_bars_do_not_split_alternatives() {
        assert_eq!(repair_alternation("a\\|b(", false), None, "one alternative");
        assert_eq!(repair_alternation("x[|y", false).as_deref(), Some("x\\[|y"));
    }
}

/// The binary-quit files a warning names: every root-relative path the
/// engine reports, then the count of any it could not name
/// (`a.bin, b.dat and 3 more`).
fn binary_file_list(paths: &[String], total: u32, root: &std::path::Path) -> String {
    if paths.is_empty() {
        return "a file with a NUL byte was".into();
    }
    let mut names = paths
        .iter()
        .map(|path| {
            let path = std::path::Path::new(path);
            path.strip_prefix(root)
                .ok()
                .filter(|relative| !relative.as_os_str().is_empty())
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>()
        .join(", ");
    let unnamed = (total as usize).saturating_sub(paths.len());
    if unnamed > 0 {
        names.push_str(&format!(" and {unnamed} more"));
    }
    let verb = if paths.len() + unnamed == 1 {
        "was"
    } else {
        "were"
    };
    format!("{names} {verb}")
}

/// Line runs `moreLines` names before it summarizes the rest as a count.
const MAX_MORE_LINE_RANGES: usize = 24;

/// Sorted, distinct line numbers as runs: `711-717,802`. Past `max_ranges`
/// runs the rest is a count (`,+40 more`), so a file with thousands of
/// scattered hits costs a bounded hint, and the match pages still hold them.
fn line_ranges(lines: &[u32], max_ranges: usize) -> String {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &line in lines {
        match runs.last_mut() {
            Some((_, end)) if line == end.saturating_add(1) => *end = line,
            _ => runs.push((line, line)),
        }
    }
    let mut out = runs
        .iter()
        .take(max_ranges)
        .map(|&(start, end)| {
            if start == end {
                start.to_string()
            } else {
                format!("{start}-{end}")
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let omitted: u64 = runs
        .iter()
        .skip(max_ranges)
        .map(|&(start, end)| u64::from(end - start) + 1)
        .sum();
    if omitted > 0 {
        out.push_str(&format!(",+{omitted} more"));
    }
    out
}

#[cfg(test)]
mod line_range_tests {
    use super::line_ranges;

    #[test]
    fn more_lines_compress_runs_and_cap_scattered_hits_with_a_count() {
        assert_eq!(line_ranges(&[2], 24), "2");
        assert_eq!(
            line_ranges(&[711, 712, 713, 714, 715, 716, 717, 802], 24),
            "711-717,802"
        );
        let scattered = (1..=100).map(|n| n * 10).collect::<Vec<u32>>();
        assert_eq!(line_ranges(&scattered, 3), "10,20,30,+97 more");
    }
}

#[cfg(test)]
mod verification_tests {
    use super::*;
    use crate::tools::cancel::NeverCancel;

    fn hit(line: u32, value: &str) -> octocode_engine::types::RipgrepMatch {
        octocode_engine::types::RipgrepMatch {
            line,
            column: 0,
            value: value.into(),
            count: None,
            kind: None,
            score_hint: None,
            rank: None,
            original_chars: None,
        }
    }

    fn open(source: &std::path::Path) -> std::io::BufReader<std::fs::File> {
        std::io::BufReader::new(std::fs::File::open(source).expect("fixture"))
    }

    fn file_with(
        matches: Vec<octocode_engine::types::RipgrepMatch>,
    ) -> octocode_engine::types::RipgrepFile {
        octocode_engine::types::RipgrepFile {
            path: "source.txt".into(),
            match_count: matches.len() as u32,
            matches,
        }
    }

    #[test]
    fn a_late_hit_retains_only_its_neighborhood_with_exact_coordinates() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let filler = "émoji ✓ filler line that is not near the hit\n".repeat(60_000);
        std::fs::write(&source, format!("{filler}before\nthe hit ✓\nafter\ntail\n"))
            .expect("fixture");
        let hit_line = 60_002;
        let read = read_verification_lines(
            &mut open(&source),
            &[(hit_line - 1, hit_line + 1)],
            hit_line + 1,
            &NeverCancel,
        )
        .unwrap_or_else(|_| panic!("readable"));
        assert_eq!(read.lines_read, hit_line + 1);
        assert_eq!(read.retained.len(), 3);
        assert_eq!(read.retained[&hit_line], "the hit ✓");
        assert!(read.retained_bytes < 64, "{}", read.retained_bytes);
        assert!(read.key_ranges.is_empty());
    }

    #[test]
    fn streamed_key_state_matches_the_whole_file_scan() {
        let body = "MIIEpQIBAAKCAQEAinteriorKeyBodyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        for content in [
            format!(
                "a\n-----BEGIN RSA PRIVATE KEY-----\n{body}\n-----END RSA PRIVATE KEY-----\nb\n"
            ),
            format!("a\n-----BEGIN OPENSSH PRIVATE KEY-----\n{body}\n{body}\n"),
            format!(
                "{}\n-----BEGIN EC PRIVATE KEY-----\n{body}\r\n-----END EC PRIVATE KEY-----\r\n",
                "x".repeat(200_000)
            ),
        ] {
            let dir = tempfile::tempdir().expect("fixture");
            let source = dir.path().join("key.txt");
            std::fs::write(&source, &content).expect("fixture");
            let read = read_verification_lines(&mut open(&source), &[], usize::MAX, &NeverCancel)
                .unwrap_or_else(|_| panic!("readable"));
            assert!(read.retained.is_empty());
            assert!(!read.unclassified);
            assert_eq!(
                read.key_ranges,
                crate::security::private_key_block_line_ranges(&content)
            );
        }
    }

    #[test]
    fn an_unbufferable_line_mentioning_a_private_key_fails_closed() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let giant = format!("{}PRIVATE KEY{}", " ".repeat(200_000), "-".repeat(10));
        std::fs::write(
            &source,
            format!("{giant}\nspacer\nspacer\nspacer\nMIIEpQIBAAKCAQEAinteriorKeyBody\n"),
        )
        .expect("fixture");
        let mut file = file_with(vec![hit(5, "…interiorKeyBo")]);
        let verified = guard_clipped_secrets(
            &mut file,
            &source,
            None,
            0..10,
            &ContentSecurity::new(),
            true,
            &NeverCancel,
        )
        .expect("not cancelled");
        assert_eq!(verified, Verification::Unverified);
        assert_eq!(file.matches[0].value, UNVERIFIED_PLACEHOLDER);
        // The same long line without a key mention is an ordinary line.
        std::fs::write(
            &source,
            format!(
                "{}\nspacer\nspacer\nspacer\nplain text here\n",
                " ".repeat(200_000)
            ),
        )
        .expect("fixture");
        let mut file = file_with(vec![hit(5, "plain text")]);
        assert_eq!(
            guard_clipped_secrets(
                &mut file,
                &source,
                None,
                0..10,
                &ContentSecurity::new(),
                true,
                &NeverCancel
            )
            .expect("not cancelled"),
            Verification::Verified
        );
        assert_eq!(file.matches[0].value, "plain text");
    }

    /// Values from a stored scan are checked only against the bytes the scan
    /// was stored with; other bytes, or no stored digest, mean `Changed`.
    #[test]
    fn a_stored_digest_binds_the_check_to_the_stored_bytes() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let stored = "one\nthe hit\nthree\n";
        std::fs::write(&source, stored).expect("fixture");
        let digest: super::super::manifest::Digest = Sha256::digest(stored.as_bytes()).into();
        let check = |expected| {
            let mut file = file_with(vec![hit(2, "the hit")]);
            let outcome = guard_clipped_secrets(
                &mut file,
                &source,
                expected,
                0..10,
                &ContentSecurity::new(),
                false,
                &NeverCancel,
            )
            .expect("not cancelled");
            (outcome, file.matches[0].value.clone())
        };
        assert_eq!(
            check(Some(Some(digest))),
            (Verification::Verified, "the hit".to_owned())
        );
        assert_eq!(check(Some(None)).0, Verification::Changed);
        std::fs::write(&source, "one\nthe hit\nthre3\n").expect("fixture");
        assert_eq!(check(Some(Some(digest))).0, Verification::Changed);
        assert_eq!(check(None).0, Verification::Verified);
    }

    #[test]
    fn a_neighborhood_over_the_retained_budget_fails_closed() {
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        let giant = "a".repeat(MAX_VERIFY_RETAINED_BYTES + 1);
        std::fs::write(&source, format!("{giant}\n")).expect("fixture");
        let mut file = file_with(vec![hit(1, "…aaaa…")]);
        let verified = guard_clipped_secrets(
            &mut file,
            &source,
            None,
            0..10,
            &ContentSecurity::new(),
            false,
            &NeverCancel,
        )
        .expect("not cancelled");
        assert_eq!(verified, Verification::Unverified);
        assert_eq!(file.matches[0].value, OVERSIZED_PLACEHOLDER);
    }

    #[test]
    fn verification_stops_when_cancelled() {
        struct Cancelled;
        impl CancellationCheck for Cancelled {
            fn check(&self) -> Result<(), String> {
                Err("Cancelled".into())
            }
        }
        let dir = tempfile::tempdir().expect("fixture");
        let source = dir.path().join("source.txt");
        std::fs::write(&source, "line\n".repeat(VERIFY_CANCEL_EVERY * 2)).expect("fixture");
        let mut file = file_with(vec![hit((VERIFY_CANCEL_EVERY * 2) as u32, "line")]);
        let reason = guard_clipped_secrets(
            &mut file,
            &source,
            None,
            0..10,
            &ContentSecurity::new(),
            false,
            &Cancelled,
        )
        .expect_err("cancelled during the stream");
        assert_eq!(reason, "Cancelled");
    }
}
