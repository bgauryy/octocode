//! ghSearchRepo: repository search, or an owner-only repository listing.
#[cfg(test)]
mod provider_tests;
mod query;

use crate::id::ToolId;
use crate::num::usize_of;
use crate::response::paging::{clamp_page_size, push_warnings};
use crate::response::result::{Continuation, ToolData, query_row, remove_null_fields};
use crate::tools::gh_shared::{
    Broaden, GhFailure, SearchCompleteness, SearchWindow, add_next, apply_partial, broaden,
    field_names, pick_rows, rerun_renamed, search_failure, search_may_be_renamed,
    with_search_repair,
};
use octocode_github::{
    GitHubProvider, ProviderError, RepositorySearchPage, RepositorySearchRequest, RequestContext,
};
use serde_json::{Value, json};

pub use octocode_contract_types::{GhSearchRepoQuery, GhSearchRepoQuerySort};

/// Run one ghSearchRepo row.
pub async fn run(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    request: Result<&RequestContext, ProviderError>,
) -> Result<ToolData, GhFailure> {
    let result = match request {
        Ok(context) => execute(provider, query, context).await,
        Err(error) => Err(error),
    };
    result.map_err(|error| {
        // An owner listing's bare 404 is the login itself.
        let missing_owner = owner_only(query).filter(|_| {
            error.kind == octocode_github::ProviderErrorKind::NotFound && error.reason.is_none()
        });
        let failure = with_search_repair(
            search_failure(error, None),
            ToolId::GhSearchRepo,
            query,
            page_size(query),
            query.repo.as_deref().map(|repo| repo.as_str()),
        );
        // The message names the login; no repair lead exists for it.
        match missing_owner {
            Some(owner) => GhFailure {
                message: crate::tools::gh_shared::owner_not_found_message(owner),
                hints: Vec::new(),
                ..failure
            },
            None => failure,
        }
    })
}

/// Repositories per page.
fn page_size(query: &GhSearchRepoQuery) -> usize {
    clamp_page_size(ToolId::GhSearchRepo, None, usize_of(query.page_size))
}

pub(crate) async fn execute(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    if let (Some(owner), Some(repo)) = (query.owner.as_deref(), query.repo.as_deref())
        && !filtered(query)
    {
        return exact_repository(provider, query, owner, repo, context).await;
    }
    crate::tools::gh_shared::reject_boolean_keywords(&query.keywords)?;
    let current = usize_of(query.page);
    let per = page_size(query);
    let fetched = fetch_page(provider, query, current, per, context).await;
    // GitHub search does not follow renames: a search inside one
    // repository that a rename could explain reruns once under the
    // canonical name.
    if let (Some(owner), Some(repo)) = (query.owner.as_deref(), query.repo.as_deref())
        && search_may_be_renamed(&fetched, |(page, _)| {
            page.items.is_empty() && page.total_count == 0
        })
    {
        let rerun = |renamed: GhSearchRepoQuery| async move {
            Box::pin(execute(provider, &renamed, context)).await
        };
        if let Some(output) = rerun_renamed(provider, query, owner, repo, context, rerun).await? {
            return Ok(output);
        }
    }
    let (data, listing) = fetched?;
    let page = RepoPage::of(&data, listing.as_ref(), current, per);
    let mut leads = top_leads(data.items.first());
    // Archived repositories are excluded unless the caller set `includeArchived`;
    // a listing that skipped some offers them (the description states the
    // search default once).
    if query.include_archived != Some(true) && page.archived_skipped > 0 {
        leads.insert(
            leads.len().min(1),
            ("includeArchived", include_archived(query)),
        );
    }
    let wanted = wanted_topics(query);
    let repositories = data
        .items
        .into_iter()
        .map(|item| repository_row(item, &wanted))
        .collect::<Vec<_>>();
    let ignored_language = match &listing {
        None => ignored_language(provider, query, &repositories, page.matched, context).await,
        Some(_) => None,
    };
    let empty = repositories.is_empty();
    let mut value = page.value(repositories);
    add_next(
        &mut value,
        ToolId::GhSearchRepo,
        query,
        page.next_from,
        page.more,
    );
    for (name, lead) in leads {
        value["next"][name] = lead;
    }
    page.disclose(&mut value, query, ignored_language);
    pick_rows(
        &mut value,
        "repositories",
        REPO_ID,
        &field_names(&query.fields),
    );
    Ok(repository_output(
        value,
        query,
        empty && !page.more && !page.past_end,
        page.incomplete,
        page.matched > page.reachable,
    ))
}

/// One page of repositories: an owner's REST listing (no 1,000-result
/// search window, no total) or a search page.
async fn fetch_page(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    current: usize,
    per: usize,
    context: &RequestContext,
) -> Result<(RepositorySearchPage, Option<OwnerListing>), ProviderError> {
    if let Some(owner) = owner_only(query) {
        let (data, listing) = owner_listing(provider, owner, current, per, context).await?;
        return Ok((data, Some(listing)));
    }
    SearchWindow::PageStart.reject(current, per)?;
    let data = provider
        .transport
        .search_repositories(
            &RepositorySearchRequest {
                query: query::repositories(query),
                sort: (query.sort != GhSearchRepoQuerySort::BestMatch)
                    .then(|| query.sort.to_string()),
                page: current,
                per_page: per,
            },
            context,
        )
        .await?;
    Ok((data, None))
}

/// Where one page of repositories stands.
struct RepoPage {
    current: usize,
    per: usize,
    more: bool,
    /// The page the next one follows: the last provider page an owner
    /// listing read, else the requested page.
    next_from: usize,
    /// GitHub's own match count; an owner listing reports none.
    matched: usize,
    /// The search total within the 1,000-result window (a last page may
    /// run past 1,000).
    reachable: usize,
    incomplete: bool,
    /// An owner listing's REST pages read by this call.
    provider_pages: Option<usize>,
    archived_skipped: usize,
    /// No rows though the search (or the listing's earlier pages) has some:
    /// the shared response stage restarts it.
    past_end: bool,
}

impl RepoPage {
    fn of(
        data: &RepositorySearchPage,
        listing: Option<&OwnerListing>,
        current: usize,
        per: usize,
    ) -> Self {
        let reach = SearchWindow::PageStart.reach(data.total_count, per);
        let empty = data.items.is_empty() && current > 1;
        match listing {
            Some(listing) => Self {
                current,
                per,
                more: listing.more,
                next_from: listing.last_page,
                matched: 0,
                reachable: reach.total,
                incomplete: data.incomplete_results,
                provider_pages: Some(listing.pages_read),
                archived_skipped: listing.archived,
                past_end: empty && listing.archived == 0,
            },
            None => Self {
                current,
                per,
                more: current < reach.pages,
                next_from: current,
                matched: data.total_count,
                reachable: reach.total,
                incomplete: data.incomplete_results,
                provider_pages: None,
                archived_skipped: 0,
                past_end: empty && reach.past_last(current),
            },
        }
    }

    /// The rows and their `pagination`. The REST listing reports no total:
    /// `page` is the provider page cursor and `next.nextPage` follows the
    /// real Link header. The listing is ordered by latest push (the `owner`
    /// description says so); page counters are verbose (core field class).
    fn value(&self, repositories: Vec<Value>) -> Value {
        use crate::response::pages::PageFacts;
        let pagination = match self.provider_pages {
            Some(read) => {
                let mut pagination = PageFacts::open(self.current, Some(self.per), self.more)
                    .out_of_range(self.past_end)
                    .to_value();
                pagination["providerPagesRead"] = json!(read);
                pagination
            }
            None => PageFacts::counted(self.current, self.per, self.reachable)
                .out_of_range(self.past_end)
                .to_value(),
        };
        json!({"repositories": repositories, "pagination": pagination})
    }

    /// The provider's partial facts and the page's warnings: archived
    /// repositories a listing skipped, a language filter GitHub ignored.
    fn disclose(&self, value: &mut Value, query: &GhSearchRepoQuery, ignored: Option<String>) {
        apply_partial(
            value,
            query,
            &SearchCompleteness {
                tool: ToolId::GhSearchRepo,
                matched: self.matched,
                reachable: self.reachable,
                incomplete: self.incomplete,
                page: self.current,
                has_more: self.more,
                subject: "repository",
            },
        );
        if self.archived_skipped > 0 {
            push_warnings(
                value,
                [format!(
                    "{} skipped",
                    crate::response::pages::counted(
                        u64::try_from(self.archived_skipped).unwrap_or(u64::MAX),
                        "archived repository",
                        "archived repositories"
                    )
                )],
            );
        }
        push_warnings(value, ignored);
    }
}

/// `owner`+`repo` alone: that repository's row, with its default branch, from the
/// repository endpoint; no search request and no page.
async fn exact_repository(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    owner: &str,
    repo: &str,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let item = provider.transport.repository(owner, repo, context).await?;
    // GitHub answers a renamed repository's old name with the new one.
    let renamed = item
        .full_name
        .split_once('/')
        .filter(|(to_owner, to_repo)| {
            !to_owner.eq_ignore_ascii_case(owner) || !to_repo.eq_ignore_ascii_case(repo)
        })
        .map(|(to_owner, to_repo)| (to_owner.to_owned(), to_repo.to_owned()));
    let row = repository_row(item, &[]);
    let fields = field_names(&query.fields);
    let mut value = json!({ "repositories": [row] });
    pick_rows(&mut value, "repositories", REPO_ID, &fields);
    if let Some(to) = renamed {
        value["warnings"] = json!([crate::tools::gh_shared::renamed_warning(owner, repo, &to)]);
    }
    Ok(ToolData::from(value))
}

/// A repository row's id: a `fields` pick keeps it.
const REPO_ID: &[&str] = &["owner", "repo"];

/// The filters a query can set beside its keywords, by wire name, one
/// group each: the one table every narrowing check reads.
const FILTERS: &[&[&str]] = &[
    &["topics"],
    &["language"],
    &["stars"],
    &["pushed"],
    &["created"],
    &["matchIn"],
    &["license"],
    &["qualifiers"],
];

/// Whether the query sets any of [`FILTERS`].
fn sets_filter(query: &GhSearchRepoQuery) -> bool {
    let row = query_row(query, &[]);
    FILTERS
        .iter()
        .flat_map(|group| group.iter())
        .any(|key| row.get(*key).is_some())
}

/// GitHub drops a `language:` qualifier it does not know and returns
/// unfiltered rows. When no row on the page is that language, the filter
/// was ignored if the rows span several languages; if they share one (an
/// alias GitHub resolved, like `js` or `golang`, or unfiltered rows that
/// happen to agree), one more search without the qualifier decides: an
/// ignored qualifier leaves the count unchanged. That probe runs only on
/// such a page, so an alias costs one extra search request.
async fn ignored_language(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    rows: &[Value],
    matched: usize,
    context: &RequestContext,
) -> Option<String> {
    let wanted = query.language.as_deref()?;
    let languages = rows
        .iter()
        .filter_map(|row| row["language"].as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if languages.is_empty()
        || languages
            .iter()
            .any(|language| language.eq_ignore_ascii_case(wanted))
    {
        return None;
    }
    if languages.len() == 1 {
        // One other language: an alias GitHub resolved, or an ignored
        // filter whose rows happen to agree. An ignored qualifier changes
        // no count, so the same search without it tells them apart. The
        // probe is advisory: if it fails, nothing is claimed.
        let mut without = query.clone();
        without.language = None;
        let probe = provider
            .transport
            .search_repositories(
                &RepositorySearchRequest {
                    query: query::repositories(&without),
                    sort: None,
                    page: 1,
                    per_page: 1,
                },
                context,
            )
            .await
            .ok()?;
        if probe.total_count != matched {
            return None;
        }
    }
    Some(format!(
        "GitHub ignored language `{wanted}`, not a language name it knows: these rows are unfiltered."
    ))
}

/// Filters beside owner+repo search inside that repository; without them
/// the pair is a one-row lookup.
fn filtered(query: &GhSearchRepoQuery) -> bool {
    !query.keywords.is_empty()
        || sets_filter(query)
        // `archived:false` is the default, so it filters nothing.
        || query.include_archived == Some(true)
}

/// The owner a query only lists: no term, topic, or filter beyond the owner,
/// in an order the listing API can serve.
fn owner_only(query: &GhSearchRepoQuery) -> Option<&str> {
    let listable = !filtered(query)
        && matches!(
            query.sort,
            GhSearchRepoQuerySort::BestMatch | GhSearchRepoQuerySort::UpdatedDesc
        );
    query
        .owner
        .as_deref()
        .map(String::as_str)
        .filter(|_| listable)
}

/// List an owner's repositories by latest push. The listing API cannot rank
/// by relevance, and its own default order is creation (oldest first).
/// Search excludes archived repositories by default (`archived:false`); the
/// listing API cannot, so it filters them and keeps reading provider pages
/// until a page of kept rows, the end of the listing (no Link next), or the
/// page budget. A page never holds more than `per` rows: a provider page
/// whose kept rows would overflow it is left whole to the next call (the
/// cursor resumes at it), so no kept row is cut or shown twice.
async fn owner_listing(
    provider: &GitHubProvider,
    owner: &str,
    current: usize,
    per: usize,
    context: &RequestContext,
) -> Result<(RepositorySearchPage, OwnerListing), ProviderError> {
    let mut items = Vec::new();
    let mut archived = 0;
    let mut provider_page = current;
    let mut pages_read = 0;
    let more = loop {
        let (batch, has_next) = provider
            .transport
            .list_owner_repositories(owner, "pushed", provider_page, per, context)
            .await?;
        pages_read += 1;
        let read = batch.len();
        let kept = batch
            .into_iter()
            .filter(|item| !item.archived)
            .collect::<Vec<_>>();
        if !items.is_empty() && items.len() + kept.len() > per {
            provider_page -= 1;
            break true;
        }
        archived += read - kept.len();
        items.extend(kept);
        if !has_next || items.len() >= per || pages_read >= MAX_OWNER_LISTING_PAGES {
            break has_next;
        }
        provider_page += 1;
    };
    Ok((
        RepositorySearchPage {
            total_count: items.len(),
            incomplete_results: false,
            items,
        },
        OwnerListing {
            last_page: provider_page,
            pages_read,
            more,
            archived,
        },
    ))
}

/// Where the top repository leads: its default-branch root listing. Repo
/// discovery words are not code keywords, so no code-search lead follows.
fn top_leads(top: Option<&octocode_github::RepositorySearchItem>) -> Vec<(&'static str, Value)> {
    let Some((owner, repo)) = top.and_then(|item| item.full_name.split_once('/')) else {
        return Vec::new();
    };
    vec![(
        "viewRepo",
        crate::tools::gh_shared::RepoPath::root(owner, repo)
            .lead(ToolId::GhStructure)
            .build(),
    )]
}

/// The search a lead copies, from its first page: defaults and the brief
/// stay out of the copied call.
fn copied_search(query: &GhSearchRepoQuery) -> Value {
    let mut row = query_row(
        query,
        &[
            "page",
            "extras",
            "sort",
            "pageSize",
            "mainGoal",
            "reasoning",
        ],
    );
    if query.sort != GhSearchRepoQuerySort::BestMatch {
        row["sort"] = json!(query.sort.to_string());
    }
    row
}

/// The same search with archived repositories included, from its first page.
fn include_archived(query: &GhSearchRepoQuery) -> Value {
    let mut row = copied_search(query);
    row["includeArchived"] = json!(true);
    Continuation::new(ToolId::GhSearchRepo, row).build()
}

/// Broadened variants at most: one call's batch.
const BROADEN_VARIANTS: usize = 5;

/// An empty search, broadened ([`broaden`]): one variant per set filter,
/// then one per dropped keyword; a lone keyword of an `owner`+`repo` search
/// drops to that repository's own row.
fn broaden_search(query: &GhSearchRepoQuery) -> Option<Value> {
    broaden(
        ToolId::GhSearchRepo,
        &copied_search(query),
        &Broaden {
            strip: &[],
            groups: FILTERS,
        },
        BROADEN_VARIANTS,
    )
}

/// Provider pages read by one owner-only listing call before it stops.
const MAX_OWNER_LISTING_PAGES: usize = 5;

struct OwnerListing {
    /// Last provider page listed; the next cursor is the page after it.
    last_page: usize,
    /// Provider pages this call read (a page left to the next call too).
    pages_read: usize,
    /// The last page carried a Link `rel="next"`.
    more: bool,
    /// Archived repositories skipped on the provider pages read.
    archived: usize,
}

fn repository_output(
    value: Value,
    query: &GhSearchRepoQuery,
    repositories_empty: bool,
    provider_incomplete: bool,
    provider_capped: bool,
) -> ToolData {
    let mut output = ToolData::from(value);
    if repositories_empty && !provider_incomplete && !provider_capped {
        output.status = Some("empty");
        if let Some(broader) = broaden_search(query) {
            output.data["next"]["broadenSearch"] = broader;
        }
    }
    output
}

fn date(value: Option<String>) -> Option<String> {
    value.map(|v| v.chars().take(10).collect())
}

/// Lowercased query topics and keyword words: a row lists these topics first.
fn wanted_topics(query: &GhSearchRepoQuery) -> Vec<String> {
    query
        .topics
        .iter()
        .map(|topic| topic.to_lowercase())
        .chain(query.keywords.iter().flat_map(|keyword| {
            let keyword = keyword.to_lowercase();
            let hyphenated = keyword.split_whitespace().collect::<Vec<_>>().join("-");
            keyword
                .split_whitespace()
                .map(str::to_owned)
                .chain(std::iter::once(hyphenated))
                .collect::<Vec<_>>()
        }))
        .collect()
}

/// One compact repository row: `owner`, `repo` and `defaultBranch` (the
/// next hop's coordinates; a page whose rows agree states the branch once in
/// `shared`), the decision facts by default (stars, language, last push,
/// `archived`/`fork` when set) and the whole description. The license, the
/// creation date and every topic (query matches first) are `details`
/// extras; the fork count and the metadata-update date are `stats` extras
/// (the contract's extra paths decide what a response keeps).
fn repository_row(item: octocode_github::RepositorySearchItem, wanted: &[String]) -> Value {
    let mut topics = item.topics;
    // A topic equal to the repository's own name repeats `repo`; one equal
    // to its language repeats `language`.
    topics.retain(|topic| {
        !topic.eq_ignore_ascii_case(&item.name)
            && item
                .language
                .as_deref()
                .is_none_or(|language| !topic.eq_ignore_ascii_case(language))
    });
    // Stable: matching topics keep GitHub's order, then the rest.
    topics.sort_by_key(|topic| !wanted.contains(&topic.to_lowercase()));
    let (owner, repo) = item
        .full_name
        .split_once('/')
        .map_or((None, item.name.clone()), |(owner, repo)| {
            (Some(owner.to_owned()), repo.to_owned())
        });
    let default_branch = Some(item.default_branch).filter(|branch| !branch.is_empty());
    let mut row = json!({
        "owner": owner,
        "repo": repo,
        "defaultBranch": default_branch,
        "stars": item.stargazers_count,
        "language": item.language,
        "license": item.license.and_then(|license| license.spdx_id),
        "pushedAt": date(item.pushed_at),
        "createdAt": date(item.created_at),
        "description": item.description,
    });
    if item.archived {
        row["archived"] = json!(true);
    }
    // A fork in an owner listing is not the owner's own source.
    if item.fork {
        row["fork"] = json!(true);
    }
    if !topics.is_empty() {
        row["topics"] = json!(topics);
    }
    row["forks"] = json!(item.forks_count);
    row["updatedAt"] = json!(date(item.updated_at));
    remove_null_fields(&mut row);
    row
}

/// This tool's output facts for the shared response stages.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    /// A search with a filter to drop leads to `broadenSearch` instead.
    fn fallback_hint(&self, _query: &serde_json::Value) -> &'static str {
        "Use fewer or broader keywords, or set owner and repo to open one."
    }
    /// A missing owner names the login and has no repair; an exact
    /// repository that did not resolve carries its own `findRepository`
    /// lead. Neither is helped by the search fallback.
    fn error_hint(&self, code: &str) -> Option<&'static str> {
        (code == "notFound").then_some(crate::response::output::NO_HINT)
    }
    fn evidence_kind(&self, _query: &serde_json::Value, _data: &serde_json::Value) -> &'static str {
        "provider"
    }
}

#[cfg(test)]
mod tests {
    /// A missing owner is not told to broaden its keywords: the error names
    /// the login, and no search change repairs it.
    #[test]
    fn a_missing_owner_gets_no_search_fallback_hint() {
        use crate::id::ToolId;
        let query = serde_json::json!({"owner": "no-such-owner"});
        let mut row = serde_json::json!({"status": "error", "data": {
            "errorCode": "notFound",
            "error": "Owner \"no-such-owner\" not found: no GitHub user or organization has this login"
        }});
        crate::response::rows::ensure_recovery(&mut row, ToolId::GhSearchRepo, &query);
        assert!(row["data"].get("hints").is_none(), "{row}");
        // An empty search still gets its fallback.
        let mut empty = serde_json::json!({"status": "empty", "data": {}});
        crate::response::rows::ensure_recovery(&mut empty, ToolId::GhSearchRepo, &query);
        assert!(empty["data"]["hints"][0].is_string(), "{empty}");
    }

    use super::*;

    #[test]
    fn complete_repository_zero_is_empty_but_partial_zero_is_not() {
        let query: GhSearchRepoQuery =
            serde_json::from_value(json!({"keywords":["x"],"language":"rust"})).expect("query");
        let complete = repository_output(json!({"repositories":[]}), &query, true, false, false);
        assert_eq!(complete.status, Some("empty"));
        assert!(complete.data.get("hints").is_none());
        assert_eq!(
            complete.data["next"]["broadenSearch"]["query"]["queries"],
            json!([{"keywords":["x"]}, {"language":"rust"}])
        );

        for (incomplete, capped) in [(true, false), (false, true)] {
            let partial =
                repository_output(json!({"repositories":[]}), &query, true, incomplete, capped);
            assert_eq!(partial.status, None);
            assert!(partial.data.get("hints").is_none());
            assert!(partial.data.get("next").is_none());
        }
    }

    /// An empty search inside one repository with a lone keyword broadens
    /// to that repository's own row; each filter drops by its wire name.
    #[test]
    fn a_lone_keyword_in_one_repository_broadens_to_its_row() {
        let query: GhSearchRepoQuery =
            serde_json::from_value(json!({"owner":"o","repo":"r","keywords":["x"]}))
                .expect("query");
        let empty = repository_output(json!({"repositories":[]}), &query, true, false, false);
        assert_eq!(
            empty.data["next"]["broadenSearch"]["query"]["queries"],
            json!([{"owner":"o","repo":"r"}]),
            "{}",
            empty.data
        );
        let matched: GhSearchRepoQuery =
            serde_json::from_value(json!({"keywords":["x"],"matchIn":["name"]})).expect("query");
        let empty = repository_output(json!({"repositories":[]}), &matched, true, false, false);
        assert_eq!(
            empty.data["next"]["broadenSearch"]["query"]["queries"],
            json!([{"keywords":["x"]}]),
            "{}",
            empty.data
        );
    }

    /// A topic that only repeats the repository's own name is dropped;
    /// every other topic stays.
    #[test]
    fn topic_naming_the_repository_itself_is_dropped() {
        let item: octocode_github::RepositorySearchItem = serde_json::from_value(json!({
            "full_name":"harlanc/xiu","name":"xiu","html_url":"h","default_branch":"main",
            "topics":["rtmp","XIU","rust"]
        }))
        .expect("item");
        let row = repository_row(item, &[]);
        assert_eq!(row["topics"], json!(["rtmp", "rust"]), "{row}");
    }

    /// A topic that only repeats the row's `language` is dropped too.
    #[test]
    fn topic_naming_the_language_is_dropped() {
        let item: octocode_github::RepositorySearchItem = serde_json::from_value(json!({
            "full_name":"psf/requests","name":"requests","html_url":"h","default_branch":"main",
            "language":"Python","topics":["python","http"]
        }))
        .expect("item");
        let row = repository_row(item, &[]);
        assert_eq!(row["topics"], json!(["http"]), "{row}");
        assert_eq!(row["language"], "Python", "{row}");
    }
}
