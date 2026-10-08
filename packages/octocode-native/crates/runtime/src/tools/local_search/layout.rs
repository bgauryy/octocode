//! Page layout: how a scan is cut into pages (grid or streamed), what a
//! page costs, and the reads that expand clipped values.

use super::executor::*;
use super::types::*;
use super::{cursor::*, leads::*, rows::*};
use crate::policy::path::PathPolicy;
use crate::tools::id::ToolId;
use crate::tools::local_fetch::MAX_READ_RANGES;
use crate::tools::result::Continuation;
use serde_json::{Value, json};

/// Whether a view lists paths (with counts) instead of match rows.
pub(super) fn list_view(view: LocalSearchQueryResultView) -> bool {
    matches!(
        view,
        LocalSearchQueryResultView::Files
            | LocalSearchQueryResultView::FilesWithout
            | LocalSearchQueryResultView::CountLines
            | LocalSearchQueryResultView::CountMatches
    )
}

/// How one page is cut and shown.
pub(super) struct Layout {
    pub(super) list: bool,
    pub(super) shown: PageRows,
    pub(super) total_pages: u32,
    pub(super) files_per_page: Option<u32>,
    pub(super) matches_per: u32,
    pub(super) out_of_range: bool,
    /// Context lines around each row (0 for matchOnly).
    pub(super) context_lines: u32,
    pub(super) streamed: bool,
    /// The response budget's per-row value share, when rows are shown.
    pub(super) budget_cap: Option<usize>,
    /// The tighter of the matchOnly display bound and `budget_cap`.
    pub(super) display_cap: Option<usize>,
    /// The budget is tighter than the caller's own per-match limit.
    pub(super) budget_binds: bool,
    /// Context windows of nearby rows merge into one block.
    pub(super) merge_context: Option<u32>,
}

impl Layout {
    /// Cut the page. A result within the page budget shows every hit on one
    /// page: paging metadata and continuations would cost more than the rows
    /// they hide. A caller cap always wins; larger results keep the
    /// 10-rows-per-file default on their first page.
    pub(super) fn cut(
        query: &LocalSearchQuery,
        paths: &PathPolicy,
        output_root: &std::path::Path,
        parsed: &mut octocode_engine::types::RipgrepParseResult,
        page_budget: usize,
    ) -> Self {
        let view = query.result_view;
        let list = list_view(view);
        let match_only = view == LocalSearchQueryResultView::MatchOnly;
        let context_lines = query
            .context_lines()
            .unwrap_or_else(|| default_context_lines(view));
        let costs = PageCosts::new(query, paths, output_root, context_lines);
        let hits_total: usize = if list {
            0
        } else {
            parsed.files.iter().map(|file| file.matches.len()).sum()
        };
        let show_all = !list
            && query.match_page_size().is_none()
            && fits_within(&parsed.files, page_budget, |file| {
                costs.file(file) + file.matches.iter().map(|m| costs.row(m)).sum::<usize>()
            });
        let matches_per = query
            .match_page_size()
            .unwrap_or(if show_all {
                u32::try_from(hits_total).unwrap_or(u32::MAX)
            } else {
                DEFAULT_MAX_MATCHES_PER_FILE
            })
            .max(1);
        // A file with more hits than one match page shows its deciding rows
        // first: declarations, then assignments/branches/returns, then other
        // code, then comments and strings (stable by line within a rank). A
        // row repeating an earlier row's text adds nothing and follows every
        // distinct row. Pages partition that order; each page is shown in
        // source order.
        if !match_only {
            for file in &mut parsed.files {
                if file.matches.len() as u32 > matches_per {
                    rank_file_rows(&mut file.matches);
                }
            }
        }
        let streamed = streamed_layout(query);
        let (shown, total_pages, files_per_page) = if streamed {
            Self::stream(
                query,
                &parsed.files,
                matches_per,
                show_all,
                page_budget,
                &costs,
            )
        } else {
            Self::grid(query, parsed.files.len(), matches_per, show_all)
        };
        let total_files = parsed.files.len() as u32;
        let shown_total: usize = shown
            .iter()
            .map(|(index, rows)| {
                let len = parsed.files[*index].matches.len();
                rows.end.min(len).saturating_sub(rows.start.min(len))
            })
            .sum();
        // Distribute the response value-char budget across the matches shown
        // on this page: a giant match is clipped (flagged `truncated`) rather
        // than dropped, so the page/match cursors and the line anchor +
        // localFetch cover full retrieval unchanged.
        let budget_cap = (shown_total > 0)
            .then(|| (RESPONSE_VALUE_CHAR_BUDGET / shown_total).max(MIN_MATCH_VALUE_CHARS));
        let match_only_limit = costs.match_only_limit;
        let display_cap = match (match_only_limit, budget_cap) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, b) => b,
        };
        let budget_binds = budget_cap.is_some_and(|cap| {
            cap < effective_match_content_length(query) as usize && display_cap == Some(cap)
        });
        // Content views emit a ±contextLines window per match row; windows of
        // nearby rows overlap, so merge them into one block per run of lines.
        // matchOnly carries exact spans and multiline rows span several
        // lines, so neither has per-row windows to merge.
        let merge_context = (!list
            && !match_only
            && query.multiline == LocalSearchQueryMultiline::Off
            && context_lines > 0)
            .then_some(context_lines);
        Self {
            list,
            out_of_range: total_files > 0 && query.page().max(1) > total_pages,
            shown,
            total_pages,
            files_per_page,
            matches_per,
            context_lines: if match_only { 0 } else { context_lines },
            streamed,
            budget_cap,
            display_cap,
            budget_binds,
            merge_context,
        }
    }

    /// Default layout: pages are cut from one row stream by the response
    /// budget, so a walk never pages a hot file ten rows at a time.
    pub(super) fn stream(
        query: &LocalSearchQuery,
        files: &[octocode_engine::types::RipgrepFile],
        matches_per: u32,
        show_all: bool,
        page_budget: usize,
        costs: &PageCosts,
    ) -> (PageRows, u32, Option<u32>) {
        let mut pages = stream_pages(
            files,
            matches_per as usize,
            show_all,
            page_budget,
            StreamCosts {
                file: &|file| costs.file(file),
                row: &|row| costs.row(row),
            },
        );
        let total_pages = u32::try_from(pages.len()).unwrap_or(u32::MAX).max(1);
        let page = query.page().max(1) as usize;
        let shown = if page <= pages.len() {
            pages.swap_remove(page - 1)
        } else {
            Vec::new()
        };
        (shown, total_pages, None)
    }

    /// A caller layout keeps the file-page × match-page grid.
    pub(super) fn grid(
        query: &LocalSearchQuery,
        file_count: usize,
        matches_per: u32,
        show_all: bool,
    ) -> (PageRows, u32, Option<u32>) {
        let view = query.result_view;
        let page_size = query
            .page_size()
            .unwrap_or_else(|| {
                if show_all {
                    default_page_size(view).max(u32::try_from(file_count).unwrap_or(u32::MAX))
                } else {
                    default_page_size(view)
                }
            })
            .max(1);
        let page = query.page().max(1);
        let start = (page - 1).saturating_mul(page_size) as usize;
        let end = start.saturating_add(page_size as usize).min(file_count);
        let skip = (query.match_page().max(1) - 1).saturating_mul(matches_per) as usize;
        let shown = (start.min(end)..end)
            .map(|index| (index, skip..skip.saturating_add(matches_per as usize)))
            .collect::<PageRows>();
        let total_files = u32::try_from(file_count).unwrap_or(u32::MAX);
        (
            shown,
            total_files.div_ceil(page_size).max(1),
            Some(page_size),
        )
    }
}

/// Serialized sizes a page is cut by. Rows render with workspace-relative
/// paths, so a file entry costs its root prefix too; a clipped row rides
/// its file's `next.expandValues*` read (see [`expand_values`]): the page
/// pays for that read, so it still fits.
pub(super) struct PageCosts {
    pub(super) prefix_chars: usize,
    /// matchOnly rows show their exact span clipped to the display bound.
    pub(super) match_only_limit: Option<usize>,
    pub(super) match_only: bool,
    pub(super) expand_context: u32,
    pub(super) read_chars: usize,
}

impl PageCosts {
    pub(super) fn new(
        query: &LocalSearchQuery,
        paths: &PathPolicy,
        output_root: &std::path::Path,
        context_lines: u32,
    ) -> Self {
        let match_only = query.result_view == LocalSearchQueryResultView::MatchOnly;
        let prefix_chars = match paths.workspace_relative(output_root).as_deref() {
            Some(".") => 0,
            Some(relative) => crate::tools::stream_page::json_text_chars(relative) + 1,
            None => crate::tools::stream_page::json_text_chars(&output_root.to_string_lossy()) + 1,
        };
        Self {
            prefix_chars,
            match_only_limit: match_only.then_some(effective_match_content_length(query) as usize),
            match_only,
            expand_context: if match_only { 0 } else { context_lines },
            read_chars: expansion_read_chars(
                query,
                query.multiline != LocalSearchQueryMultiline::Off,
            ),
        }
    }

    pub(super) fn clipped(&self, matched: &octocode_engine::types::RipgrepMatch) -> bool {
        match self.match_only_limit {
            Some(limit) => matched.value.chars().nth(limit).is_some(),
            None => matched.original_chars.is_some(),
        }
    }

    pub(super) fn file(&self, file: &octocode_engine::types::RipgrepFile) -> usize {
        let path = crate::tools::stream_page::json_text_chars(&file.path) + self.prefix_chars;
        let entry = path + FILE_ENTRY_CHARS;
        if file.matches.iter().any(|m| self.clipped(m)) {
            entry + self.read_chars + path
        } else {
            entry
        }
    }

    pub(super) fn row(&self, matched: &octocode_engine::types::RipgrepMatch) -> usize {
        let row = row_chars(matched, self.match_only_limit, self.match_only);
        if self.clipped(matched) {
            // Its line range, and a share of the extra read every
            // `MAX_READ_RANGES` ranges open.
            let digits = |n: u32| n.checked_ilog10().map_or(1, |log| log as usize + 1);
            row + 2 * digits(matched.line.saturating_add(self.expand_context))
                + 4
                + self.read_chars.div_ceil(MAX_READ_RANGES)
        } else {
            row
        }
    }
}

/// Per-file paging, reported only while it routes somewhere: more rows
/// remain, or the requested match page is past the end.
pub(super) fn file_pagination(
    layout: &Layout,
    total: u32,
    start: usize,
    end: usize,
    later: &[u32],
    match_page: u32,
) -> Option<ItemPagination> {
    if layout.list {
        return None;
    }
    if layout.streamed {
        return (end < total as usize).then(|| {
            let (lines, unlisted) = crate::tools::line_spans::more_lines(later, MAX_MORE_LINES);
            ItemPagination {
                current_page: None,
                total_pages: None,
                total_matches: total,
                has_more: true,
                more_lines: Some(lines),
                more_lines_unlisted: unlisted,
                out_of_range: false,
            }
        });
    }
    let total_pages = total.div_ceil(layout.matches_per).max(1);
    let has_more = match_page < total_pages;
    let out_of_range = start >= total as usize && total > 0;
    let (more_lines, more_lines_unlisted) = if has_more && !later.is_empty() {
        let (lines, unlisted) = crate::tools::line_spans::more_lines(later, MAX_MORE_LINES);
        (Some(lines), unlisted)
    } else {
        (None, None)
    };
    (has_more || out_of_range).then_some(ItemPagination {
        current_page: Some(match_page),
        total_pages: Some(total_pages),
        total_matches: total,
        has_more,
        more_lines,
        more_lines_unlisted,
        out_of_range,
    })
}

/// Serialized chars of one file entry around its rows and path,
/// `{"path":"","matches":[]},`.
pub(super) const FILE_ENTRY_CHARS: usize = 25;

/// Serialized chars of one match row as a page shows it, with its separator.
pub(super) fn row_chars(
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
/// are named as at most [`MAX_MORE_LINES`] lines plus a count.
pub(super) fn pagination_chars(file: &octocode_engine::types::RipgrepFile) -> usize {
    let digits = |n: u64| n.checked_ilog10().map_or(1, |log| log as usize + 1);
    let total = file.matches.len();
    let widest_line = file.matches.iter().map(|m| m.line).max().unwrap_or(0);
    let listed = total.saturating_sub(1).min(MAX_MORE_LINES);
    let more_lines = listed * (digits(u64::from(widest_line)) + 1)
        + ",\"moreLinesUnlisted\":".len()
        + digits(total as u64);
    let empty = ItemPagination {
        current_page: None,
        total_pages: None,
        total_matches: u32::try_from(total).unwrap_or(u32::MAX),
        has_more: true,
        more_lines: Some(Vec::new()),
        more_lines_unlisted: None,
        out_of_range: false,
    };
    ",\"pagination\":".len() + crate::tools::stream_page::json_chars(&empty) + more_lines
}

/// Whether every file entry, with all its rows, fits `budget` chars.
pub(super) fn fits_within(
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
pub(super) struct StreamCosts<'a> {
    /// A file entry around its rows.
    pub(super) file: &'a dyn Fn(&octocode_engine::types::RipgrepFile) -> usize,
    /// One match row.
    pub(super) row: &'a dyn Fn(&octocode_engine::types::RipgrepMatch) -> usize,
}

/// Rows one page shows: (file index, rank-order row range), in page order.
pub(super) type PageRows = Vec<(usize, std::ops::Range<usize>)>;

/// The default layout: neither page axis is caller-sized and no later match
/// page is asked for. Path-only list views keep file pages.
pub(super) fn streamed_layout(q: &LocalSearchQuery) -> bool {
    !matches!(
        q.result_view,
        LocalSearchQueryResultView::Files
            | LocalSearchQueryResultView::FilesWithout
            | LocalSearchQueryResultView::CountLines
            | LocalSearchQueryResultView::CountMatches
    ) && q.page_size().is_none()
        && q.match_page_size().is_none()
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
pub(super) fn stream_pages(
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

/// The contract maximum of `matchContentLength`.
pub(super) const MAX_MATCH_CONTENT_LENGTH: usize = 100_000;

/// Serialized chars of one `expandValuesNN` read without its path (and its
/// ranges, which each row pays for).
pub(super) fn expansion_read_chars(query: &LocalSearchQuery, multiline: bool) -> usize {
    let read = if multiline {
        json!({"expandValues99": {
            "tool": ToolId::LocalSearch.as_str(),
            "query": {"queries": [normalized_query(query, true)]},
            "why": MULTILINE_READ_WHY,
            "confidence": "exact",
        }})
    } else {
        json!({"expandValues99": {
            "tool": ToolId::LocalFetch.as_str(),
            "query": {"queries": [{"path": "", "ranges": []}]},
            "why": LINES_READ_WHY,
            "confidence": "exact",
        }})
    };
    crate::tools::stream_page::json_chars(&read)
}

pub(super) const LINES_READ_WHY: &str = "Read the clipped values' source lines whole.";

pub(super) const MULTILINE_READ_WHY: &str =
    "Search this file alone with room for its clipped values.";

/// A caller-sized page: the same `page`/`matchPage` show the same rows at
/// any `matchContentLength`, so one widened query reaches every clipped
/// value while `value_cap` (the response budget's per-row share) still
/// holds them.
pub(super) struct GridPage {
    pub(super) value_cap: usize,
}

/// Reads that return every clipped value of a page whole. A grid page whose
/// longest clipped value fits both the contract maximum and the per-row
/// budget gets one widened copy of the query. Otherwise each file gets its
/// own read: a line row's value is its hit line with up to `context` lines
/// on each side, so a localFetch of those source lines (`ranges`, merged, at
/// most [`MAX_READ_RANGES`] per read) holds it whole, and localFetch pages a
/// long read itself. A clipped multiline row hides how many lines its match
/// spans, so its file is searched again alone with `matchContentLength`
/// raised to its longest clipped value (past the contract maximum, a
/// localFetch from its first line).
pub(super) fn expand_values(
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
        return vec![
            Continuation::new(ToolId::LocalSearch, widened)
                .why("The same page with room for every clipped value.")
                .confidence("exact")
                .build(),
        ];
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
                reads.push(
                    Continuation::new(
                        ToolId::LocalFetch,
                        json!({"path": path, "ranges": [format!("{}-{}", first.saturating_sub(context).max(1), READ_TO_END_LINE)]}),
                    )
                    .why("Read from the clipped multiline value on; the read pages.")
                    .confidence("exact")
                    .build(),
                );
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
            reads.push(
                Continuation::new(ToolId::LocalSearch, search)
                    .why(MULTILINE_READ_WHY)
                    .confidence("exact")
                    .build(),
            );
            continue;
        }
        let merged = hit_windows(clipped.iter().map(|matched| matched.line), context);
        for chunk in merged.chunks(MAX_READ_RANGES) {
            let ranges = chunk
                .iter()
                .map(|(start, end)| format!("{start}-{end}"))
                .collect::<Vec<_>>();
            reads.push(
                Continuation::new(ToolId::LocalFetch, json!({"path": path, "ranges": ranges}))
                    .why(LINES_READ_WHY)
                    .confidence("exact")
                    .build(),
            );
        }
    }
    reads
}

/// Line runs `moreLines` names before it summarizes the rest as a count.
pub(super) const MAX_MORE_LINES: usize = 24;
