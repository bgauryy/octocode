//! ghSearchRepo: repository search, or an owner-only repository listing.
#[cfg(test)]
mod provider_tests;
mod query;

use crate::providers::github::{
    ConditionalCache, GitHubProvider, ProviderError, RepositorySearchPage, RepositorySearchRequest,
    RequestContext,
};
use crate::tools::gh_shared::{
    GhFailure, SEARCH_RESULT_CAP, add_next, apply_partial, reject_window, search_failure,
};
use crate::tools::id::ToolId;
use crate::tools::num::usize_of;
use crate::tools::result::{Continuation, ToolData, remove_null_fields};
use serde_json::{Value, json};

pub use crate::contracts::tool_types::{GhSearchRepoQuery, GhSearchRepoQuerySort};

/// Run one ghSearchRepo row.
pub async fn run<C: ConditionalCache>(
    provider: &GitHubProvider<C>,
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
            error.kind == crate::providers::github::ProviderErrorKind::NotFound
                && error.reason.is_none()
        });
        let failure = search_failure(
            error,
            "Lower page, or narrow with keywords, stars, created, or updated to reach deeper results.",
        );
        match missing_owner {
            Some(owner) => GhFailure {
                message: format!(
                    "Owner \"{owner}\" not found: no GitHub user or organization has this login"
                ),
                ..failure
            }
            .hint("Check the login's spelling, or find the repository with keywords."),
            None => failure,
        }
    })
}

pub(crate) async fn execute<C: ConditionalCache>(
    provider: &GitHubProvider<C>,
    query: &GhSearchRepoQuery,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    query::validate_repo_scope(query)?;
    let current = usize_of(query.page);
    let per = usize_of(query.page_size).min(crate::contracts::query_schema_max(
        ToolId::GhSearchRepo,
        None,
        "pageSize",
    ));
    let owner_only = owner_only(query);
    // The owner listing pages through the REST list API, which has no
    // 1,000-result search window; only search is bounded by it.
    let (data, listing) = match owner_only {
        Some(owner) => {
            let (data, listing) = owner_listing(provider, owner, current, per, context).await?;
            (data, Some(listing))
        }
        None => {
            reject_window(current, per)?;
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
            (data, None)
        }
    };
    let total = data.total_count.min(SEARCH_RESULT_CAP);
    let pages = total.div_ceil(per);
    let more = match &listing {
        Some(listing) => listing.more,
        None => current < pages,
    };
    let provider_incomplete = data.incomplete_results;
    // An owner listing reports no total (REST pages, no search window).
    let provider_matched = if listing.is_none() {
        data.total_count
    } else {
        0
    };
    let provider_capped = provider_matched > SEARCH_RESULT_CAP;
    let mut leads = top_leads(data.items.first());
    // Archived repositories are excluded unless the caller set `archived`:
    // a search states it once (page 1); a listing, whenever it skipped some.
    let archived_skipped = listing.as_ref().map_or(0, |listing| listing.archived);
    let excluded = query.archived.is_none()
        && match &listing {
            Some(_) => archived_skipped > 0,
            None => current == 1,
        };
    if excluded {
        leads.insert(
            leads.len().min(1),
            ("includeArchived", include_archived(query)),
        );
    }
    let repositories = if query.concise == Some(true) {
        data.items
            .into_iter()
            .map(|r| json!(r.full_name))
            .collect::<Vec<_>>()
    } else {
        let wanted = wanted_topics(query);
        data.items
            .into_iter()
            .map(|item| repository_row(item, &wanted))
            .collect::<Vec<_>>()
    };
    let repositories_empty = repositories.is_empty();
    let mut value = match &listing {
        // The REST listing reports no total: `page` is the provider page
        // cursor and `next.nextPage` follows the real Link header. The
        // listing is ordered by latest push; page counters are verbose
        // (core field class).
        Some(listing) => {
            let mut pagination =
                crate::response::pages::PageFacts::open(current, Some(per), more).to_value();
            pagination["providerPagesRead"] = json!(listing.last_page + 1 - current);
            json!({"repositories":repositories,"order":"pushed","pagination":pagination})
        }
        None => json!({"repositories":repositories,"pagination":
            crate::response::pages::PageFacts::counted(current, per, total).to_value()}),
    };
    let next_from = listing
        .as_ref()
        .map_or(current, |listing| listing.last_page);
    add_next(&mut value, ToolId::GhSearchRepo, query, next_from, more);
    for (name, lead) in leads {
        value["next"][name] = lead;
    }
    apply_partial(
        &mut value,
        ToolId::GhSearchRepo,
        query,
        provider_incomplete,
        provider_matched,
        current,
        more,
        "repository",
    );
    if archived_skipped > 0 {
        let warning = json!(format!(
            "Skipped {archived_skipped} archived repositories: hints.includeArchived lists them."
        ));
        match value.get_mut("warnings").and_then(Value::as_array_mut) {
            Some(warnings) => warnings.push(warning),
            None => value["warnings"] = json!([warning]),
        }
    }
    let mut output = repository_output(
        value,
        repositories_empty && !more,
        provider_incomplete,
        provider_capped,
    );
    if output.status == Some("empty")
        && let Some(broader) = broader_search(query)
    {
        output.data["next"]["findRepository"] = broader;
    }
    Ok(output)
}

/// The owner a query only lists: no term, topic, or filter beyond the owner,
/// in an order the listing API can serve.
fn owner_only(query: &GhSearchRepoQuery) -> Option<&str> {
    let unfiltered = query.keywords.is_empty()
        && query.topics.is_empty()
        && query.language.is_none()
        && query.stars.is_none()
        && query.pushed.is_none()
        && query.created.is_none()
        && query.match_.is_empty()
        && query.archived.is_none()
        && query.license.is_none()
        && query.qualifiers.is_none()
        && matches!(
            query.sort,
            GhSearchRepoQuerySort::BestMatch | GhSearchRepoQuerySort::Updated
        );
    query
        .owner
        .as_deref()
        .map(String::as_str)
        .filter(|_| unfiltered)
}

/// List an owner's repositories by latest push. The listing API cannot rank
/// by relevance, and its own default order is creation (oldest first).
/// Search excludes archived repositories by default (`archived:false`); the
/// listing API cannot, so it filters them and keeps reading provider pages
/// until a page of kept rows, the end of the listing (no Link next), or the
/// page budget.
async fn owner_listing<C: ConditionalCache>(
    provider: &GitHubProvider<C>,
    owner: &str,
    current: usize,
    per: usize,
    context: &RequestContext,
) -> Result<(RepositorySearchPage, OwnerListing), ProviderError> {
    let mut items = Vec::new();
    let mut archived = 0;
    let mut provider_page = current;
    let more = loop {
        let (batch, has_next) = provider
            .transport
            .list_owner_repositories(owner, "pushed", provider_page, per, context)
            .await?;
        let read = batch.len();
        let kept = items.len();
        items.extend(batch.into_iter().filter(|item| !item.archived));
        archived += read - (items.len() - kept);
        if !has_next || items.len() >= per || provider_page + 1 - current >= MAX_OWNER_LISTING_PAGES
        {
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
            more,
            archived,
        },
    ))
}

/// Where the top repository leads: its default-branch root listing. Repo
/// discovery words are not code keywords, so no code-search lead follows.
fn top_leads(
    top: Option<&crate::providers::github::RepositorySearchItem>,
) -> Vec<(&'static str, Value)> {
    let Some((owner, repo)) = top.and_then(|item| item.full_name.split_once('/')) else {
        return Vec::new();
    };
    vec![(
        "viewRepo",
        Continuation::new(ToolId::GhStructure, json!({"owner": owner, "repo": repo})).build(),
    )]
}

/// The same search with archived repositories included, from its first page.
fn include_archived(query: &GhSearchRepoQuery) -> Value {
    let mut row = serde_json::to_value(query).unwrap_or_else(|_| json!({}));
    if let Some(row) = row.as_object_mut() {
        // Defaults and the brief stay out of the copied call.
        row.retain(|key, value| {
            !matches!(
                key.as_str(),
                "page" | "debug" | "sort" | "pageSize" | "mainGoal" | "reasoning"
            ) && !value.is_null()
        });
    }
    if query.sort != GhSearchRepoQuerySort::BestMatch {
        row["sort"] = json!(query.sort.to_string());
    }
    row["archived"] = json!(true);
    Continuation::new(ToolId::GhSearchRepo, row)
        .why("Archived repositories are excluded by default.")
        .build()
}

/// An empty filtered search, rerun with its keywords (and owner) alone.
fn broader_search(query: &GhSearchRepoQuery) -> Option<Value> {
    let filtered = !query.topics.is_empty()
        || query.language.is_some()
        || query.stars.is_some()
        || query.pushed.is_some()
        || query.created.is_some()
        || !query.match_.is_empty()
        || query.license.is_some()
        || query.qualifiers.is_some();
    if !filtered || query.keywords.is_empty() {
        return None;
    }
    let mut broader = json!({"keywords": query.keywords});
    if let Some(owner) = query.owner.as_deref() {
        broader["owner"] = json!(owner.as_str());
    }
    Some(
        Continuation::new(ToolId::GhSearchRepo, broader)
            .why("Rerun the keywords without the filters that emptied the search.")
            .build(),
    )
}

/// Provider pages read by one owner-only listing call before it stops.
const MAX_OWNER_LISTING_PAGES: usize = 5;

struct OwnerListing {
    /// Last provider page read; the next cursor is the page after it.
    last_page: usize,
    /// The last page carried a Link `rel="next"`.
    more: bool,
    /// Archived repositories skipped on the provider pages read.
    archived: usize,
}

fn repository_output(
    value: Value,
    repositories_empty: bool,
    provider_incomplete: bool,
    provider_capped: bool,
) -> ToolData {
    let mut output = ToolData::from(value);
    if repositories_empty && !provider_incomplete && !provider_capped {
        output.status = Some("empty");
        output.data["hints"] = json!(["Broaden keywords or remove repository filters."]);
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

/// One compact repository row: `owner` and `repo`, the decision facts (stars,
/// language, license, last push, creation, `archived`/`fork` when set), the whole
/// description, and every topic (query matches first). Forks and the
/// metadata-update date are verbose (core field class).
fn repository_row(
    item: crate::providers::github::RepositorySearchItem,
    wanted: &[String],
) -> Value {
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
    let mut row = json!({
        "owner": owner,
        "repo": repo,
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
impl crate::tools::output::ToolOutput for Output {
    fn fallback_hint(&self, _query: &serde_json::Value) -> &'static str {
        "Broaden keywords or remove repository filters."
    }
    fn evidence_kind(&self, _query: &serde_json::Value, _data: &serde_json::Value) -> &'static str {
        "provider"
    }
    fn clasify_items(
        &self,
        _source: &crate::tools::clasify::resource::ResourceSource,
        state: &serde_json::Value,
    ) -> Option<Vec<crate::tools::clasify::items::Item>> {
        clasify_repositories(state)
    }
}

/// One clasify candidate per repository, read through ghStructure.
fn clasify_repositories(
    state: &serde_json::Value,
) -> Option<Vec<crate::tools::clasify::items::Item>> {
    use crate::tools::clasify::items;
    let found = items::page_data(state)?
        .get("repositories")?
        .as_array()?
        .iter()
        .filter_map(|repository| {
            let owner = repository.get("owner")?.as_str()?;
            let repo = repository.get("repo")?.as_str()?;
            let mut fetch = serde_json::Map::new();
            fetch.insert("owner".into(), json!(owner));
            fetch.insert("repo".into(), json!(repo));
            Some(items::Item {
                state: items::narrowed(
                    state,
                    "/results/0/data/repositories",
                    vec![repository.clone()],
                ),
                read: Some(items::read(ToolId::GhStructure, fetch)),
                path: None,
                item: Some(format!("{owner}/{repo}")),
            })
        })
        .collect();
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_repository_zero_is_empty_but_partial_zero_is_not() {
        let complete = repository_output(json!({"repositories":[]}), true, false, false);
        assert_eq!(complete.status, Some("empty"));
        assert!(complete.data["hints"][0].is_string());

        for (incomplete, capped) in [(true, false), (false, true)] {
            let partial = repository_output(json!({"repositories":[]}), true, incomplete, capped);
            assert_eq!(partial.status, None);
            assert!(partial.data.get("hints").is_none());
        }
    }

    /// A topic that only repeats the repository's own name is dropped;
    /// every other topic stays.
    #[test]
    fn topic_naming_the_repository_itself_is_dropped() {
        let item: crate::providers::github::RepositorySearchItem = serde_json::from_value(json!({
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
        let item: crate::providers::github::RepositorySearchItem = serde_json::from_value(json!({
            "full_name":"psf/requests","name":"requests","html_url":"h","default_branch":"main",
            "language":"Python","topics":["python","http"]
        }))
        .expect("item");
        let row = repository_row(item, &[]);
        assert_eq!(row["topics"], json!(["http"]), "{row}");
        assert_eq!(row["language"], "Python", "{row}");
    }
}
