//! One merged page across the `extensions` searches, with repeats from earlier pages dropped.
use super::listed::{Listed, Seen};
use super::query::{self, PathAs};
use super::{GhSearchCodeQuery, GhSearchCodeQueryMatchInItem, Withholds, per_search};
use octocode_github::{
    CodeSearchItem, CodeSearchPage, CodeSearchRequest, ContentRequest, GitHubProvider,
    ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext, SearchRepository,
};
use octocode_tools_shared::gh_shared::SearchWindow;
use octocode_tools_shared::gh_shared::text::file_text;

/// One merged page across `extensions`: GitHub ANDs repeated `extension:`
/// qualifiers, so each extension is its own indexed search of
/// `floor(limit / k)` files a GitHub page ([`per_search`]). Page 1 lists
/// page 1 of every search; their totals lay out the later pages, which take
/// the searches' remaining GitHub pages `k` at a time, page by page in
/// extension order ([`later_pages`]). A search that has run out is never
/// asked for a later page, a page holds at most `limit` files while every
/// search lists more than one a page, and every GitHub page is on exactly
/// one merged page: lossless. One search's page N is its GitHub page N. A later page
/// lays itself out from the totals its continuation carries ([`Listed`]),
/// refreshed by each search page it reads; a page asked for directly has no
/// carried totals and reads page 1 of every search for them.
pub(super) struct SearchPage {
    /// The files this page lists: GitHub's, less those an earlier page of
    /// the walk already listed (with those an earlier part of this page
    /// showed, [`SearchPage::was_shown`]).
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
    /// The file a repo-scoped `path` names when the page was answered from
    /// its read, with no code search ([`file_page`]).
    pub(super) read: Option<FileRead>,
}

/// A repo-scoped `path` read in place of a code search ([`probe_path`]).
pub(super) struct FileRead {
    /// The commit it was read at.
    pub(super) commit: String,
    /// The path read: the row's own, or the path the commit holds in another
    /// case of its parts ([`path_in_any_case`]).
    pub(super) path: String,
    /// Whether a file is at `path`: false when the default branch's head
    /// holds nothing at the row's path in any case of its parts.
    pub(super) found: bool,
    /// The deepest folder on the row's path the commit holds
    /// (case-corrected; `""` is the root): the listing a miss routes to.
    pub(super) directory: String,
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

/// Merged page `current` of the search. A `path` names a folder or a file,
/// and GitHub's `path:` matches only a folder. A repo-scoped `path` is read
/// first ([`probe_path`]): a file answers the row from its own bytes with no
/// code search, a folder runs the one folder search. A path the read cannot
/// type searches as a folder, then, when that finds nothing, once more as
/// the file it names ([`PathAs::File`]).
pub(super) async fn search_page(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    current: usize,
    per: usize,
    context: &RequestContext,
    withholds: Withholds<'_>,
) -> Result<SearchPage, ProviderError> {
    let kind = probe_path(provider, query, current, context, withholds).await?;
    if let PathKind::File(page) = kind {
        return Ok(*page);
    }
    let folder = search_as(provider, query, PathAs::Folder, current, per, context).await?;
    if query.path.is_none()
        || folder.total > 0
        || folder.fetched > 0
        || matches!(kind, PathKind::Folder)
    {
        return Ok(folder);
    }
    search_as(provider, query, PathAs::File, current, per, context).await
}

/// What a row's `path` names, as one read of it says.
enum PathKind {
    /// A file, or nothing at the default branch's head: the page answered
    /// from that read ([`file_page`]).
    File(Box<SearchPage>),
    /// A folder: the folder search alone answers it.
    Folder,
    /// No repo-scoped path, or a read that cannot type it (missing at a
    /// requested ref, withheld, binary, over the scan bound, a failed read):
    /// the searches decide.
    Undecided,
}

/// How one read of a path at a commit types it.
enum Read {
    /// A file, with its text and blob.
    File(String, Option<String>),
    Folder,
    /// Nothing at the path at that commit.
    Missing,
    /// Not typed: withheld, binary, over the scan bound, or a failed read.
    Unread,
}

/// Read a repo-scoped row's `path` at its `ref` (the default branch when it
/// has none) before any code search. GitHub's `path:` never matches a
/// file's own path, so a searched file costs a folder search and a file
/// search of the code-search quota (10 a minute); its bytes answer the same
/// row from the core quota: the file matches when it holds every keyword
/// ([`file_matches`]), and its lines are then listed from the same read. A
/// path missing at the default branch's head, the one branch the index
/// covers, is matched as the code search matches it, in any case of its
/// parts ([`path_in_any_case`]); with no such path the row is answered empty
/// from that read.
async fn probe_path(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    current: usize,
    context: &RequestContext,
    withholds: Withholds<'_>,
) -> Result<PathKind, ProviderError> {
    let (Some(repo), Some(path)) = (
        query.repo.as_deref(),
        query.path.as_deref().map(|path| path.trim_matches('/')),
    ) else {
        return Ok(PathKind::Undecided);
    };
    // A path the policy withholds is never read: the searches list it by
    // path only, as every hit of it is listed.
    if path.is_empty() || withholds(path) {
        return Ok(PathKind::Undecided);
    }
    let owner = query.owner.as_str();
    let reference = super::code_output::requested_ref(query);
    let commit = match provider
        .resolve_reference(owner, repo, reference, context)
        .await
    {
        Ok(commit) => commit,
        Err(error) if error.is_cancelled_or_timed_out() => return Err(error),
        Err(_) => return Ok(PathKind::Undecided),
    };
    let at = Target {
        owner,
        repo,
        commit: &commit,
    };
    let (path, read) = match read_path(provider, at, path, context, withholds).await? {
        Read::Missing if reference.is_none() => {
            match path_in_any_case(provider, at, path, context).await {
                Case::Absent(directory) => {
                    let read = FileRead {
                        commit: commit.clone(),
                        path: path.to_owned(),
                        found: false,
                        directory,
                    };
                    return Ok(PathKind::File(Box::new(file_page(
                        query, current, None, read,
                    ))));
                }
                Case::Unknown => return Ok(PathKind::Undecided),
                Case::Folder => return Ok(PathKind::Folder),
                // The listed name is read as the row's own path is.
                Case::File(listed) if !withholds(&listed) => {
                    let read = read_path(provider, at, &listed, context, withholds).await?;
                    (listed, read)
                }
                Case::File(_) => return Ok(PathKind::Undecided),
            }
        }
        read => (path.to_owned(), read),
    };
    let (text, blob) = match read {
        Read::File(text, blob) => (text, blob),
        Read::Folder => return Ok(PathKind::Folder),
        Read::Missing | Read::Unread => return Ok(PathKind::Undecided),
    };
    let path = path.as_str();
    let hit = file_matches(query, path, &text).then(|| CodeSearchItem {
        name: path.rsplit('/').next().unwrap_or(path).to_owned(),
        path: path.to_owned(),
        sha: blob.unwrap_or_default(),
        // The commit rides `html_url` as an indexed hit's does, so the
        // row's lines are read from the same commit.
        html_url: format!("https://github.com/{owner}/{repo}/blob/{commit}/{path}"),
        repository: SearchRepository {
            full_name: format!("{owner}/{repo}"),
            html_url: format!("https://github.com/{owner}/{repo}"),
            url: String::new(),
            default_branch: String::new(),
        },
        text_matches: Vec::new(),
    });
    let read = FileRead {
        commit: commit.clone(),
        path: path.to_owned(),
        found: true,
        directory: String::new(),
    };
    Ok(PathKind::File(Box::new(file_page(
        query, current, hit, read,
    ))))
}

/// The repository and commit a probe reads at.
#[derive(Clone, Copy)]
struct Target<'a> {
    owner: &'a str,
    repo: &'a str,
    commit: &'a str,
}

/// One read of `path` at the target's commit, typed ([`Read`]).
async fn read_path(
    provider: &GitHubProvider,
    at: Target<'_>,
    path: &str,
    context: &RequestContext,
    withholds: Withholds<'_>,
) -> Result<Read, ProviderError> {
    let request = ContentRequest {
        owner: at.owner.to_owned(),
        repo: at.repo.to_owned(),
        path: path.to_owned(),
        reference: Some(at.commit.to_owned()),
    };
    let bounded = super::lines::read_bound(context);
    let read = match provider
        .get_file_bytes_following_links(&request, &bounded, withholds)
        .await
    {
        Ok(Ok((content, _))) => content,
        // A withheld target lists its path only, as a searched hit does.
        Ok(Err(_)) => return Ok(Read::Unread),
        Err(error) if error.is_cancelled_or_timed_out() => return Err(error),
        Err(error) if error.reason == Some(ProviderErrorReason::PathIsDirectory) => {
            return Ok(Read::Folder);
        }
        Err(error) if error.kind == ProviderErrorKind::NotFound => return Ok(Read::Missing),
        Err(_) => return Ok(Read::Unread),
    };
    Ok(match file_text(&read.bytes) {
        Ok(text) => Read::File(text.into_owned(), read.blob),
        Err(_) => Read::Unread,
    })
}

/// What the commit holds at `path` in any case of its parts.
enum Case {
    /// A file at that path in another case, by its path.
    File(String),
    /// A folder at that path in another case: the folder search, which
    /// matches a path in any case, answers it.
    Folder,
    /// No such file or folder at the commit: the deepest folder on the path
    /// it holds (case-corrected).
    Absent(String),
    /// No folder listing settled it (a failed or truncated tree): the
    /// searches decide.
    Unknown,
}

/// What the commit holds at `path` in any case of its parts, as GitHub's
/// code search matches a path: the shared walk
/// ([`octocode_tools_shared::gh_shared::locate_path_at`]) reads the path's
/// folder tree, the one a read's link check already asked for, and climbs
/// only while that folder is missing, so a miss names the folder a listing
/// will find.
async fn path_in_any_case(
    provider: &GitHubProvider,
    at: Target<'_>,
    path: &str,
    context: &RequestContext,
) -> Case {
    let place = octocode_tools_shared::gh_shared::RepoPath {
        owner: at.owner,
        repo: at.repo,
        path,
        reference: Some(at.commit),
    };
    let Some(found) =
        octocode_tools_shared::gh_shared::locate_path_at(provider, &place, at.commit, context)
            .await
    else {
        return Case::Unknown;
    };
    if let Some(file) = found.file {
        return Case::File(file);
    }
    if found.truncated {
        return Case::Unknown;
    }
    let parts = |path: &str| path.split('/').filter(|part| !part.is_empty()).count();
    if parts(&found.directory) == parts(path) {
        return Case::Folder;
    }
    Case::Absent(found.directory)
}

/// The page of a row answered from the file its `path` names: that file
/// alone on page 1 when it matched, nothing on a later page.
fn file_page(
    query: &GhSearchCodeQuery,
    current: usize,
    hit: Option<CodeSearchItem>,
    read: FileRead,
) -> SearchPage {
    let found = usize::from(hit.is_some());
    let mut page = SearchPage {
        items: Vec::new(),
        fetched: 0,
        dropped: 0,
        listed: Listed::of(query.listed.as_deref().map(String::as_str), 1),
        total: found,
        pages: found,
        matched: found,
        incomplete: false,
        sso_partial: Vec::new(),
        read: Some(read),
    };
    if current == 1 {
        page.add(CodeSearchPage {
            total_count: found,
            incomplete_results: false,
            items: hit.into_iter().collect(),
            sso_partial_organizations: Vec::new(),
        });
    }
    page
}

/// Whether the file at `path` holds the row's terms as the code index
/// matches them: its name passes `extensions` and `nameContains`, and every
/// keyword (case-insensitive; a phrase whole) is in its text or its path,
/// as `matchIn` says.
fn file_matches(query: &GhSearchCodeQuery, path: &str, text: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    if !query.extensions.is_empty()
        && !query.extensions.iter().any(|wanted| {
            Some(
                wanted
                    .as_str()
                    .trim_start_matches('.')
                    .to_lowercase()
                    .as_str(),
            ) == extension
        })
    {
        return false;
    }
    if let Some(part) = query.name_contains.as_ref()
        && !name.contains(&part.as_str().to_lowercase())
    {
        return false;
    }
    let content = query
        .match_in
        .contains(&GhSearchCodeQueryMatchInItem::Content)
        .then(|| text.to_lowercase());
    let in_path = query
        .match_in
        .contains(&GhSearchCodeQueryMatchInItem::Path)
        .then(|| path.to_lowercase());
    query.keywords.iter().all(|keyword| {
        let keyword = keyword.as_str().to_lowercase();
        content
            .as_deref()
            .is_some_and(|text| text.contains(&keyword))
            || in_path
                .as_deref()
                .is_some_and(|path| path.contains(&keyword))
    })
}

/// [`search_page`] with `path` read as `path_as`.
async fn search_as(
    provider: &GitHubProvider,
    query: &GhSearchCodeQuery,
    path_as: PathAs,
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
        read: None,
    };
    let searches = scopes
        .into_iter()
        .map(|extension| CodeSearchRequest {
            query: query::code(query, extension, path_as),
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
        let mut data = provider.transport.search_code(request, context).await?;
        totals[search] = data.total_count;
        data.items.retain(|item| path_as.keeps(query, &item.path));
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
            match self.listed.insert(&item) {
                Seen::New | Seen::Shown => self.items.push(item),
                Seen::Repeat => self.dropped += 1,
            }
        }
    }

    /// Whether an earlier part of this page showed `row` (its `owner`,
    /// `repo` and `path`): the page lists it again only to rank the rest.
    pub(super) fn was_shown(&self, row: &serde_json::Value) -> bool {
        let field = |name: &str| row.get(name).and_then(serde_json::Value::as_str);
        match (field("owner"), field("repo"), field("path")) {
            (Some(owner), Some(repo), Some(path)) => {
                self.listed.was_shown(&format!("{owner}/{repo}"), path)
            }
            _ => false,
        }
    }

    /// What a walk's page says when GitHub's results moved since the
    /// previous page (`before`, the match count it stated): `None` when
    /// nothing moved; else the warnings beside the `providerResultsShifted`
    /// reason. A file listed again (dropped here) loses no evidence and the
    /// reason states the shift, so only a changed match count is worded, with
    /// both counts: the walk keeps no ranks, so it cannot tell whether a file
    /// moved past it.
    pub(super) fn shift_warnings(&self, before: Option<usize>) -> Option<Vec<String>> {
        let changed = before.filter(|&before| before != self.matched);
        if self.dropped == 0 && changed.is_none() {
            return None;
        }
        Some(
            changed
                .map(|before| {
                    format!(
                        "GitHub's match count changed from {before} to {} between pages; with no ranks, the walk cannot tell whether a file moved past it; re-walk to confirm.",
                        self.matched,
                    )
                })
                .into_iter()
                .collect(),
        )
    }
}
