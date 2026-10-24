//! One merged page across the `extensions` searches, with repeats from earlier pages dropped.
use super::{GhSearchCodeQuery, GhSearchCodeQueryMatchInItem, per_search, query};
use crate::tools::gh_shared::SearchWindow;
use octocode_github::{CodeSearchRequest, GitHubProvider, ProviderError, RequestContext};

/// One merged page across `extensions`: GitHub ANDs repeated `extension:`
/// qualifiers, so each extension is its own indexed search of
/// `ceil(pageSize / k)` files a GitHub page ([`per_search`]). Page 1 lists
/// page 1 of every search; their totals lay out the later pages, which take
/// the searches' remaining GitHub pages `k` at a time, page by page in
/// extension order ([`later_pages`]). A search that has run out is never
/// asked for a later page, a page stays about `pageSize` files while any
/// search has files left, and every GitHub page is on exactly one merged
/// page: lossless. One search's page N is its GitHub page N. A later page
/// lays itself out from every search's page 1, read from the code-search
/// cache: when that cache is cold (memory storage across CLI processes, or
/// an expired page 1) each page 1 is searched again, one request per
/// search, an exhausted one included.
pub(super) struct SearchPage {
    /// The files this page lists: GitHub's, less those an earlier page of
    /// the same search already listed.
    pub(super) items: Vec<octocode_github::CodeSearchItem>,
    /// Files GitHub returned for the page, before repeats were dropped.
    pub(super) fetched: usize,
    /// Repeats dropped: files an earlier page (or this one) already listed.
    pub(super) dropped: usize,
    /// Earlier page searches the repeat check could read, of those it needed.
    pub(super) earlier_checked: usize,
    pub(super) earlier_total: usize,
    /// Reachable results (each search's window ends at its last whole
    /// page within 1,000 results).
    pub(super) total: usize,
    pub(super) pages: usize,
    /// GitHub's match counts, summed over the merged searches.
    pub(super) matched: usize,
    pub(super) incomplete: bool,
    /// Organizations SAML enforcement withheld from the merged searches.
    pub(super) sso_partial: Vec<u64>,
}

/// The GitHub pages (search index, page number) merged page `current`
/// (≥ 2) lists, given each search's page count: pages 2 and on of every
/// search that has them, page by page, `searches` to a merged page.
pub(super) fn later_pages(pages: &[usize], current: usize) -> Vec<(usize, usize)> {
    let per = pages.len().max(1);
    let deepest = pages.iter().copied().max().unwrap_or(0);
    (2..=deepest)
        .flat_map(|page| {
            pages
                .iter()
                .enumerate()
                .filter(move |&(_, &count)| count >= page)
                .map(move |(search, _)| (search, page))
        })
        .skip((current.saturating_sub(2)).saturating_mul(per))
        .take(per)
        .collect()
}

/// Merged pages of searches with these page counts: page 1, then the
/// later GitHub pages `searches` at a time; none when every search is empty.
pub(super) fn merged_pages(pages: &[usize]) -> usize {
    if pages.iter().all(|&count| count == 0) {
        return 0;
    }
    let later: usize = pages.iter().map(|count| count.saturating_sub(1)).sum();
    1 + later.div_ceil(pages.len().max(1))
}

pub(super) async fn search_page(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    current: usize,
    per: usize,
    context: &RequestContext,
) -> Result<SearchPage, ProviderError> {
    let extensions = query
        .extensions
        .iter()
        .map(|extension| extension.as_str())
        .collect::<Vec<_>>();
    let scopes = match extensions.as_slice() {
        [_, _, ..] => extensions.iter().copied().map(Some).collect(),
        _ => vec![None],
    };
    let per_search = per_search(query, per);
    let mut page = SearchPage {
        items: Vec::new(),
        fetched: 0,
        dropped: 0,
        earlier_checked: 0,
        earlier_total: 0,
        total: 0,
        pages: 0,
        matched: 0,
        incomplete: false,
        sso_partial: Vec::new(),
    };
    let searches = scopes
        .into_iter()
        .map(|extension| CodeSearchRequest {
            query: query::code(query, extension),
            page: 1,
            per_page: per_search,
            include_fragments: query
                .match_in
                .contains(&GhSearchCodeQueryMatchInItem::Content),
        })
        .collect::<Vec<_>>();
    let at = |search: usize, number: usize| page_of(&searches[search], number);
    let merged = searches.len() > 1;
    // A later merged page is laid out by every search's page 1 (usually
    // cached by the walk's first page).
    let (firsts, reserved) = if merged && current > 1 {
        first_pages(provider, &searches, context).await?
    } else {
        (Vec::new(), false)
    };
    let counts = |firsts: &[Option<octocode_github::CodeSearchPage>]| {
        firsts
            .iter()
            .map(|first| first.as_ref().map_or(0, |first| first.total_count))
            .collect::<Vec<_>>()
    };
    let reach_pages = |counts: &[usize]| {
        counts
            .iter()
            .map(|&count| SearchWindow::PageEnd.reach(count, per_search).pages)
            .collect::<Vec<_>>()
    };
    let units = if merged && current > 1 {
        later_pages(&reach_pages(&counts(&firsts)), current)
    } else {
        (0..searches.len())
            .map(|search| (search, current))
            .collect()
    };
    let requests = units
        .iter()
        .map(|&(search, number)| at(search, number))
        .collect::<Vec<_>>();
    let mut cached = Vec::with_capacity(requests.len());
    for request in &requests {
        cached.push(
            provider
                .transport
                .cached_code_search(request, context)
                .await?,
        );
    }
    if !reserved {
        refuse(
            provider,
            cached.iter().filter(|page| page.is_none()).count(),
            context,
        )?;
    }
    let mut seen = listed_earlier(provider, &searches, &units, context, &mut page).await;
    let mut unit_counts = Vec::with_capacity(requests.len());
    for (request, cached) in requests.iter().zip(cached) {
        let data = match cached {
            Some(data) => data,
            None => {
                provider
                    .transport
                    .search_code_uncached(request, context)
                    .await?
            }
        };
        unit_counts.push(data.total_count);
        page.add(data, &mut seen);
    }
    // Merged totals are every search's page-1 count, so each page of the
    // walk states the same layout; one search states its own page's.
    let counts = if merged && current > 1 {
        counts(&firsts)
    } else {
        unit_counts
    };
    for &count in &counts {
        page.total += SearchWindow::PageEnd.reach(count, per_search).total;
        page.matched += count;
    }
    page.pages = merged_pages(&reach_pages(&counts));
    Ok(page)
}

/// `search` at GitHub page `number`.
fn page_of(search: &CodeSearchRequest, number: usize) -> CodeSearchRequest {
    CodeSearchRequest {
        page: number,
        ..search.clone()
    }
}

/// Refuse a row whose `sends` uncached searches cannot all start within the
/// wait cap: each GitHub page takes its own window slot, so it is refused
/// before the first is sent, not midway with its slots spent.
fn refuse(
    provider: &GitHubProvider,
    sends: usize,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    match provider.transport.code_search_batch_refusal(sends, context) {
        Some(refusal) => Err(refusal),
        None => Ok(()),
    }
}

/// Page 1 of every search, which lays out a later merged page. Uncached
/// ones are read first; the pages they lay out are counted unseen, as all
/// uncached: `true` when that reserved the row's window slots.
async fn first_pages(
    provider: &GitHubProvider,
    searches: &[CodeSearchRequest],
    context: &RequestContext,
) -> Result<(Vec<Option<octocode_github::CodeSearchPage>>, bool), ProviderError> {
    let mut firsts = Vec::with_capacity(searches.len());
    for search in searches {
        firsts.push(
            provider
                .transport
                .cached_code_search(search, context)
                .await?,
        );
    }
    let missing = firsts.iter().filter(|first| first.is_none()).count();
    let reserved = missing > 0;
    if reserved {
        refuse(provider, missing + searches.len(), context)?;
    }
    for (search, first) in searches.iter().zip(&mut firsts) {
        if first.is_none() {
            *first = Some(
                provider
                    .transport
                    .search_code_uncached(search, context)
                    .await?,
            );
        }
    }
    Ok((firsts, reserved))
}

/// The files earlier pages already listed. GitHub's index moves between
/// page reads, so a file can come back on a later page: the earlier pages
/// of each listed search that are still cached (no request) say which
/// files the walk already delivered. Counts the pages read on `page`.
async fn listed_earlier(
    provider: &GitHubProvider,
    searches: &[CodeSearchRequest],
    units: &[(usize, usize)],
    context: &RequestContext,
    page: &mut SearchPage,
) -> std::collections::HashSet<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut checked = Vec::with_capacity(units.len());
    for &(search, number) in units {
        // A search's first page here is its lowest: the earlier ones precede it.
        if checked.contains(&search) {
            continue;
        }
        checked.push(search);
        for earlier in 1..number {
            page.earlier_total += 1;
            if let Ok(Some(listed)) = provider
                .transport
                .cached_code_search(&page_of(&searches[search], earlier), context)
                .await
            {
                page.earlier_checked += 1;
                seen.extend(listed.items.iter().map(repeat_key));
            }
        }
    }
    seen
}

/// A file's identity across pages: its repository and path (its blob `sha`
/// changes with the index state, so it is not part of it).
fn repeat_key(item: &octocode_github::CodeSearchItem) -> (String, String) {
    (item.repository.full_name.to_lowercase(), item.path.clone())
}

impl SearchPage {
    /// Take one GitHub page's files, less those `seen` already lists.
    fn add(
        &mut self,
        data: octocode_github::CodeSearchPage,
        seen: &mut std::collections::HashSet<(String, String)>,
    ) {
        self.incomplete |= data.incomplete_results;
        for organization in data.sso_partial_organizations {
            if !self.sso_partial.contains(&organization) {
                self.sso_partial.push(organization);
            }
        }
        self.fetched += data.items.len();
        for item in data.items {
            if seen.insert(repeat_key(&item)) {
                self.items.push(item);
            } else {
                self.dropped += 1;
            }
        }
    }

    /// What a paged page says about overlap: the repeats this page dropped,
    /// with how many earlier pages the check could read. That GitHub's
    /// totals are estimates is static and lives in the tool description.
    pub(super) fn paging_warnings(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.dropped > 0 {
            let files = if self.dropped == 1 { "file" } else { "files" };
            warnings.push(format!(
                "Dropped {} {files} GitHub listed again (already on an earlier page or this one).",
                self.dropped,
            ));
        }
        // Said beside a found repeat, to scope it: a page the caller jumped
        // to dropped nothing, and a missed repeat would list a row twice,
        // not lose one, so a clean or uncheckable page restates nothing.
        if self.dropped > 0 {
            warnings.push(format!(
                "Repeat check read {} of {} earlier page{}; pages no longer cached were not compared.",
                self.earlier_checked,
                self.earlier_total,
                if self.earlier_total == 1 { "" } else { "s" },
            ));
        }
        warnings
    }
}
