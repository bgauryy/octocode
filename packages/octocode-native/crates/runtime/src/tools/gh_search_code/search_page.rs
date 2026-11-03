//! One merged page across the `extensions` searches, with repeats from earlier pages dropped.
use super::listed::Listed;
use super::{GhSearchCodeQuery, GhSearchCodeQueryMatchInItem, per_search, query};
use crate::tools::gh_shared::SearchWindow;
use octocode_github::{CodeSearchRequest, GitHubProvider, ProviderError, RequestContext};

/// One merged page across `extensions`: GitHub ANDs repeated `extension:`
/// qualifiers, so each extension is its own indexed search of
/// `ceil(limit / k)` files a GitHub page ([`per_search`]). Page 1 lists
/// page 1 of every search; their totals lay out the later pages, which take
/// the searches' remaining GitHub pages `k` at a time, page by page in
/// extension order ([`later_pages`]). A search that has run out is never
/// asked for a later page, a page stays about `limit` files while any
/// search has files left, and every GitHub page is on exactly one merged
/// page: lossless. One search's page N is its GitHub page N. A later page
/// lays itself out from the totals its continuation carries ([`Listed`]),
/// refreshed by each search page it reads; a page asked for directly has no
/// carried totals and reads page 1 of every search for them.
pub(super) struct SearchPage {
    /// The files this page lists: GitHub's, less those an earlier page of
    /// the walk already listed.
    pub(super) items: Vec<octocode_github::CodeSearchItem>,
    /// Files GitHub returned for the page, before repeats were dropped.
    pub(super) fetched: usize,
    /// Repeats dropped: files an earlier page (or this one) already listed.
    pub(super) dropped: usize,
    /// Every file the walk has listed through this page ([`Listed`]).
    pub(super) listed: Listed,
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
        listed: Listed::of(query.listed.as_deref().map(String::as_str), scopes.len()),
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
    // A later merged page is laid out by every search's match count: the
    // walk's carried counts, else page 1 of every search, read now.
    let (mut totals, reserved) = match page.listed.totals() {
        Some(carried) if merged && current > 1 => (carried.to_vec(), false),
        _ if merged && current > 1 => first_pages(provider, &searches, context).await?,
        _ => (vec![0; searches.len()], false),
    };
    let reach_pages = |counts: &[usize]| {
        counts
            .iter()
            .map(|&count| SearchWindow::PageEnd.reach(count, per_search).pages)
            .collect::<Vec<_>>()
    };
    let units = if merged && current > 1 {
        later_pages(&reach_pages(&totals), current)
    } else {
        (0..searches.len())
            .map(|search| (search, current))
            .collect()
    };
    let requests = units
        .iter()
        .map(|&(search, number)| at(search, number))
        .collect::<Vec<_>>();
    if !reserved {
        refuse(provider, &requests, 0, context)?;
    }
    for (&(search, _), request) in units.iter().zip(&requests) {
        let data = provider.transport.search_code(request, context).await?;
        totals[search] = data.total_count;
        page.add(data);
    }
    for &count in &totals {
        page.total += SearchWindow::PageEnd.reach(count, per_search).total;
        page.matched += count;
    }
    page.pages = merged_pages(&reach_pages(&totals));
    if merged {
        page.listed.set_totals(totals);
    }
    Ok(page)
}

/// `search` at GitHub page `number`.
fn page_of(search: &CodeSearchRequest, number: usize) -> CodeSearchRequest {
    CodeSearchRequest {
        page: number,
        ..search.clone()
    }
}

/// Refuse a row whose searches cannot all start before the deadline: each
/// GitHub page draws on the code-search quota, so it is refused before the
/// first is sent, not midway with the quota spent. Only the `requests` the
/// continuation's journal cannot answer count, with `later` more reads the
/// journal has not seen.
fn refuse(
    provider: &GitHubProvider,
    requests: &[CodeSearchRequest],
    later: usize,
    context: &RequestContext,
) -> Result<(), ProviderError> {
    let sends = provider
        .transport
        .code_search_unanswered(requests, context)?
        + later;
    match provider.transport.code_search_batch_refusal(sends, context) {
        Some(refusal) => Err(refusal),
        None => Ok(()),
    }
}

/// Page 1 of every search lays out a later merged page. Reserve admission
/// for these reads and the later page before spending the first search slot.
async fn first_pages(
    provider: &GitHubProvider,
    searches: &[CodeSearchRequest],
    context: &RequestContext,
) -> Result<(Vec<usize>, bool), ProviderError> {
    refuse(provider, searches, searches.len(), context)?;
    let mut totals = Vec::with_capacity(searches.len());
    for search in searches {
        totals.push(
            provider
                .transport
                .search_code(search, context)
                .await?
                .total_count,
        );
    }
    Ok((totals, true))
}

impl SearchPage {
    /// Take one GitHub page's files, less those the walk already listed.
    fn add(&mut self, data: octocode_github::CodeSearchPage) {
        self.incomplete |= data.incomplete_results;
        for organization in data.sso_partial_organizations {
            if !self.sso_partial.contains(&organization) {
                self.sso_partial.push(organization);
            }
        }
        self.fetched += data.items.len();
        for item in data.items {
            if self.listed.insert(&item) {
                self.items.push(item);
            } else {
                self.dropped += 1;
            }
        }
    }

    /// What a walk's page says when GitHub's results moved since the
    /// previous page (`before`, the match count it stated): a file listed
    /// again (dropped here) or a changed match count. Either way a file may
    /// also have moved past the walk unseen; `None` when neither was seen.
    pub(super) fn shift_warnings(&self, before: Option<usize>) -> Option<Vec<String>> {
        let changed = before.filter(|&before| before != self.matched);
        if self.dropped == 0 && changed.is_none() {
            return None;
        }
        let mut warnings = Vec::new();
        if self.dropped > 0 {
            let files = if self.dropped == 1 { "file" } else { "files" };
            warnings.push(format!(
                "Dropped {} {files} GitHub listed again (already on an earlier page of this walk or this one).",
                self.dropped,
            ));
        }
        if let Some(before) = changed {
            warnings.push(format!(
                "GitHub's match count changed from {before} to {} since the previous page.",
                self.matched,
            ));
        }
        warnings.push(
            "GitHub's results moved between page reads, so a file may have moved past this walk unseen."
                .to_owned(),
        );
        Some(warnings)
    }
}
