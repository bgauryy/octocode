//! Page layout: how a scan is cut into pages (grid or streamed), what a
//! page costs, and the reads that expand clipped values.
use crate::tools::local_search::local_search_query::View;

mod costs;
mod entries;
mod expansion;
mod stream;

use super::local_search_query::{
    self, DEFAULT_MAX_MATCHES_PER_FILE, MIN_MATCH_VALUE_CHARS, RESPONSE_VALUE_CHAR_BUDGET,
    default_page_size, streamed_layout,
};
use super::rows::rank_file_rows;
use crate::num::u32_of_usize;
use crate::policy::path::PathPolicy;
use crate::response::result::ToolError;
pub(super) use costs::{FilePrice, NamePricing};
use costs::{PageCosts, RowName};
pub(super) use entries::{file_pagination, row_chars};
use entries::{fits_whole, listed_file_chars};
pub(super) use expansion::expand_values;
use expansion::expansion_rows;
use octocode_contract_types::LocalSearchQuery;
use octocode_engine::types::SearchContext;
use stream::{PageRows, StreamCosts, max_expansion_rows, stream_pages};

/// Whether a view lists paths (with counts) instead of match rows.
pub(super) fn list_view(view: View) -> bool {
    matches!(
        view,
        View::Files | View::FilesWithout | View::CountLines | View::CountMatches
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
        let view = local_search_query::view(query);
        let list = list_view(view);
        let match_only = view == View::MatchOnly;
        let context = local_search_query::context(query);
        let mut costs = PageCosts::new(query, paths, output_root, context);
        costs.owners = pricing.priced.clone();
        let priced_names = costs.shows_names;
        let hits_total: usize = if list {
            0
        } else {
            parsed.files.iter().map(|file| file.matches.len()).sum()
        };
        // A name costs at least nothing: a result that does not fit whole
        // without its names never shows whole, and one that may is priced
        // whole (it is at most one page of files).
        let show_all = !list
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
        let matches_per = if show_all {
            u32_of_usize(hits_total)
        } else {
            DEFAULT_MAX_MATCHES_PER_FILE
        }
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
    fn stream(
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
                listed_file_chars(file, local_search_query::view(query)) + costs.prefix_chars + 1
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
    fn grid(
        query: &LocalSearchQuery,
        file_count: usize,
        matches_per: u32,
    ) -> (PageRows, u32, Option<u32>, u32) {
        let page_size = default_page_size(local_search_query::view(query)).max(1);
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

#[cfg(test)]
mod tests;
