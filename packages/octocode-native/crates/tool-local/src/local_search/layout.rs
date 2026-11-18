//! Page layout: how a scan is cut into pages, what a
//! page costs, preserving each selected value whole.
use crate::local_search::local_search_query::View;

mod costs;
mod entries;
mod stream;

use super::local_search_query;
#[cfg(test)]
pub(super) use costs::priced_on_this_thread;
pub(super) use costs::{FilePrice, NamePricing};
use costs::{PageCosts, RowName};
pub(super) use entries::{file_pagination, row_chars};
use entries::{fits_whole, listed_file_chars};
use octocode_contract_types::LocalSearchQuery;
use octocode_engine::num::saturating_u32;
use octocode_engine::types::SearchContext;
use octocode_runtime_base::policy::path::PathPolicy;
use octocode_runtime_base::response::result::ToolError;
pub(super) use stream::PageRows;
use stream::{StreamCosts, stream_pages};

/// Whether a view lists paths (with counts) instead of match rows.
pub(super) fn list_view(view: View) -> bool {
    matches!(
        view,
        View::Files | View::FilesWithout | View::CountLines | View::CountMatches
    )
}

/// The pages one scan's stream cut yields. Every call of a walk cuts the
/// same pages from the same scan (see [`Layout::stream`]), so the stored
/// scan keeps them once cut: a continuation takes its page instead of
/// pricing every row of the scan again. Once the scan is stored, a cut is
/// charged to the store ([`Billing`]) and kept only when the charge holds.
#[derive(Default)]
pub(super) struct StreamCuts(
    std::sync::Mutex<Option<std::sync::Arc<Vec<PageRows>>>>,
    super::manifest::Billing,
);

impl StreamCuts {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<std::sync::Arc<Vec<PageRows>>>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Bytes the cut keeps.
    pub(super) fn weight(&self) -> usize {
        self.lock().as_deref().map_or(0, |pages| cut_weight(pages))
    }

    /// Drop the kept cut.
    pub(super) fn clear(&self) {
        *self.lock() = None;
    }

    /// Where a cut made once the scan is stored is charged.
    pub(super) fn billing(&self) -> &super::manifest::Billing {
        &self.1
    }

    /// Keep `pages` as the scan's cut, unless one is kept already, when its
    /// bytes are charged (or nothing bills them yet). Charged and kept under
    /// one lock, so a cut two pages make is charged once.
    pub(super) fn keep(&self, pages: &std::sync::Arc<Vec<PageRows>>) {
        let mut kept = self.lock();
        if kept.is_none() && self.1.charge(cut_weight(pages)) {
            *kept = Some(std::sync::Arc::clone(pages));
        }
    }
}

/// Bytes a cut of `pages` keeps.
fn cut_weight(pages: &[PageRows]) -> usize {
    pages
        .iter()
        .map(|page| {
            std::mem::size_of::<PageRows>()
                + page.len() * std::mem::size_of::<(usize, std::ops::Range<usize>)>()
        })
        .sum()
}

impl StreamCuts {
    #[cfg(test)]
    pub(super) fn kept(&self) -> bool {
        self.lock().is_some()
    }
}

/// A page cut from the files a traversal stream read ([`super::streamed`]):
/// the first file's rows an earlier page showed are skipped, and nothing
/// shows whole while the walk goes on.
pub(super) struct Walking {
    pub(super) start_row: usize,
    /// The walk reached its end: no candidate lies past the files.
    pub(super) ended: bool,
}

/// The test a traversal stream ends by: `true` for the file that makes its
/// page overflow, so the page is full and the rows past it are the next
/// page's. It prices the page as the cut does (each file's path named from
/// the root, as the page shows it), less the enclosing names and the
/// per-file paging (which cost more), so a page it calls full is full
/// whatever the cut charges: it never ends a stream early on a file the page
/// then shows whole, and may read a file or two beyond the exact cut. Files are given in walk order; the
/// first one's rows before `start_row` were shown by an earlier page.
pub(super) fn page_fill(
    query: &LocalSearchQuery,
    paths: &PathPolicy,
    output_root: &std::path::Path,
    (budget, start_row): (usize, usize),
) -> impl FnMut(&octocode_engine::types::TextSearchFile) -> bool + Send + use<> {
    let costs = PageCosts::new(query, paths, output_root);
    let view = local_search_query::view(query);
    let list = list_view(view);
    let limit = local_search_query::limit(query)
        .unwrap_or(usize::MAX)
        .max(1);
    let (mut used, mut entries, mut first) = (0usize, 0usize, true);
    move |file| {
        let skipped = if std::mem::take(&mut first) {
            start_row.min(file.matches.len())
        } else {
            0
        };
        let (chars, count) = if list {
            (listed_file_chars(file, view) + costs.prefix_chars + 1, 1)
        } else {
            let rows = &file.matches[skipped..];
            (
                costs.file(file) + rows.iter().map(|row| costs.row(row)).sum::<usize>(),
                rows.len().max(1),
            )
        };
        let past = used.saturating_add(chars) > budget || entries.saturating_add(count) > limit;
        let overflows = past && (used > 0 || count > 1);
        used = used.saturating_add(chars);
        entries = entries.saturating_add(count);
        overflows
    }
}

/// What a streamed page says of the pages around it.
#[derive(Clone, Copy, Debug)]
pub(super) struct WalkPage {
    /// Rows remain after this page.
    pub(super) more: bool,
    /// An earlier page showed rows of this search.
    pub(super) continued: bool,
    /// What the pages before carried past their cut
    /// ([`super::executor::streamed::StreamFacts::carried`]).
    pub(super) carried: super::executor::streamed::Carried,
}

/// How one page is cut and shown.
pub(super) struct Layout {
    pub(super) list: bool,
    pub(super) shown: PageRows,
    pub(super) total_pages: u32,
    /// What the pages walk: hit rows on default content pages (a file rides
    /// several), files on list pages.
    pub(super) total_items: u32,
    /// Where this page starts among `total_items`, from 0, and how many of
    /// them it holds.
    pub(super) offset: usize,
    pub(super) length: usize,
    /// What one hit row is: a matched line on content pages, an occurrence
    /// on matchOnly pages. Hit-row pagination states it as its `unit`.
    pub(super) row_unit: &'static str,
    pub(super) out_of_range: bool,
    /// Resolved context around each row (empty for matchOnly).
    pub(super) context: SearchContext,
    /// Context windows of nearby rows merge into one block.
    pub(super) merge_context: Option<SearchContext>,
    /// Every page of a streamed cut, when this call cut it (a fresh page
    /// stores the scan by them, see [`super::page_view::encode_pages`]).
    pub(super) pages: Option<std::sync::Arc<Vec<PageRows>>>,
    /// The page is one of a traversal stream: its totals are floors while
    /// `more` rows remain ([`Self::walked`]).
    pub(super) walk: Option<WalkPage>,
}

/// The unit of one hit row: an occurrence on matchOnly pages, a matched line
/// (its occurrences on one row) otherwise.
fn row_unit(match_only: bool) -> &'static str {
    if match_only { "matches" } else { "lines" }
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
    /// row that ends it (see [`Self::stream`]). `cuts`: the scan's stream
    /// cut, taken when known and kept once made.
    pub(super) fn cut(
        query: &LocalSearchQuery,
        paths: &PathPolicy,
        output_root: &std::path::Path,
        parsed: &mut octocode_engine::types::TextSearchResult,
        page_budget: usize,
        (pricing, cuts, walking): (&mut NamePricing<'_>, Option<&StreamCuts>, Option<&Walking>),
    ) -> Result<Self, ToolError> {
        let view = local_search_query::view(query);
        let list = list_view(view);
        let match_only = view == View::MatchOnly;
        let context = local_search_query::context(query);
        let mut costs = PageCosts::new(query, paths, output_root);
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
        // A `limit` below the result's rows pages it, however small it is.
        let within_limit = local_search_query::limit(query).is_none_or(|limit| hits_total <= limit);
        let show_all = !list
            && within_limit
            && walking.is_none_or(|walking| walking.ended)
            && fits_whole(&parsed.files, page_budget, &costs, true)
            && {
                if priced_names {
                    let all = (0..parsed.files.len()).collect::<Vec<_>>();
                    pricing.price(&parsed.files, all)?;
                    costs.owners = pricing.priced.clone();
                }
                fits_whole(&parsed.files, page_budget, &costs, false)
            };
        // A page shows rows as written, file after file.
        let mut pages = None;
        let (shown, total_pages, total_items, offset) = if list {
            Self::list_stream(query, &parsed.files, page_budget, &costs)
        } else {
            let pricing = priced_names.then_some(pricing);
            Self::stream(
                query,
                &parsed.files,
                show_all,
                page_budget,
                &mut costs,
                (pricing, cuts),
                (walking.map_or(0, |walking| walking.start_row), &mut pages),
            )?
        };
        let total_files = saturating_u32(parsed.files.len());
        let length = if list {
            shown.len()
        } else {
            shown_rows(&shown, &parsed.files)
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
            total_items,
            offset,
            length,
            row_unit: row_unit(match_only),
            context: if match_only {
                SearchContext::default()
            } else {
                context
            },
            merge_context,
            pages,
            walk: None,
        })
    }

    /// State that this page is one of a traversal stream: it starts after
    /// `offset` rows earlier pages showed, and `more` rows remain. Its
    /// totals count what the walk has shown so far, a floor while `more`.
    pub(super) fn walked(
        &mut self,
        offset: usize,
        more: bool,
        carried: super::executor::streamed::Carried,
    ) {
        self.walk = Some(WalkPage {
            more,
            continued: offset > 0,
            carried,
        });
        self.offset = offset;
        self.total_items = saturating_u32(offset + self.length);
        self.total_pages = if more { 2 } else { 1 };
        self.out_of_range = false;
    }

    /// The layout of the page `query` asks for of a seekable stored scan
    /// (always streamed), from the cut its first page stored (`index`): no
    /// row is ranked or priced again. `shown` are the page's own rows (see
    /// [`super::page_view::PageView`]).
    pub(super) fn from_index(
        query: &LocalSearchQuery,
        index: &super::packed::PageIndex,
        shown: PageRows,
    ) -> Self {
        let match_only = local_search_query::view(query) == View::MatchOnly;
        let context = if match_only {
            SearchContext::default()
        } else {
            local_search_query::context(query)
        };
        let total_pages = saturating_u32(index.pages.len()).max(1);
        let page = local_search_query::page(query).max(1) as usize;
        let offset = index
            .starts
            .get(page - 1)
            .map_or(index.total_rows as usize, |start| *start as usize);
        let length = shown.iter().map(|(_, rows)| rows.len()).sum();
        Self {
            list: false,
            out_of_range: index.total_files > 0
                && local_search_query::page(query).max(1) > total_pages,
            shown,
            total_pages,
            total_items: index.total_rows,
            offset,
            length,
            row_unit: row_unit(match_only),
            merge_context: (!match_only && !context.is_empty()).then_some(context),
            context,
            pages: None,
            walk: None,
        }
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
        show_all: bool,
        page_budget: usize,
        costs: &mut PageCosts,
        (pricing, cuts): (Option<&mut NamePricing<'_>>, Option<&StreamCuts>),
        (start, cut_pages): (usize, &mut Option<std::sync::Arc<Vec<PageRows>>>),
    ) -> Result<(PageRows, u32, u32, usize), ToolError> {
        // `through`: the cut stops after that page, every reserved name
        // costing nothing (see below).
        let cut = |costs: &PageCosts, through: Option<usize>| {
            stream_pages(
                files,
                usize::MAX,
                show_all,
                (page_budget, local_search_query::limit(query)),
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
                },
                through,
                start,
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
        let known = cuts.and_then(|cuts| cuts.lock().clone());
        let pages = known.unwrap_or_else(|| {
            let pages = std::sync::Arc::new(cut(costs, None));
            if let Some(cuts) = cuts {
                cuts.keep(&pages);
            }
            pages
        });
        let total_pages = saturating_u32(pages.len()).max(1);
        let page = local_search_query::page(query).max(1) as usize;
        let shown = pages.get(page - 1).cloned().unwrap_or_default();
        let rows: usize = files.iter().map(|file| file.matches.len()).sum();
        let offset = pages
            .iter()
            .take(page - 1)
            .map(|before| shown_rows(before, files))
            .sum();
        *cut_pages = Some(pages);
        Ok((shown, total_pages, saturating_u32(rows), offset))
    }

    fn list_stream(
        query: &LocalSearchQuery,
        files: &[octocode_engine::types::TextSearchFile],
        page_budget: usize,
        costs: &PageCosts,
    ) -> (PageRows, u32, u32, usize) {
        let sizes = files
            .iter()
            .map(|file| {
                listed_file_chars(file, local_search_query::view(query)) + costs.prefix_chars + 1
            })
            .collect::<Vec<_>>();
        let cut = octocode_runtime_base::response::page_windows::select(
            octocode_runtime_base::response::page_windows::Paging::of(
                local_search_query::limit(query),
                page_budget,
            ),
            &sizes,
            local_search_query::page(query) as usize,
        );
        (
            cut.range.clone().map(|index| (index, 0..0)).collect(),
            saturating_u32(cut.pages),
            saturating_u32(files.len()),
            cut.range.start,
        )
    }
}

/// The hit rows `page` shows of `files`.
fn shown_rows(page: &PageRows, files: &[octocode_engine::types::TextSearchFile]) -> usize {
    page.iter()
        .map(|(index, rows)| {
            let total = files.get(*index).map_or(0, |file| file.matches.len());
            rows.end.min(total).saturating_sub(rows.start.min(total))
        })
        .sum()
}

#[cfg(test)]
mod tests;
