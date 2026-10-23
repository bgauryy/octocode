//! Page layout: how a scan is cut into pages (grid or streamed), what a
//! page costs, and the reads that expand clipped values.

use super::local_search_query::{
    self, DEFAULT_MAX_MATCHES_PER_FILE, MIN_MATCH_VALUE_CHARS, RESPONSE_VALUE_CHAR_BUDGET,
    default_page_size, effective_match_content_length, normalized_query, shows_span_columns,
    streamed_layout,
};
use super::types::*;
use super::{enclosing::*, handoff::*, rows::*};
use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use crate::id::ToolId;
use crate::num::u32_of_usize;
use crate::policy::path::PathPolicy;
use crate::response::result::Continuation;
use crate::response::result::ToolError;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

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
    /// What the pages walk: hit rows on default content pages (a file rides
    /// several), files on list and grid pages.
    pub(super) total_items: u32,
    pub(super) matches_per: u32,
    pub(super) out_of_range: bool,
    /// Resolved context around each row (empty for matchOnly).
    pub(super) context: SearchContext,
    pub(super) streamed: bool,
    /// The response budget's per-row value share, when rows are shown.
    pub(super) budget_cap: Option<usize>,
    /// The tighter of the matchOnly display bound and `budget_cap`.
    pub(super) display_cap: Option<usize>,
    /// Context windows of nearby rows merge into one block.
    pub(super) merge_context: Option<SearchContext>,
    /// What each shown file's rendered path adds to its stored one: the
    /// root's workspace-relative prefix (see [`PageCosts`]).
    pub(super) prefix_chars: usize,
}

impl Layout {
    /// Cut the page. A result within the page budget and the default page
    /// size of files shows every hit on one page: paging metadata and
    /// continuations would cost more than the rows they hide. A page never
    /// holds more files than its page size, so its enclosing names cover it.
    /// A caller cap always wins; default pages fill their byte budget.
    /// `pricing` prices the rows' enclosing names (see [`PageCosts::name`]),
    /// outlining only the files the cut must charge exactly: every file of a
    /// result that may fit whole, else those of the first page and of the
    /// row that ends it (see [`Self::stream`]).
    pub(super) fn cut(
        query: &LocalSearchQuery,
        paths: &PathPolicy,
        output_root: &std::path::Path,
        parsed: &mut octocode_engine::types::TextSearchResult,
        page_budget: usize,
        pricing: &mut NamePricing<'_>,
    ) -> Result<Self, ToolError> {
        let view = query.result_view;
        let list = list_view(view);
        let match_only = view == LocalSearchQueryResultView::MatchOnly;
        let context = local_search_query::context(query);
        let mut costs = PageCosts::new(query, paths, output_root, context);
        costs.owners = pricing.priced.clone();
        let priced_names =
            costs.shows_names && local_search_query::match_page_size(query).is_none();
        let hits_total: usize = if list {
            0
        } else {
            parsed.files.iter().map(|file| file.matches.len()).sum()
        };
        // A name costs at least nothing: a result that does not fit whole
        // without its names never shows whole, and one that may is priced
        // whole (it is at most one page of files).
        let show_all = !list
            && local_search_query::match_page_size(query).is_none()
            && fits_whole(&parsed.files, page_budget, &costs, true)
            && {
                let all = (0..parsed.files.len()).collect::<Vec<_>>();
                pricing.price(&parsed.files, all)?;
                costs.owners = pricing.priced.clone();
                fits_whole(&parsed.files, page_budget, &costs, false)
            }
            && parsed
                .files
                .iter()
                .map(|file| expansion_rows(&file.matches, &costs))
                .sum::<usize>()
                <= max_expansion_rows();
        let matches_per = local_search_query::match_page_size(query)
            .unwrap_or(if show_all {
                u32_of_usize(hits_total)
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
        let (shown, total_pages, files_per_page, total_items) = if streamed && list {
            Self::list_stream(query, &parsed.files, page_budget, &costs)
        } else if streamed {
            let pricing = priced_names.then_some(pricing);
            Self::stream(
                query,
                &parsed.files,
                matches_per,
                show_all,
                page_budget,
                &mut costs,
                pricing,
            )?
        } else {
            Self::grid(query, parsed.files.len(), matches_per)
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
        // on this page: a giant match is clipped (reported by `originalChars`) rather
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
        // Content views emit a resolved context window per match row; windows of
        // nearby rows overlap, so merge them into one block per run of lines.
        // matchOnly carries exact spans, so it has no windows. A multiline
        // row spanning several lines is numbered as its own block.
        let merge_context = (!list && !match_only && !context.is_empty()).then_some(context);
        Ok(Self {
            list,
            out_of_range: total_files > 0 && local_search_query::page(query).max(1) > total_pages,
            shown,
            total_pages,
            files_per_page,
            total_items,
            matches_per,
            context: if match_only {
                SearchContext::default()
            } else {
                context
            },
            streamed,
            budget_cap,
            display_cap,
            merge_context,
            prefix_chars: costs.prefix_chars,
        })
    }

    /// Default layout: pages are cut from one row stream by the response
    /// budget, so a walk never pages a hot file ten rows at a time. With
    /// `pricing`, the first page is cut with the exact names of its rows
    /// and of the row that ends it: the files of the first page a cut with
    /// free names reaches are outlined; rows of every other file reserve.
    /// Which files are priced depends on the result alone, never on the
    /// requested page or on prices already known, so every call of a walk
    /// (fresh, rescanned, or cut from the stored scan and the prices its
    /// first page stored) cuts the same pages and counts the same total.
    pub(super) fn stream(
        query: &LocalSearchQuery,
        files: &[octocode_engine::types::TextSearchFile],
        matches_per: u32,
        show_all: bool,
        page_budget: usize,
        costs: &mut PageCosts,
        pricing: Option<&mut NamePricing<'_>>,
    ) -> Result<(PageRows, u32, Option<u32>, u32), ToolError> {
        // `through`: the cut stops after that page, every reserved name
        // costing nothing (see below).
        let cut = |costs: &PageCosts, through: Option<usize>| {
            stream_pages(
                files,
                matches_per as usize,
                show_all,
                page_budget,
                StreamCosts {
                    file: &|file| costs.file(file),
                    row: &|row| costs.row(row),
                    name: &|index, row| match costs.name(
                        index,
                        &files[index],
                        &files[index].matches[row],
                    ) {
                        RowName::Reserved if through.is_some() => RowName::None,
                        name => name,
                    },
                    read: &|row| costs.read(row),
                },
                through,
            )
        };
        // Free reserved names make the first page reach at least as far as
        // the exact cut, so pricing the files up to the row that ends it
        // prices every row that decides the exact first page: the second
        // round finds nothing left to outline.
        if let Some(pricing) = pricing.filter(|_| !show_all) {
            loop {
                let pages = cut(costs, Some(1));
                // The first page's rows, and the row that opens the second:
                // the one its cut stopped before.
                let deciding = pages
                    .iter()
                    .take(1)
                    .flatten()
                    .chain(pages.get(1).and_then(|next| next.first()))
                    .filter(|(_, rows)| !rows.is_empty())
                    .map(|(index, _)| *index)
                    .collect::<Vec<_>>();
                let before = pricing.priced.len();
                pricing.price(files, deciding)?;
                if pricing.priced.len() == before {
                    break;
                }
                costs.owners = pricing.priced.clone();
            }
        }
        let mut pages = cut(costs, None);
        let total_pages = u32_of_usize(pages.len()).max(1);
        let page = local_search_query::page(query).max(1) as usize;
        let shown = if page <= pages.len() {
            pages.swap_remove(page - 1)
        } else {
            Vec::new()
        };
        let rows: usize = files.iter().map(|file| file.matches.len()).sum();
        Ok((shown, total_pages, None, u32_of_usize(rows)))
    }

    fn list_stream(
        query: &LocalSearchQuery,
        files: &[octocode_engine::types::TextSearchFile],
        page_budget: usize,
        costs: &PageCosts,
    ) -> (PageRows, u32, Option<u32>, u32) {
        let sizes = files
            .iter()
            .map(|file| {
                let projected = SearchFile {
                    path: file.path.clone(),
                    matches: None,
                    match_keys: Vec::new(),
                    total_occurrences: (query.result_view
                        == LocalSearchQueryResultView::CountMatches)
                        .then_some(file.match_count),
                    total_matched_lines: (query.result_view
                        == LocalSearchQueryResultView::CountLines)
                        .then_some(file.match_count),
                    pagination: None,
                };
                octocode_engine::json_size::utf16_units(&projected).unwrap_or(0)
                    + costs.prefix_chars
                    + 1
            })
            .collect::<Vec<_>>();
        let cut = crate::response::page_windows::select(
            crate::response::page_windows::Paging::Chars(page_budget),
            &sizes,
            local_search_query::page(query) as usize,
        );
        (
            cut.range.map(|index| (index, 0..0)).collect(),
            u32_of_usize(cut.pages),
            None,
            u32_of_usize(files.len()),
        )
    }

    /// A caller layout keeps the file-page × match-page grid.
    pub(super) fn grid(
        query: &LocalSearchQuery,
        file_count: usize,
        matches_per: u32,
    ) -> (PageRows, u32, Option<u32>, u32) {
        let page_size = local_search_query::page_size(query)
            .unwrap_or_else(|| default_page_size(query.result_view))
            .max(1);
        let page = local_search_query::page(query).max(1);
        let window = crate::response::page_windows::count_window(
            page as usize,
            page_size as usize,
            file_count,
        );
        let skip =
            (local_search_query::match_page(query).max(1) - 1).saturating_mul(matches_per) as usize;
        let shown = window
            .map(|index| (index, skip..skip.saturating_add(matches_per as usize)))
            .collect::<PageRows>();
        let total_files = u32_of_usize(file_count);
        (
            shown,
            crate::response::page_windows::count_pages(file_count, page_size as usize) as u32,
            Some(page_size),
            total_files,
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
    /// Span rows carry `column` in the response (see [`shows_span_columns`]).
    pub(super) span_columns: bool,
    pub(super) expand_context: SearchContext,
    /// Multiline values are read once per file (see [`expansion_rows`]).
    pub(super) multiline: bool,
    pub(super) read_chars: usize,
    /// Rows are shown, so each hit row of an outlined language carries its
    /// `enclosing` name: the cut charges it (see [`PageCosts::name`]).
    pub(super) shows_names: bool,
    /// The names of each file the cut priced, by file index.
    owners: HashMap<usize, FilePrice>,
}

/// The enclosing names one file's hit rows carry, as a page cut prices
/// them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FilePrice {
    /// The owner of each hit line inside a declaration, by line (a line
    /// outside every declaration has none, as has every line of a file
    /// without an outline).
    Owners(Arc<HashMap<u32, Owner>>),
    /// The file could not be read for its outline: its rows reserve
    /// [`ENCLOSING_RESERVE_CHARS`] each.
    Unread,
}

impl FilePrice {
    /// The names of `file`'s hit rows by its `outline` (`None`: it has
    /// none). A row's owner is the one [`annotate_enclosing`] names it by,
    /// except that a declaration search names only its declaring rows (and
    /// a merged context block only its first line's owner): charging every
    /// hit row bounds those pages from above.
    pub(super) fn of(
        file: &octocode_engine::types::TextSearchFile,
        outline: Option<&Outline>,
    ) -> Self {
        Self::Owners(Arc::new(
            outline
                .map(|outline| {
                    file.matches
                        .iter()
                        .filter_map(|matched| {
                            let owner = outline.label(outline.owner(matched.line)?)?;
                            let full = ENCLOSING_FIELD_CHARS
                                + octocode_engine::json_size::utf16_units(&owner).unwrap_or(0);
                            Some((
                                matched.line,
                                Owner {
                                    line: owner.line,
                                    full,
                                },
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ))
    }

    /// Bytes this price keeps, as a stored scan weighs it.
    pub(super) fn weight(&self) -> usize {
        match self {
            Self::Owners(owners) => owners.len() * std::mem::size_of::<(u32, Owner)>(),
            Self::Unread => 0,
        }
    }
}

/// Outlines `files` (by index) for a page cut and prices their rows'
/// names: one [`FilePrice`] per file.
pub(super) type Outliner<'p> = dyn FnMut(
        &[(usize, &octocode_engine::types::TextSearchFile)],
    ) -> Result<Vec<(usize, FilePrice)>, ToolError>
    + 'p;

/// The enclosing names a page cut charges, priced on demand: a file is
/// outlined only once a cut must charge its rows exactly (see
/// [`Layout::cut`]); every other file of an outlined language reserves.
pub(super) struct NamePricing<'p> {
    /// The prices known so far, by file index: a stored scan's, then the
    /// ones this cut outlined.
    pub(super) priced: HashMap<usize, FilePrice>,
    outline: &'p mut Outliner<'p>,
}

impl<'p> NamePricing<'p> {
    pub(super) fn new(priced: HashMap<usize, FilePrice>, outline: &'p mut Outliner<'p>) -> Self {
        Self { priced, outline }
    }

    /// Price the files of `wanted` that hold rows in an outlined language
    /// and have no price yet.
    fn price(
        &mut self,
        files: &[octocode_engine::types::TextSearchFile],
        wanted: Vec<usize>,
    ) -> Result<(), ToolError> {
        let mut seen = HashSet::new();
        let unpriced = wanted
            .into_iter()
            .filter(|index| seen.insert(*index) && !self.priced.contains_key(index))
            .filter(|index| {
                let file = &files[*index];
                !file.matches.is_empty() && outlined_language(std::path::Path::new(&file.path))
            })
            .map(|index| (index, &files[index]))
            .collect::<Vec<_>>();
        if !unpriced.is_empty() {
            let mut outlined = (self.outline)(&unpriced)?
                .into_iter()
                .collect::<HashMap<_, _>>();
            for (index, _) in unpriced {
                // A file the outliner left out was not read.
                let price = outlined.remove(&index).unwrap_or(FilePrice::Unread);
                self.priced.insert(index, price);
            }
        }
        Ok(())
    }
}

/// A hit row's enclosing declaration as the page cut prices it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Owner {
    /// Its name line: what later rows inside it refer to.
    pub(super) line: u32,
    /// `,"enclosing":{…}`, on the first row of the page inside it.
    pub(super) full: usize,
}

impl Owner {
    /// `,"enclosingLine":N`, on each later row of the page inside it.
    pub(super) fn reference(self) -> usize {
        ENCLOSING_LINE_FIELD_CHARS + self.line.checked_ilog10().map_or(1, |log| log as usize + 1)
    }
}

/// What a hit row's enclosing name adds to a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowName {
    /// The row carries no name.
    None,
    /// A row of a file the cut has not outlined (or could not read):
    /// [`ENCLOSING_RESERVE_CHARS`].
    Reserved,
    /// The full name on the page's first row inside `Owner`, a reference on
    /// the rest.
    Owned(Owner),
}

/// Prices hit rows' enclosing names on one page: each declaration's first
/// row pays its full name, later rows a reference (as
/// [`annotate_enclosing`] names them).
#[derive(Default)]
pub(super) struct PageNames(std::collections::HashSet<(usize, u32)>);

impl PageNames {
    /// What `name`, on a row of file `index`, adds to this page.
    pub(super) fn price(&self, index: usize, name: RowName) -> usize {
        match name {
            RowName::None => 0,
            RowName::Reserved => ENCLOSING_RESERVE_CHARS,
            RowName::Owned(owner) if self.0.contains(&(index, owner.line)) => owner.reference(),
            RowName::Owned(owner) => owner.full,
        }
    }

    /// Put `name`, on a row of file `index`, on this page.
    pub(super) fn add(&mut self, index: usize, name: RowName) {
        if let RowName::Owned(owner) = name {
            self.0.insert((index, owner.line));
        }
    }
}

impl PageCosts {
    pub(super) fn new(
        query: &LocalSearchQuery,
        paths: &PathPolicy,
        output_root: &std::path::Path,
        context: SearchContext,
    ) -> Self {
        let match_only = query.result_view == LocalSearchQueryResultView::MatchOnly;
        let prefix_chars = match paths.workspace_relative(output_root).as_deref() {
            Some(".") => 0,
            Some(relative) => octocode_engine::json_size::string_body_units(relative) + 1,
            None => {
                octocode_engine::json_size::string_body_units(&output_root.to_string_lossy()) + 1
            }
        };
        Self {
            prefix_chars,
            match_only_limit: match_only.then_some(effective_match_content_length(query) as usize),
            span_columns: shows_span_columns(query),
            expand_context: if match_only {
                SearchContext::default()
            } else {
                context
            },
            multiline: query.multiline != LocalSearchQueryMultiline::Off,
            shows_names: !list_view(query.result_view),
            owners: HashMap::new(),
            read_chars: expansion_read_chars(
                query,
                query.multiline != LocalSearchQueryMultiline::Off,
            ),
        }
    }

    pub(super) fn clipped(&self, matched: &octocode_engine::types::TextSearchMatch) -> bool {
        match self.match_only_limit {
            Some(limit) => matched.value.chars().nth(limit).is_some(),
            None => matched.original_chars.is_some(),
        }
    }

    pub(super) fn file(&self, file: &octocode_engine::types::TextSearchFile) -> usize {
        let path = octocode_engine::json_size::string_body_units(&file.path) + self.prefix_chars;
        let entry = path + FILE_ENTRY_CHARS;
        if file.matches.iter().any(|m| self.clipped(m)) {
            entry + self.read_chars + path
        } else {
            entry
        }
    }

    /// The enclosing name a hit row of file `index` carries (priced by
    /// [`PageNames`]): its owner in a file the cut outlined, a reserve in
    /// another outlined-language file (unread, or not priced yet), else
    /// none.
    pub(super) fn name(
        &self,
        index: usize,
        file: &octocode_engine::types::TextSearchFile,
        matched: &octocode_engine::types::TextSearchMatch,
    ) -> RowName {
        if !self.shows_names {
            return RowName::None;
        }
        match self.owners.get(&index) {
            Some(FilePrice::Owners(owners)) => owners
                .get(&matched.line)
                .map_or(RowName::None, |owner| RowName::Owned(*owner)),
            Some(FilePrice::Unread) => RowName::Reserved,
            None if outlined_language(std::path::Path::new(&file.path)) => RowName::Reserved,
            None => RowName::None,
        }
    }

    pub(super) fn row(&self, matched: &octocode_engine::types::TextSearchMatch) -> usize {
        let row = row_chars(matched, self.match_only_limit, self.span_columns);
        if self.clipped(matched) {
            // Its line range, and a share of the extra read every
            // `RANGES_MAX_ITEMS` ranges open.
            let digits = |n: u32| n.checked_ilog10().map_or(1, |log| log as usize + 1);
            row + 2 * digits(matched.line.saturating_add(self.expand_context.after))
                + 4
                + self.read_chars.div_ceil(RANGES_MAX_ITEMS)
        } else {
            row
        }
    }
}

/// Per-file paging, reported only while it routes somewhere: more rows
/// remain, or the requested match page is past the end. `shown` counts the
/// rows the file entry carries.
pub(super) fn file_pagination(
    layout: &Layout,
    total: u32,
    (start, end, shown): (usize, usize, usize),
    later: &[u32],
    match_page: u32,
) -> Option<ItemPagination> {
    if layout.list {
        return None;
    }
    if layout.streamed {
        return (end < total as usize).then(|| {
            let (lines, unlisted) = crate::tools::line_spans::more_lines(later, MAX_MORE_LINES);
            let named = shown + lines.len() + unlisted.map_or(0, |n| n as usize);
            ItemPagination {
                current_page: None,
                total_pages: None,
                total_matches: (named != total as usize).then_some(total),
                has_more: true,
                more_lines: Some(lines),
                more_lines_unlisted: unlisted,
                out_of_range: false,
            }
        });
    }
    let total_pages = u32_of_usize(crate::response::page_windows::count_pages(
        total as usize,
        layout.matches_per as usize,
    ));
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
        total_matches: Some(total),
        has_more,
        more_lines,
        more_lines_unlisted,
        out_of_range,
    })
}

/// Serialized chars of one file entry around its rows and path.
pub(super) const FILE_ENTRY_CHARS: usize = "{\"path\":\"\",\"matches\":[]},".len();

/// Serialized chars of one match row as a page shows it, with its separator.
/// The value is priced in place, never copied.
pub(super) fn row_chars(
    matched: &octocode_engine::types::TextSearchMatch,
    display_cap: Option<usize>,
    span_columns: bool,
) -> usize {
    if let Some(price) = matched.deferred.filter(|price| *price > 0) {
        return price as usize;
    }
    let (mut row, value) = project_shape(matched, display_cap);
    if !span_columns {
        row.column = None;
    }
    octocode_engine::json_size::utf16_units(&row).unwrap_or(0)
        + octocode_engine::json_size::string_body_units(value)
        + 1
}

/// Most chars a streamed file entry's `pagination` can take: its later rows
/// are named as at most [`MAX_MORE_LINES`] lines plus a count.
pub(super) fn pagination_chars(file: &octocode_engine::types::TextSearchFile) -> usize {
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
        total_matches: Some(u32_of_usize(total)),
        has_more: true,
        more_lines: Some(Vec::new()),
        more_lines_unlisted: None,
        out_of_range: false,
    };
    ",\"pagination\":".len()
        + octocode_engine::json_size::utf16_units(&empty).unwrap_or(0)
        + more_lines
}

/// Whether every file entry (by index), with all its rows, fits `budget`
/// chars.
pub(super) fn fits_within(
    files: &[octocode_engine::types::TextSearchFile],
    budget: usize,
    mut entry_chars: impl FnMut(usize, &octocode_engine::types::TextSearchFile) -> usize,
) -> bool {
    let mut used = 0usize;
    files.iter().enumerate().all(|(index, file)| {
        used = used.saturating_add(entry_chars(index, file));
        used <= budget
    })
}

/// Whether every file entry, with all its rows and their names, fits
/// `budget` chars. With `unpriced_free`, a name not priced yet costs
/// nothing: a lower bound of the exact fit.
fn fits_whole(
    files: &[octocode_engine::types::TextSearchFile],
    budget: usize,
    costs: &PageCosts,
    unpriced_free: bool,
) -> bool {
    let mut names = PageNames::default();
    fits_within(files, budget, |index, file| {
        costs.file(file)
            + file
                .matches
                .iter()
                .map(|m| {
                    let name = match costs.name(index, file, m) {
                        RowName::Reserved if unpriced_free => RowName::None,
                        name => name,
                    };
                    let price = names.price(index, name);
                    names.add(index, name);
                    costs.row(m) + price
                })
                .sum::<usize>()
    })
}

/// Serialized sizes a streamed page is cut by.
pub(super) struct StreamCosts<'a> {
    /// A file entry around its rows.
    pub(super) file: &'a dyn Fn(&octocode_engine::types::TextSearchFile) -> usize,
    /// One match row.
    pub(super) row: &'a dyn Fn(&octocode_engine::types::TextSearchMatch) -> usize,
    /// The enclosing name a row (by file index and rank) carries.
    pub(super) name: &'a dyn Fn(usize, usize) -> RowName,
    /// What reading one row's clipped value takes (see [`FileReads`]).
    pub(super) read: &'a dyn Fn(&octocode_engine::types::TextSearchMatch) -> RowRead,
}

/// Rows one `expandValues` call runs: a page never needs more to read every
/// clipped value it shows.
pub(super) fn max_expansion_rows() -> usize {
    crate::response::result::rows_per_call(ToolId::LocalFetch)
        .min(crate::response::result::rows_per_call(ToolId::LocalSearch))
}

/// Rows one page shows: (file index, rank-order row range), in page order.
pub(super) type PageRows = Vec<(usize, std::ops::Range<usize>)>;

/// Default-layout pages, cut from one row stream: each file's first
/// `per_file` ranked rows in file order, then every file's remaining rows.
/// Each page holds as much of the stream as fits `budget` serialized chars
/// (at least one row), counting a file's `pagination` whenever its later
/// rows may fall on a later page; however many files that is, every hit
/// row it shows is named. No page shows clipped values that need more reads than one `expandValues`
/// call runs ([`max_expansion_rows`]), so each page's values are one call
/// away. `whole` puts the entire result on one page. A file's
/// rows on one page are always one contiguous rank range, and its later
/// rows follow on later pages. With `through`, the cut stops after that
/// many pages, plus a page of the one entry that opens the next.
pub(super) fn stream_pages(
    files: &[octocode_engine::types::TextSearchFile],
    per_file: usize,
    whole: bool,
    budget: usize,
    costs: StreamCosts<'_>,
    through: Option<usize>,
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
        .map(|(index, file)| (index, 0..file.matches.len().min(per_file)));
    let rest = files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.matches.len() > per_file)
        .map(|(index, file)| (index, per_file..file.matches.len()));
    let mut pages: Vec<PageRows> = Vec::new();
    let mut page: PageRows = Vec::new();
    // Each file on the page: its entry's position, and the reads its rows
    // there need, kept as rows join so a row's added reads cost no rescan.
    let mut slot = std::collections::HashMap::<usize, (usize, FileReads)>::new();
    // Files whose `pagination` this page already counts.
    let mut paged = std::collections::HashSet::<usize>::new();
    // Declarations the page already names in full.
    let mut names = PageNames::default();
    // Each file entry's own chars and its `pagination`'s, priced once: a
    // file rides several pages.
    let mut entries_chars = vec![None::<(usize, usize)>; files.len()];
    let mut used = 0usize;
    let max_reads = max_expansion_rows();
    // `expandValues` rows the page's clipped values need.
    let mut reads = 0usize;
    for (index, rows) in first.chain(rest) {
        let file = &files[index];
        let (file_chars, pagination) = *entries_chars[index]
            .get_or_insert_with(|| ((costs.file)(file), pagination_chars(file)));
        // A row-less file still takes its place as one entry.
        let entries: Vec<Option<usize>> = if rows.is_empty() {
            vec![None]
        } else {
            rows.map(Some).collect()
        };
        for row in entries {
            let more_after = row.is_some_and(|row| row + 1 < file.matches.len());
            let name = row.map_or(RowName::None, |row| (costs.name)(index, row));
            let read = row.map_or(RowRead::None, |row| (costs.read)(&file.matches[row]));
            let row_chars = row.map_or(0, |row| (costs.row)(&file.matches[row]));
            let cost = |slot: &std::collections::HashMap<usize, (usize, FileReads)>,
                        paged: &std::collections::HashSet<usize>,
                        names: &PageNames| {
                row_chars
                    + names.price(index, name)
                    + if slot.contains_key(&index) {
                        0
                    } else {
                        file_chars
                    }
                    + if more_after && !paged.contains(&index) {
                        pagination
                    } else {
                        0
                    }
            };
            // The reads this row adds to its file's on the page: only a
            // clipped row adds one.
            let added_reads = |slot: &std::collections::HashMap<usize, (usize, FileReads)>| {
                slot.get(&index).map_or_else(
                    || FileReads::default().added(read),
                    |(_, reads)| reads.added(read),
                )
            };
            let over_budget = used + cost(&slot, &paged, &names) > budget;
            let over_reads = reads + added_reads(&slot) > max_reads;
            if !page.is_empty() && (over_budget || over_reads) {
                pages.push(std::mem::take(&mut page));
                if through.is_some_and(|through| pages.len() > through) {
                    // The entry that opens the next page, alone.
                    let span = row.map_or(0..0, |row| row..row + 1);
                    pages.push(vec![(index, span)]);
                    return pages;
                }
                slot.clear();
                paged.clear();
                names = PageNames::default();
                used = 0;
                reads = 0;
            }
            used += cost(&slot, &paged, &names);
            names.add(index, name);
            reads += added_reads(&slot);
            if more_after {
                paged.insert(index);
            }
            let span = row.map_or(0..0, |row| row..row + 1);
            match slot.get_mut(&index) {
                Some((position, file_reads)) => {
                    let range = &mut page[*position].1;
                    *range = range.start.min(span.start)..range.end.max(span.end);
                    file_reads.add(read);
                }
                None => {
                    let mut file_reads = FileReads::default();
                    file_reads.add(read);
                    slot.insert(index, (page.len(), file_reads));
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
const MAX_MATCH_CONTENT_LENGTH: usize =
    crate::contracts::query_limits::local_search::MATCH_CONTENT_LENGTH_MAXIMUM;

/// Serialized chars of one `expandValues` read without its path (and its
/// ranges, which each row pays for).
pub(super) fn expansion_read_chars(query: &LocalSearchQuery, multiline: bool) -> usize {
    let read = if multiline {
        json!({"expandValues": {
            "tool": ToolId::LocalSearch.as_str(),
            "query": {"queries": [normalized_query(query, true)]},
        }})
    } else {
        json!({"expandValues": {
            "tool": ToolId::LocalFetch.as_str(),
            "query": {"queries": [{"path": "", "ranges": []}]},
        }})
    };
    octocode_engine::json_size::utf16_units(&read).unwrap_or(0)
}

/// A caller-sized page: the same `page`/`matchPage` show the same rows at
/// any `matchContentLength`, so one widened query reaches every clipped
/// value while `value_cap` (the response budget's per-row share) still
/// holds them.
pub(super) struct GridPage {
    pub(super) value_cap: usize,
}

/// A multiline file's clipped values: the file searched again alone with
/// `matchContentLength` raised to its longest clipped value (`None` when
/// already at the widest view), and a read from its first clipped line,
/// which localFetch pages to the end of the file.
fn multiline_expansion(
    query: &LocalSearchQuery,
    path: &str,
    clipped: &[&SearchMatch],
    context: SearchContext,
) -> (Option<Value>, Value) {
    let longest = clipped
        .iter()
        .filter_map(|matched| matched.original_chars)
        .max()
        .unwrap_or(0)
        .min(MAX_MATCH_CONTENT_LENGTH);
    let first = clipped
        .iter()
        .map(|matched| matched.line)
        .min()
        .unwrap_or(1);
    let fetch = Continuation::new(
        ToolId::LocalFetch,
        json!({"path": path, "ranges": [format!("{}-{}", first.saturating_sub(context.before).max(1), READ_TO_END_LINE)]}),
    )
    .build();
    // A file-scoped search is a fresh run: no page, snapshot or saved
    // selection of the wider scope carries over.
    let search = (longest > effective_match_content_length(query) as usize).then(|| {
        let mut search = normalized_query(query, true);
        if let Some(fields) = search.as_object_mut() {
            for key in ["page", "matchPage", "snapshot", "selection", "pageSize"] {
                fields.remove(key);
            }
            fields.insert("path".into(), json!(path));
            fields.insert("matchContentLength".into(), json!(longest));
        }
        Continuation::new(ToolId::LocalSearch, search).build()
    });
    (search, fetch)
}

/// Reads that return every clipped value of a page whole. A grid page whose
/// longest clipped value fits both the contract maximum and the per-row
/// budget gets one widened copy of the query. Otherwise each file gets its
/// own read: a line row's value is its hit line with the requested context
/// before and after it, so a localFetch of those source lines (`ranges`, merged, at
/// most [`RANGES_MAX_ITEMS`] per read) holds it whole, and localFetch pages a
/// long read itself. A clipped multiline row hides how many lines its match
/// spans, so its file is searched again alone with `matchContentLength`
/// raised to its longest clipped value (past the contract maximum, a
/// localFetch from its first line).
pub(super) fn expand_values(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    root: &std::path::Path,
    files: &[SearchFile],
    context: SearchContext,
    multiline: bool,
    grid: Option<GridPage>,
) -> Vec<Value> {
    let longest = files
        .iter()
        .flat_map(|file| file.matches.iter().flatten())
        .filter(|matched| matched.clipped)
        .filter_map(|matched| matched.original_chars)
        .max();
    let Some(longest) = longest else {
        return Vec::new();
    };
    if let Some(grid) = grid
        && longest <= MAX_MATCH_CONTENT_LENGTH.min(grid.value_cap)
        && longest > effective_match_content_length(query) as usize
    {
        // The snapshot and a saved selection name this value width, so the
        // widened page is a fresh run of the same page (same rows while the
        // source is unchanged).
        let mut widened = normalized_query(query, false);
        widened["matchContentLength"] = json!(longest);
        if let Some(fields) = widened.as_object_mut() {
            fields.remove("snapshot");
            fields.remove("selection");
        }
        return vec![Continuation::new(ToolId::LocalSearch, widened).build()];
    }
    let mut reads = Vec::new();
    // Multiline files: a file-scoped search widened to its longest value
    // when that view exists, else a read from its first clipped line.
    let mut multiline_reads = Vec::new();
    for file in files {
        let clipped = file
            .matches
            .iter()
            .flatten()
            .filter(|matched| matched.clipped)
            .collect::<Vec<_>>();
        if clipped.is_empty() {
            continue;
        }
        let source = root.join(&file.path);
        let path = paths
            .workspace_relative(&source)
            .unwrap_or_else(|| source.to_string_lossy().into_owned());
        if multiline {
            multiline_reads.push(multiline_expansion(query, &path, &clipped, context));
            continue;
        }
        // A value clipped out of a line longer than a read page is read by
        // bytes from its hit; a line-range read would show the line's start.
        let (long, plain): (Vec<_>, Vec<_>) = clipped
            .into_iter()
            .partition(|matched| is_long_line_hit(matched));
        reads.extend(
            long.into_iter()
                .map(|matched| long_line_read(&path, matched)),
        );
        let merged = hit_windows(plain.iter().map(|matched| matched.line), context);
        for chunk in merged.chunks(RANGES_MAX_ITEMS) {
            let ranges = chunk
                .iter()
                .map(|(start, end)| format!("{start}-{end}"))
                .collect::<Vec<_>>();
            reads.push(
                Continuation::new(ToolId::LocalFetch, json!({"path": path, "ranges": ranges}))
                    .build(),
            );
        }
    }
    // One lead runs one tool: the widened searches only while every file
    // has one; otherwise every file is read from its first clipped line,
    // which reaches any value whole.
    let search_all = multiline_reads.iter().all(|(search, _)| search.is_some());
    reads.extend(
        multiline_reads
            .into_iter()
            .map(|(search, fetch)| search.filter(|_| search_all).unwrap_or(fetch)),
    );
    reads
}

/// The `expandValues` rows a page's rows of `file` need when their values
/// are read per file (see [`expand_values`]): one per long-line hit and one
/// per [`RANGES_MAX_ITEMS`] merged hit windows; a multiline file needs one.
pub(super) fn expansion_rows<'a>(
    rows: impl IntoIterator<Item = &'a octocode_engine::types::TextSearchMatch>,
    costs: &PageCosts,
) -> usize {
    let mut reads = FileReads::default();
    for matched in rows {
        reads.add(costs.read(matched));
    }
    reads.rows()
}

/// What reading one row's value whole takes ([`expand_values`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowRead {
    /// The value is whole.
    None,
    /// A clipped multiline value: its file is read once, however many.
    Multiline,
    /// A clipped value on a line past one read page: its own byte read.
    Long,
    /// A clipped line value: its hit lines with this context on each side.
    Window { start: u32, end: u32 },
}

impl PageCosts {
    /// What reading `matched`'s value whole takes.
    pub(super) fn read(&self, matched: &octocode_engine::types::TextSearchMatch) -> RowRead {
        if !self.clipped(matched) {
            RowRead::None
        } else if self.multiline {
            RowRead::Multiline
        } else if super::handoff::passes_line_page(
            // The same rows `is_long_line_hit` reads by a byte window.
            matched
                .original_bytes
                .map_or_else(|| matched.value.len(), |bytes| bytes as usize),
        ) {
            RowRead::Long
        } else {
            let (start, end) = hit_windows([matched.line], self.expand_context)[0];
            RowRead::Window { start, end }
        }
    }
}

/// The `expandValues` rows one file's rows on a page need, kept as rows
/// join: its long-line hits, its merged hit windows ([`hit_windows`]), and
/// whether a multiline value is clipped. Adding a row costs a lookup among
/// the windows, so a page cut never rescans the rows a file already shows.
#[derive(Clone, Debug, Default)]
pub(super) struct FileReads {
    multiline: bool,
    long: usize,
    /// Merged windows, start to end: disjoint and not adjacent.
    windows: std::collections::BTreeMap<u32, u32>,
}

impl FileReads {
    /// The `expandValues` rows these reads take.
    pub(super) fn rows(&self) -> usize {
        if self.multiline {
            1
        } else {
            self.long + self.windows.len().div_ceil(RANGES_MAX_ITEMS)
        }
    }

    /// The rows `read` would add, without adding it.
    pub(super) fn added(&self, read: RowRead) -> usize {
        let after = match read {
            RowRead::None => return 0,
            RowRead::Multiline => 1,
            _ if self.multiline => 1,
            RowRead::Long => self.long + 1 + self.windows.len().div_ceil(RANGES_MAX_ITEMS),
            RowRead::Window { start, end } => {
                let merged = self.touching(start, end).count();
                self.long + (self.windows.len() + 1 - merged).div_ceil(RANGES_MAX_ITEMS)
            }
        };
        after.saturating_sub(self.rows())
    }

    pub(super) fn add(&mut self, read: RowRead) {
        match read {
            RowRead::None => {}
            RowRead::Multiline => self.multiline = true,
            RowRead::Long => self.long += 1,
            RowRead::Window { start, end } => {
                let touching = self.touching(start, end).collect::<Vec<_>>();
                let mut merged = (start, end);
                for (from, to) in touching {
                    self.windows.remove(&from);
                    merged = (merged.0.min(from), merged.1.max(to));
                }
                self.windows.insert(merged.0, merged.1);
            }
        }
    }

    /// The windows `start..=end` overlaps or adjoins, as [`hit_windows`]
    /// merges them.
    fn touching(&self, start: u32, end: u32) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.windows
            .range(..=end.saturating_add(1))
            .rev()
            .take_while(move |(_, to)| to.saturating_add(1) >= start)
            .map(|(&from, &to)| (from, to))
    }
}

/// Line runs `moreLines` names before it summarizes the rest as a count.
pub(super) const MAX_MORE_LINES: usize = 24;

#[cfg(test)]
mod tests {
    use super::super::local_search_query::SPAN_COLUMN_PATH;
    use super::*;
    use octocode_engine::types::{TextSearchFile, TextSearchMatch};

    fn hit(line: u32, clipped: bool) -> TextSearchMatch {
        TextSearchMatch {
            deferred: None,
            line,
            last_line: None,
            column: 0,
            byte_offset: 0,
            value: format!("needle {line}"),
            value_digest: [0; 32],
            count: None,
            rank: Some(1),
            original_chars: clipped.then_some(4_000),
            original_bytes: clipped.then_some(4_000),
        }
    }

    /// Kept as rows join, a file's reads equal the batch count over the
    /// same rows ([`hit_windows`] merged, long hits one each), and `added`
    /// is exactly what `add` then changes.
    #[test]
    fn incremental_file_reads_equal_the_batch_count() {
        // Lines in rank order: out of line order, overlapping, adjacent,
        // bridging, repeated.
        let lines = [
            40u32, 10, 12, 60, 11, 30, 26, 34, 90, 1, 2, 70, 80, 55, 45, 50, 100, 120, 140, 160,
            180, 200, 220, 240, 260, 25, 33,
        ];
        for context in [0, 2, 4] {
            let mut reads = FileReads::default();
            let mut shown = Vec::new();
            for (n, &line) in lines.iter().enumerate() {
                let read = if n % 7 == 3 {
                    RowRead::Long
                } else {
                    let (start, end) = hit_windows([line], SearchContext::symmetric(context))[0];
                    RowRead::Window { start, end }
                };
                let before = reads.rows();
                let added = reads.added(read);
                reads.add(read);
                assert_eq!(reads.rows(), before + added, "{context} {line}");
                shown.push((line, read));
                let long = shown
                    .iter()
                    .filter(|(_, read)| *read == RowRead::Long)
                    .count();
                let windows = hit_windows(
                    shown
                        .iter()
                        .filter(|(_, read)| *read != RowRead::Long)
                        .map(|(line, _)| *line),
                    SearchContext::symmetric(context),
                );
                assert_eq!(
                    reads.rows(),
                    long + windows.len().div_ceil(RANGES_MAX_ITEMS),
                    "{context} {line}"
                );
            }
        }
        let mut multiline = FileReads::default();
        assert_eq!(multiline.added(RowRead::Multiline), 1);
        multiline.add(RowRead::Multiline);
        assert_eq!(multiline.added(RowRead::Multiline), 0);
        assert_eq!(multiline.added(RowRead::None), 0);
    }

    /// Cutting every page prices each row once and each file entry once,
    /// however many pages a file rides, and every page keeps within its
    /// read bound.
    #[test]
    fn stream_pages_price_each_row_and_file_once() {
        let files = (0..30u32)
            .map(|n| TextSearchFile {
                path: format!("src/f{n}.rs"),
                match_count: 40,
                matches: (1..=40).map(|line| hit(line * 3, line % 2 == 0)).collect(),
                source: None,
            })
            .collect::<Vec<_>>();
        let rows = std::cell::Cell::new(0usize);
        let entries = std::cell::Cell::new(0usize);
        let read = |matched: &TextSearchMatch| {
            if matched.original_chars.is_some() {
                let (start, end) = hit_windows([matched.line], SearchContext::symmetric(1))[0];
                RowRead::Window { start, end }
            } else {
                RowRead::None
            }
        };
        let pages = stream_pages(
            &files,
            10,
            false,
            2_000,
            StreamCosts {
                file: &|_| {
                    entries.set(entries.get() + 1);
                    30
                },
                row: &|_| {
                    rows.set(rows.get() + 1);
                    25
                },
                name: &|_, _| RowName::None,
                read: &read,
            },
            None,
        );
        assert!(pages.len() > 1, "{}", pages.len());
        assert_eq!(rows.get(), 30 * 40);
        assert_eq!(entries.get(), 30);
        let mut seen = 0;
        for page in &pages {
            let reads = page
                .iter()
                .map(|(index, range)| {
                    let mut reads = FileReads::default();
                    for matched in &files[*index].matches[range.clone()] {
                        reads.add(read(matched));
                    }
                    reads.rows()
                })
                .sum::<usize>();
            assert!(reads <= max_expansion_rows(), "{reads}");
            seen += page.iter().map(|(_, range)| range.len()).sum::<usize>();
        }
        assert_eq!(seen, 30 * 40, "every row on exactly one page");
    }

    /// A row is priced from its borrowed value exactly as the copy the page
    /// shows serializes, with `column` only where the response keeps it.
    #[test]
    fn row_chars_price_the_row_as_shown() {
        let mut matched = hit(7, false);
        matched.value = "quote \" tab \t é 😀 needle".into();
        matched.column = 4;
        for (cap, span_columns) in [(None, false), (Some(5), true), (Some(500), false)] {
            let mut row = project_match(&matched, cap);
            if !span_columns {
                row.column = None;
            }
            assert_eq!(
                row_chars(&matched, cap, span_columns),
                octocode_engine::json_size::utf16_units(&row).unwrap_or(0) + 1,
                "{cap:?} {span_columns}"
            );
        }
    }

    /// Page `page` of `files` cut over `root` with `known` prices: the
    /// layout, every price known after the cut, and the files it outlined.
    /// Each row's declaration is named on the line before it, unless the
    /// files are `nameless`.
    fn cut_page(
        root: &std::path::Path,
        files: &[TextSearchFile],
        page: u32,
        known: HashMap<usize, FilePrice>,
        nameless: bool,
    ) -> (Layout, HashMap<usize, FilePrice>, Vec<usize>) {
        let paths = crate::tools::test_support::workspace_policy(root);
        let mut parsed = octocode_engine::types::TextSearchResult {
            files: files.to_vec(),
            stats: Default::default(),
        };
        let mut outlined = Vec::new();
        let mut outline = |asked: &[(usize, &TextSearchFile)]| {
            Ok(asked
                .iter()
                .map(|&(index, file)| {
                    outlined.push(index);
                    let owners = file
                        .matches
                        .iter()
                        .filter(|_| !nameless)
                        .map(|m| {
                            (
                                m.line,
                                Owner {
                                    line: m.line - 1,
                                    full: 60,
                                },
                            )
                        })
                        .collect();
                    (index, FilePrice::Owners(Arc::new(owners)))
                })
                .collect())
        };
        let mut pricing = NamePricing::new(known, &mut outline);
        let request = query(serde_json::json!({"page": page}));
        let layout =
            Layout::cut(&request, &paths, root, &mut parsed, 6_000, &mut pricing).expect("cut");
        let priced = pricing.priced;
        outlined.sort_unstable();
        (layout, priced, outlined)
    }

    /// A default page cut outlines only the files that may decide its first
    /// page: those of the first page a cut with free names reaches, and the
    /// row that ends it; never a fixed count of files. Every call of a walk
    /// cuts the same pages: fresh, or from the prices the first page stored,
    /// which outlines nothing more.
    #[test]
    fn page_cuts_outline_only_the_first_pages_files_and_every_call_cuts_alike() {
        let root = tempfile::tempdir().expect("fixture directory");
        let files = (0..100u32)
            .map(|n| TextSearchFile {
                path: format!("src/f{n:03}.rs"),
                match_count: 30,
                matches: (1..=30).map(|line| hit(line * 4, false)).collect(),
                source: None,
            })
            .collect::<Vec<_>>();
        let files_of = |layout: &Layout| {
            layout
                .shown
                .iter()
                .map(|(index, _)| *index)
                .collect::<Vec<_>>()
        };
        let (first, stored, outlined) = cut_page(root.path(), &files, 1, HashMap::new(), false);
        let (free, _, _) = cut_page(root.path(), &files, 1, HashMap::new(), true);
        let (after_free, _, _) = cut_page(root.path(), &files, 2, HashMap::new(), true);
        let mut reach = files_of(&free);
        reach.push(after_free.shown[0].0);
        reach.dedup();
        assert_eq!(outlined, reach);
        assert!(outlined.len() < 64, "{outlined:?}");
        assert!(first.total_pages > 2, "{}", first.total_pages);
        assert!(
            files_of(&first)
                .iter()
                .all(|index| outlined.contains(index))
        );
        let mut rows = std::collections::BTreeSet::new();
        for page in 1..=first.total_pages {
            let (fresh, _, _) = cut_page(root.path(), &files, page, HashMap::new(), false);
            let (replayed, _, more) = cut_page(root.path(), &files, page, stored.clone(), false);
            assert!(more.is_empty(), "page {page} outlined {more:?}");
            assert_eq!(fresh.shown, replayed.shown, "page {page}");
            assert_eq!(fresh.total_pages, first.total_pages, "page {page}");
            assert_eq!(replayed.total_pages, first.total_pages, "page {page}");
            for (index, range) in fresh.shown {
                for row in range {
                    assert!(rows.insert((index, row)), "row {index}:{row} twice");
                }
            }
        }
        assert_eq!(rows.len(), 100 * 30);
    }

    fn query(fields: serde_json::Value) -> LocalSearchQuery {
        let mut value = serde_json::json!({"path": ".", "matchString": "x", "mainGoal": "test", "reasoning": "test"});
        for (key, field) in fields.as_object().expect("fields") {
            value[key] = field.clone();
        }
        serde_json::from_value(value).expect("query")
    }

    /// Span rows are priced with `column` only while the response keeps
    /// it: line rows never carry one, and span rows always do, since equal
    /// spans on one line differ only by column (L1-F4: core classes it
    /// evidence, not an `extras` detail).
    #[test]
    fn span_columns_follow_what_the_response_keeps() {
        assert!(!shows_span_columns(&query(serde_json::json!({}))));
        assert!(crate::response::extras::keeps(
            ToolId::LocalSearch,
            &serde_json::json!({}),
            SPAN_COLUMN_PATH,
        ));
        assert!(shows_span_columns(&query(
            serde_json::json!({"resultView": "matchOnly"})
        )));
        assert!(shows_span_columns(&query(
            serde_json::json!({"resultView": "matchOnly", "extras": ["details"]})
        )));
    }
}
