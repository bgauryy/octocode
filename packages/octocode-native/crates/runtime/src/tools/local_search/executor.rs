use super::types::*;
use crate::policy::discovery::{
    DISCOVERY_IGNORED_FILE_EXTENSIONS, DISCOVERY_IGNORED_FILE_NAMES, DISCOVERY_IGNORED_FOLDER_NAMES,
};
use crate::policy::path::PathPolicy;
use crate::security::ContentSecurity;
use crate::tools::local_fetch::CancellationCheck;
use octocode_engine::{
    portable::{RipgrepPathFilter, search_ripgrep_filtered},
    types::RipgrepSearchOptions,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Upper bound (bytes) on a file re-read for the SEC-1 private-key block scan.
/// Matches the secret scanner's own content cap; larger files fall back to the
/// per-match window sanitizer rather than pay an unbounded read.
const MAX_KEY_SCAN_BYTES: u64 = 10 * 1024 * 1024;

/// OUT-1: soft budget (in value chars) for a single localSearch response body.
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
const DEFAULT_MAX_MATCHES_PER_FILE: u32 = 10;
/// Files per page for snippet views; path-only list views stay at 100.
const DEFAULT_SNIPPET_PAGE_SIZE: u32 = 20;
const DEFAULT_LIST_PAGE_SIZE: u32 = 100;

pub fn execute_local_search(
    query: &LocalSearchRequest,
    paths: &PathPolicy,
    security: &ContentSecurity,
    cancel: &impl CancellationCheck,
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
    let view = query.result_view.unwrap_or_default();
    if query.match_window.is_some() && view != ResultView::MatchOnly {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "`matchWindow` requires resultView:\"matchOnly\"".into(),
            hints: vec![],
            next: None,
        });
    }
    if query.unique.unwrap_or_default() != UniqueMode::Off && view != ResultView::MatchOnly {
        return Err(LocalSearchError {
            code: "invalidQuery",
            message: "`unique` requires resultView:\"matchOnly\"".into(),
            hints: vec![],
            next: None,
        });
    }
    let validated = paths
        .validate(&query.path)
        .map_err(|error| LocalSearchError {
            code: "fileAccessFailed",
            message: error.message,
            hints: vec![],
            next: None,
        })?;
    let case = query.case_mode.unwrap_or_default();
    let regex = query.regex.unwrap_or_default();
    let multiline = query.multiline.unwrap_or_default();
    let requested_sort = query.sort.unwrap_or_default();
    let context_lines = query
        .context_lines
        .unwrap_or_else(|| default_context_lines(view));
    let path_sort = matches!(
        query.sort,
        Some(SortMode::Modified | SortMode::Accessed | SortMode::Created | SortMode::Path)
    );
    let options = RipgrepSearchOptions {
        path: validated.canonical.to_string_lossy().into_owned(),
        pattern: query.search_text.clone(),
        fixed_string: Some(regex == RegexMode::Literal),
        perl_regex: Some(regex == RegexMode::Pcre2),
        case_sensitive: Some(case == CaseMode::Sensitive),
        case_insensitive: Some(case == CaseMode::Insensitive),
        whole_word: query.whole_word,
        invert_match: query.invert_match,
        multiline: Some(multiline != MultilineMode::Off),
        multiline_dotall: Some(multiline == MultilineMode::Dotall),
        files_only: Some(view == ResultView::Files),
        files_without_match: Some(view == ResultView::FilesWithout),
        count_lines_per_file: Some(view == ResultView::CountLines),
        count_matches_per_file: Some(view == ResultView::CountMatches),
        context_lines: Some(context_lines),
        lang_type: query.lang_type.clone(),
        include: query.include.clone(),
        exclude: Some(
            query
                .exclude
                .clone()
                .unwrap_or_default()
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
            query
                .exclude_dir
                .clone()
                .unwrap_or_default()
                .into_iter()
                .chain(
                    DISCOVERY_IGNORED_FOLDER_NAMES
                        .iter()
                        .map(|s| (*s).to_owned()),
                )
                .collect(),
        ),
        no_ignore: query.no_ignore,
        hidden: query.hidden,
        max_depth: query.max_depth,
        sort: if requested_sort == SortMode::Traversal {
            Some("traversal".into())
        } else {
            Some(if path_sort {
                format!("{requested_sort:?}").to_lowercase()
            } else {
                "path".into()
            })
        },
        sort_reverse: query.reverse,
        max_snippet_chars: Some(
            query
                .match_content_length
                .unwrap_or(DEFAULT_MATCH_CONTENT_LENGTH),
        ),
        classify_matches: Some(false),
        only_matching: Some(view == ResultView::MatchOnly),
        match_window: query.match_window,
        unique: Some(matches!(
            query.unique,
            Some(UniqueMode::List | UniqueMode::Count)
        )),
        count_unique: Some(query.unique == Some(UniqueMode::Count)),
        // The engine collects every matching file, deterministically sorts, then
        // truncates to this cap (a stable path-sorted prefix — see
        // ripgrep_search::sort_and_cap). `rank_relevance` re-orders that retained
        // set afterwards, so "relevance" is relevance *within the first 10k
        // matched files*; a highly-relevant file beyond the path-prefix cap is
        // dropped before ranking sees it. Documented on the `sort` field so
        // callers narrow the search rather than expecting global ranking.
        max_collected_files: Some(10_000),
        // Use the engine default per-file byte ceiling (skips pathological
        // multi-GB files, surfaced as a maxFileSize diagnostic).
        max_file_bytes: None,
    };
    let frozen = query.no_ignore == Some(true);
    let (mut parsed, from_manifest) = if frozen
        && let Some(snapshot) = query.snapshot.as_deref()
        && let Some(stored) = super::manifest::get(snapshot)
    {
        (stored, true)
    } else {
        (
            search_ripgrep_filtered(options, Arc::new(PolicyFilter(paths.clone()))).map_err(|error| {
        let message=error.to_string(); let invalid=message.contains("regex parse error") || message.contains("PCRE2"); let bad_filter=message.contains("glob") || message.contains("unrecognized file type");
        let next=invalid.then(||{let mut repaired=normalized_query(query);repaired["regex"]=json!("literal");Box::new(json!({"repair":{"tool":"localSearch","query":repaired,"why":"Start a new search treating searchText as literal text, if that was intended."}}))});
        LocalSearchError{code:if invalid{"invalidRegex"}else if bad_filter{"invalidQuery"}else{"toolExecutionFailed"},message,hints:if invalid{vec!["Use regex:\"literal\" for exact text, or escape metacharacters/fix searchText to keep regex matching.".into()]}else{vec![]},next}
    })?,
            false,
        )
    };
    cancel.check().map_err(cancelled)?;
    for file in &parsed.files {
        paths
            .validate_read(&file.path)
            .map_err(|_| LocalSearchError {
                code: "fileAccessFailed",
                message: "Search encountered a path denied by the active path policy".into(),
                hints: vec![],
                next: None,
            })?;
    }
    let result_identity = fingerprint(query, &parsed.files, &parsed.stats);
    if frozen && !from_manifest {
        super::manifest::put(result_identity.clone(), parsed.clone());
    }
    if !from_manifest
        && query
            .snapshot
            .as_deref()
            .is_some_and(|expected| expected != result_identity)
    {
        let mut restart = normalized_query(query);
        if let Some(object) = restart.as_object_mut() {
            object.remove("snapshot");
        }
        restart["page"] = json!(1);
        restart["matchPage"] = json!(1);
        return Err(LocalSearchError {
            code: "staleSnapshot",
            message: "Search snapshot cannot be continued (resultsChanged); restart the search."
                .into(),
            hints: vec![],
            next: Some(Box::new(json!({
                "restart": {
                    "tool": "localSearch",
                    "query": restart,
                    "why": "Start a new search against the current source.",
                    "confidence": "exact"
                }
            }))),
        });
    }
    let root = &validated.canonical;
    let output_root = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root.as_path()
    };
    for file in &mut parsed.files {
        if let Ok(relative) = std::path::Path::new(&file.path).strip_prefix(output_root) {
            file.path = relative.to_string_lossy().into_owned();
        }
        let source_path = output_root.join(&file.path);
        // SEC-1: a match on an interior base64 body line of a private key would
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
            std::fs::metadata(&source_path)
                .ok()
                .filter(|meta| meta.len() <= MAX_KEY_SCAN_BYTES)
                .and_then(|_| std::fs::read(&source_path).ok())
                .map(|bytes| {
                    crate::security::private_key_block_line_ranges(&String::from_utf8_lossy(&bytes))
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for matched in &mut file.matches {
            cancel.check().map_err(cancelled)?;
            if !key_ranges.is_empty()
                && crate::security::match_window_intersects_key_block(
                    matched.line,
                    &matched.value,
                    &key_ranges,
                )
            {
                matched.value = crate::security::key_fragment_placeholder();
            } else {
                matched.value = security
                    .sanitize_text(&matched.value, Some(&source_path))
                    .content;
            }
        }
    }
    match requested_sort {
        SortMode::Path => parsed.files.sort_by(|a, b| a.path.cmp(&b.path)),
        SortMode::MatchCount => parsed.files.sort_by(|a, b| {
            b.match_count
                .cmp(&a.match_count)
                .then_with(|| a.path.cmp(&b.path))
        }),
        SortMode::Relevance => rank_relevance(&mut parsed.files, view),
        SortMode::Traversal => {}
        _ => {}
    }
    // Engine-side time sorts already honour `reverse`; every order the runtime
    // (re)establishes — path, matchCount, relevance (the default), traversal —
    // is reversed here, as the schema promises ("after sort, before pagination").
    if query.reverse.unwrap_or(false)
        && !matches!(
            requested_sort,
            SortMode::Modified | SortMode::Accessed | SortMode::Created
        )
    {
        parsed.files.reverse();
    }
    let page_size = query
        .page_size
        .unwrap_or_else(|| default_page_size(view))
        .max(1);
    let page = query.page.unwrap_or(1).max(1);
    let total_files = parsed.files.len() as u32;
    let total_pages = total_files.div_ceil(page_size).max(1);
    let start = (page - 1).saturating_mul(page_size) as usize;
    let list = matches!(
        view,
        ResultView::Files
            | ResultView::FilesWithout
            | ResultView::Discovery
            | ResultView::CountLines
            | ResultView::CountMatches
    );
    let matches_per = query
        .max_matches_per_file
        .unwrap_or(DEFAULT_MAX_MATCHES_PER_FILE)
        .max(1);
    let match_page = query.match_page.unwrap_or(1).max(1);
    let page_end = start
        .saturating_add(page_size as usize)
        .min(parsed.files.len());
    let page_range = start.min(page_end)..page_end;
    if !list {
        let match_skip = (match_page - 1).saturating_mul(matches_per) as usize;
        for file in &mut parsed.files[page_range.clone()] {
            cancel.check().map_err(cancelled)?;
            guard_clipped_secrets(
                file,
                &output_root.join(&file.path),
                match_skip..match_skip.saturating_add(matches_per as usize),
                security,
                view == ResultView::MatchOnly,
            );
        }
    }
    let total_matches = if list {
        parsed.stats.match_count.unwrap_or(0)
    } else {
        parsed.files.iter().map(|f| f.matches.len() as u32).sum()
    };
    let empty = total_files == 0;
    let snapshot = if !empty
        && (query.snapshot.is_some()
            || page < total_pages
            || parsed
                .files
                .iter()
                .any(|f| f.matches.len() as u32 > matches_per))
    {
        Some(result_identity.clone())
    } else {
        None
    };
    // Leftover rows only count on the files this page shows: another page's
    // files are reached by `nextPage` (which restarts at matchPage 1). List
    // views emit no match rows, so they have none left to page.
    let leftover_matches = !list
        && parsed.files[page_range.clone()]
            .iter()
            .any(|file| file.matches.len() as u32 > match_page.saturating_mul(matches_per));
    let next = build_next(
        query,
        page,
        total_pages,
        leftover_matches,
        match_page,
        snapshot.as_deref(),
    );
    // Keep full values for identity, unique grouping and counts. The engine's
    // match-only path emits exact spans, so apply the public display bound here.
    let match_only_limit = (view == ResultView::MatchOnly).then_some(
        query
            .match_content_length
            .unwrap_or(DEFAULT_MATCH_CONTENT_LENGTH) as usize,
    );
    // OUT-1: distribute the response value-char budget across the matches shown
    // on this page. `display_cap` is the tighter of the matchOnly display bound
    // and the budget-derived per-match cap; a giant match is clipped (flagged
    // `truncated`) rather than dropped, so the existing page/match cursors and
    // the returned line anchor + localFetch cover full retrieval unchanged.
    let shown_total: usize = parsed
        .files
        .iter()
        .skip(start)
        .take(page_size as usize)
        .map(|f| {
            let ms = (match_page - 1).saturating_mul(matches_per) as usize;
            f.matches.len().saturating_sub(ms).min(matches_per as usize)
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
        cap < query
            .match_content_length
            .unwrap_or(DEFAULT_MATCH_CONTENT_LENGTH) as usize
            && display_cap == Some(cap)
    });
    // Content views emit a ±contextLines window per match row; windows of
    // nearby rows overlap, so merge them into one block per run of lines.
    // matchOnly carries exact spans and multiline rows span several lines, so
    // neither has per-row windows to merge.
    let merge_context = (!list
        && view != ResultView::MatchOnly
        && multiline == MultilineMode::Off
        && context_lines > 0)
        .then_some(context_lines);
    let files = parsed
        .files
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .map(|f| {
            let total = f.matches.len() as u32;
            let ms = (match_page - 1).saturating_mul(matches_per) as usize;
            let shown = f
                .matches
                .iter()
                .skip(ms)
                .take(matches_per as usize)
                .map(|m| project_match(m, display_cap))
                .collect::<Vec<_>>();
            let shown = match merge_context {
                Some(context) => merge_context_windows(
                    shown,
                    context,
                    query
                        .match_content_length
                        .unwrap_or(DEFAULT_MATCH_CONTENT_LENGTH) as usize,
                ),
                None => shown,
            };
            let total_pages = total.div_ceil(matches_per).max(1);
            let has_more = match_page < total_pages;
            let out_of_range = ms >= total as usize && total > 0;
            SearchFile {
                path: f.path,
                matches: (!list).then_some(shown),
                total_occurrences: (view == ResultView::CountMatches).then_some(f.match_count),
                total_matched_lines: (view == ResultView::CountLines).then_some(f.match_count),
                // Per-file paging is only reported while it routes somewhere:
                // more match pages remain, or the requested page is past the end.
                pagination: (!list && (has_more || out_of_range)).then_some(ItemPagination {
                    current_page: match_page,
                    total_pages,
                    total_matches: total,
                    has_more,
                    next_match_page: has_more.then_some(match_page + 1),
                    out_of_range,
                }),
            }
        })
        .collect::<Vec<_>>();
    let stats = SearchStats {
        total_occurrences: parsed.stats.match_count.unwrap_or(0),
        matched_lines: parsed.stats.matched_lines.unwrap_or(0),
        files_matched: parsed.stats.files_matched.unwrap_or(total_files),
        files_searched: parsed.stats.files_searched.unwrap_or(0),
        bytes_searched: parsed.stats.bytes_searched,
        search_time: None,
        // Skipping binary files (rg's default) is normal coverage, not a cap a
        // continuation could lift; it stays visible as capReason only.
        capped: parsed.stats.capped.map(|capped| {
            capped
                && parsed
                    .stats
                    .cap_reason
                    .as_deref()
                    .is_none_or(|reason| reason.split(", ").any(|r| r != "binaryQuit"))
        }),
        cap_reason: parsed.stats.cap_reason,
        error_count: parsed.stats.error_count.filter(|n| *n > 0),
        first_error: parsed.stats.first_error,
    };
    // A PCRE2 search that blew past its wall-clock deadline is reported by the
    // engine as capped with cap_reason "pcre2Deadline" (see ripgrep_search.rs).
    // Surface it explicitly so callers know the results are a timeout-truncated
    // partial, not an exhaustive search.
    let mut warnings = vec![];
    let any_truncated = files.iter().any(|file| {
        file.matches
            .as_ref()
            .is_some_and(|matches| matches.iter().any(|matched| matched.truncated))
    });
    if budget_binds && any_truncated {
        warnings.push(
            "Match values were shortened to keep the total response within its size budget. Every match row and its line anchor is preserved; narrow the search (maxMatchesPerFile, matchContentLength, include/exclude) or use localFetch at each anchor for full source.".into(),
        );
    } else if any_truncated {
        warnings.push(
            "Some match values were truncated to matchContentLength; originalChars and returnedChars describe each shortened value. Counts and row pagination are unchanged. Use localFetch at the returned path/line anchors for full source.".into(),
        );
    }
    if stats
        .cap_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("pcre2Deadline"))
    {
        warnings.push(
            "The PCRE2 (regex:\"pcre2\") search hit its wall-clock deadline and was stopped; results are partial. Narrow the pattern/scope, or use regex:\"literal\" or the default engine.".into(),
        );
    }
    let has_more = page < total_pages;
    let capped = stats.capped.unwrap_or(false);
    let (status, terminal_limit) = classify_search(
        empty,
        capped,
        has_more,
        leftover_matches,
        stats.error_count.unwrap_or(0),
        files.len(),
        next.is_none(),
    );
    let skip_hint = skipped_target_hint(
        root.is_file(),
        stats.files_searched,
        stats.cap_reason.as_deref(),
    );
    Ok(LocalSearchResult {
        status,
        stats,
        files,
        // File paging is only reported when it routes somewhere: more file pages,
        // or a requested page past the end. Single-page totals live in `stats`,
        // and match-row continuations carry the snapshot in `next.*.query`.
        pagination: (!empty && (total_pages > 1 || start >= total_files as usize)).then_some(
            FilePagination {
                snapshot: snapshot.clone(),
                current_page: page,
                total_pages,
                files_per_page: page_size,
                total_files,
                total_matches: (!matches!(
                    view,
                    ResultView::Files | ResultView::FilesWithout | ResultView::Discovery
                ))
                .then_some(total_matches),
                has_more,
                // Hard ceiling: never advertise a next page past page 1000. Beyond
                // this, deep file pagination is refused by contract (matched in
                // `build_next`) — narrow the search rather than paging indefinitely.
                next_page: (page < total_pages && page < 1000).then_some(page + 1),
                out_of_range: start >= total_files as usize && total_files > 0,
            },
        ),
        hints: if empty {
            match skip_hint {
                Some(hint) => vec![hint],
                None => vec![empty_hint(query)],
            }
        } else {
            vec![]
        },
        next,
        terminal_limit,
        warnings,
        source_snapshot: Some(result_identity),
        source_root: output_root.to_path_buf(),
    })
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
            "The target file is binary (NUL byte found); it was not searched past that point. Use localFetch to inspect it."
                .into(),
        );
    }
    (single_file && files_searched == 0).then(|| {
        format!(
            "The target file was skipped ({reason}): nothing was searched. Raise limits or read it with localFetch chunks."
        )
    })
}

fn empty_hint(query: &LocalSearchRequest) -> String {
    let mut tips = Vec::new();
    if query.case_mode != Some(CaseMode::Insensitive) {
        tips.push("caseMode:\"insensitive\"");
    }
    tips.push("a shorter term");
    if query.regex == Some(RegexMode::Literal) {
        tips.push("regex:\"rust\"");
    } else {
        tips.push("regex:\"literal\" if searchText has metacharacters");
    }
    format!("No matches. Try {}.", tips.join(", "))
}

/// Remove `…` window markers and `...` truncation suffixes from a value line.
fn strip_clip_markers(line: &str) -> &str {
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
/// Private-key blocks are detected from the file prefix, not a snippet heuristic.
fn guard_clipped_secrets(
    file: &mut octocode_engine::types::RipgrepFile,
    source: &std::path::Path,
    shown: std::ops::Range<usize>,
    security: &ContentSecurity,
    match_only: bool,
) {
    use std::io::BufRead;
    let end = shown.end.min(file.matches.len());
    let start = shown.start.min(end);
    let shown = &mut file.matches[start..end];
    let Some(last_line) = shown
        .iter()
        .map(|m| m.line as usize + m.value.lines().count().max(1))
        .max()
    else {
        return;
    };
    let Ok(handle) = std::fs::File::open(source) else {
        return;
    };
    let mut reader = std::io::BufReader::new(handle);
    let mut lines: Vec<String> = Vec::new();
    let mut buf = Vec::new();
    while lines.len() < last_line {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buf);
                lines.push(text.trim_end_matches(['\n', '\r']).to_owned());
            }
        }
    }
    let key_ranges = crate::security::private_key_block_line_ranges(&lines.join("\n"));
    for matched in shown {
        if !key_ranges.is_empty()
            && crate::security::match_window_intersects_key_block(
                matched.line,
                &matched.value,
                &key_ranges,
            )
        {
            matched.value = crate::security::key_fragment_placeholder();
            continue;
        }
        let span = matched.value.lines().count().max(1);
        let line = matched.line as usize;
        let lo = line.saturating_sub(span).max(1);
        let hi = (line + span).min(lines.len());
        if lo > hi || line == 0 || line > lines.len() {
            continue;
        }
        let window = lines[lo - 1..hi].join("\n");
        let sanitized = security.sanitize_text(&window, Some(source));
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
                    .sanitize_text(&lines[line - 1], Some(source))
                    .content
            };
        }
    }
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
            column: matched.column,
            value: matched.value[..byte].into(),
            match_lines: None,
            count: matched.count,
            truncated: true,
            original_chars: Some(matched.value.chars().count()),
            returned_chars: Some(chars),
        };
    }
    // Content-view path: the engine already clipped the assembled snippet to
    // maxSnippetChars and reported the pre-truncation length via `original_chars`.
    // Surface that as a truncation indicator too, not just for only-matching
    // spans (fix 7).
    let truncated = matched.original_chars.is_some();
    SearchMatch {
        line: matched.line,
        column: matched.column,
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
/// source line is emitted once. A merged block keeps the first row's
/// `line`/`column`, and `matchLines` lists every matched line it holds. Rows
/// merge only when both windows are plain and their shared lines are
/// byte-identical, so a clipped or redacted window is never spliced.
/// Merges overlapping windows while the joined block stays within
/// `max_chars` (`matchContentLength`); a block never exceeds what one match
/// could have returned.
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
    fn flush(block: Block) -> SearchMatch {
        let mut head = block.head;
        if block.match_lines.len() > 1 {
            head.value = block.lines.join("\n");
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
                && grown.saturating_sub(1) <= max_chars
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

struct PolicyFilter(PathPolicy);
impl RipgrepPathFilter for PolicyFilter {
    fn allows(&self, path: &std::path::Path, is_dir: bool) -> bool {
        if is_dir {
            self.0.validate(path).is_ok()
        } else {
            self.0.validate_read(path).is_ok()
        }
    }
}

fn cancelled(message: String) -> LocalSearchError {
    LocalSearchError {
        code: "cancelled",
        message,
        hints: vec![],
        next: None,
    }
}

/// Relevance ordering for the match-bearing views.
///
/// Deliberately heuristic-free and fully deterministic: files with more matches
/// rank higher, ties broken by ascending path (a total order, so page 1 never
/// varies run-to-run). The previous `score()` function layered path-string
/// boosts (`/src/`, `/test`, `/docs/`, `main.`, dotfiles), per-language
/// affinity, and per-view flips — including a `.contains("unicode")` tweak and a
/// `/test` bonus that only applied in `matchOnly`. Those constants were overfit
/// to specific benchmark fixtures and actively mis-ranked real repositories
/// (any repo with "unicode" in a path, or one whose signal lives in test files),
/// so they were removed. Match density is the one honest, repo-agnostic signal.
fn rank_relevance(files: &mut [octocode_engine::types::RipgrepFile], view: ResultView) {
    // Path-list views carry no per-file match-density signal — order by path.
    if matches!(
        view,
        ResultView::Files | ResultView::FilesWithout | ResultView::Discovery
    ) {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        return;
    }
    files.sort_by(|a, b| {
        b.match_count
            .cmp(&a.match_count)
            .then_with(|| a.path.cmp(&b.path))
    });
}

/// Default ±context window per match row: the hit line alone, except the
/// `detailed` view, which exists to show surrounding code.
fn default_context_lines(view: ResultView) -> u32 {
    if view == ResultView::Detailed { 3 } else { 0 }
}

/// Default files per page: snippet views stay lean; path-only views are cheap.
fn default_page_size(view: ResultView) -> u32 {
    match view {
        ResultView::Files
        | ResultView::FilesWithout
        | ResultView::Discovery
        | ResultView::CountLines
        | ResultView::CountMatches => DEFAULT_LIST_PAGE_SIZE,
        _ => DEFAULT_SNIPPET_PAGE_SIZE,
    }
}

/// Views whose match rows carry a context window (so `contextLines` matters).
fn uses_context(view: ResultView) -> bool {
    matches!(
        view,
        ResultView::Paginated | ResultView::Content | ResultView::Detailed
    )
}

fn normalized_query(q: &LocalSearchRequest) -> Value {
    let mut value = serde_json::to_value(q).unwrap_or_else(|_| json!({}));
    // `LocalSearchRequest` serializes to a JSON object.
    #[allow(clippy::expect_used)]
    let o = value.as_object_mut().expect("request object");
    o.retain(|_, v| !v.is_null());
    o.entry("regex").or_insert(json!("rust"));
    o.entry("caseMode").or_insert(json!("smart"));
    let view = q.result_view.unwrap_or_default();
    if uses_context(view) {
        o.entry("contextLines")
            .or_insert(json!(default_context_lines(view)));
    }
    o.entry("matchContentLength")
        .or_insert(json!(DEFAULT_MATCH_CONTENT_LENGTH));
    o.entry("multiline").or_insert(json!("off"));
    o.entry("sort").or_insert(json!("relevance"));
    o.entry("unique").or_insert(json!("off"));
    o.entry("matchPage").or_insert(json!(1));
    o.entry("page").or_insert(json!(1));
    o.entry("resultView").or_insert(json!("paginated"));
    o.entry("pageSize")
        .or_insert(json!(default_page_size(view)));
    value
}
pub(crate) fn classify_search(
    empty: bool,
    capped: bool,
    has_more: bool,
    leftover_matches: bool,
    error_count: u32,
    files_returned: usize,
    next_missing: bool,
) -> (SearchStatus, bool) {
    if empty {
        return (SearchStatus::Empty, false);
    }
    let partial = capped || has_more || leftover_matches || (error_count > 0 && files_returned > 0);
    let status = if partial {
        SearchStatus::Partial
    } else {
        SearchStatus::Success
    };
    let terminal = (partial || has_more) && next_missing;
    (status, terminal)
}

fn build_next(
    q: &LocalSearchRequest,
    page: u32,
    total_pages: u32,
    leftover_matches: bool,
    match_page: u32,
    snapshot: Option<&str>,
) -> Option<Value> {
    let mut map = serde_json::Map::new();
    let base = normalized_query(q);
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
            json!({"tool":"localSearch","query":n,"confidence":"exact"}),
        );
    }
    if leftover_matches {
        let mut n = base;
        n["matchPage"] = json!(match_page + 1);
        if let Some(s) = snapshot {
            n["snapshot"] = json!(s)
        };
        map.insert(
            "nextMatchPage".into(),
            json!({"tool":"localSearch","query":n,"confidence":"exact"}),
        );
    }
    (!map.is_empty()).then_some(Value::Object(map))
}
fn fingerprint(
    q: &LocalSearchRequest,
    files: &[octocode_engine::types::RipgrepFile],
    stats: &octocode_engine::types::RipgrepStats,
) -> String {
    let mut identity = serde_json::Map::new();
    identity.insert("searchText".into(), json!(q.search_text));
    identity.insert(
        "mode".into(),
        json!(match q.result_view.unwrap_or_default() {
            ResultView::Discovery => "discovery",
            ResultView::Detailed => "detailed",
            _ => "paginated",
        }),
    );
    identity.insert(
        "regex".into(),
        json!(match q.regex.unwrap_or_default() {
            RegexMode::Literal => "fixed",
            RegexMode::Pcre2 => "perl",
            RegexMode::Rust => "smart",
        }),
    );
    identity.insert(
        "caseMode".into(),
        json!(match q.case_mode.unwrap_or_default() {
            CaseMode::Sensitive => "sensitive",
            CaseMode::Insensitive => "insensitive",
            CaseMode::Smart => "smart",
        }),
    );
    identity.insert(
        "contextLines".into(),
        json!(
            q.context_lines
                .unwrap_or_else(|| default_context_lines(q.result_view.unwrap_or_default()))
        ),
    );
    identity.insert(
        "matchContentLength".into(),
        json!(
            q.match_content_length
                .unwrap_or(DEFAULT_MATCH_CONTENT_LENGTH)
        ),
    );
    identity.insert(
        "multiline".into(),
        json!(match q.multiline.unwrap_or_default() {
            MultilineMode::Off => "off",
            MultilineMode::On => "on",
            MultilineMode::Dotall => "dotall",
        }),
    );
    identity.insert(
        "sort".into(),
        json!(match q.sort.unwrap_or_default() {
            SortMode::Relevance => "relevance",
            SortMode::Traversal => "traversal",
            SortMode::MatchCount => "matchCount",
            SortMode::Path => "path",
            SortMode::Modified => "modified",
            SortMode::Accessed => "accessed",
            SortMode::Created => "created",
        }),
    );
    identity.insert(
        "output".into(),
        json!(match q.result_view.unwrap_or_default() {
            ResultView::MatchOnly => "matchOnly",
            ResultView::Files => "files",
            ResultView::FilesWithout => "filesWithout",
            ResultView::CountLines => "countLines",
            ResultView::CountMatches => "countMatches",
            _ => "content",
        }),
    );
    identity.insert(
        "unique".into(),
        json!(match q.unique.unwrap_or_default() {
            UniqueMode::Off => "off",
            UniqueMode::List => "list",
            UniqueMode::Count => "count",
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
    opt!("include", q.include.as_ref());
    opt!("exclude", q.exclude.as_ref());
    opt!("excludeDir", q.exclude_dir.as_ref());
    opt!("noIgnore", q.no_ignore);
    opt!("hidden", q.hidden);
    opt!("maxDepth", q.max_depth);
    opt!("langType", q.lang_type.as_ref());
    opt!("matchWindow", q.match_window);
    opt!("sortReverse", q.reverse);
    let mut entries = identity.into_iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let query_key = hex::encode(Sha256::digest(
        serde_json::to_vec(&json!([q.path, entries])).unwrap_or_default(),
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
fn canonicalize(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries = map
                .into_iter()
                .filter(|(_, v)| !v.is_null())
                .collect::<Vec<_>>();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k, canonicalize(v)))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.into_iter().map(canonicalize).collect()),
        v => v,
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;

    fn row(line: u32, value: &str) -> SearchMatch {
        SearchMatch {
            line,
            column: 0,
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
        assert_eq!(merged[0].value, "l4\nl5\nl6\nl7");
        assert_eq!(merged[0].match_lines, Some(vec![5, 6]));
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
        let capped = merge_context_windows(vec![row(5, "l4\nl5\nl6"), row(6, "l5\nl6\nl7")], 1, 8);
        assert_eq!(
            capped.len(),
            2,
            "a merge must not exceed matchContentLength"
        );
    }
}
