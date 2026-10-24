//! The provider loads of one pull-request read: the PR itself and its
//! collections (GraphQL first page or REST windows), concurrently.
use super::PrPlan;
use crate::num::usize_of;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::graphql::{
    GraphqlCollection, GraphqlPr, map_graphql_comments, map_graphql_commits, map_graphql_files,
    map_graphql_reviews,
};
use crate::tools::gh_get_history_item::inventory::file_page_size;
use crate::tools::gh_get_history_item::sections::Section;
use crate::tools::gh_get_history_item::text::body_matches;
use crate::tools::gh_get_history_item::window::{
    Batch, Loaded, MAX_COLLECTION_BATCHES, MAX_FILE_BATCHES, MAX_PR_COMMIT_BATCHES, WindowSpec,
    load_window,
};
use crate::tools::gh_get_history_item::{HistoryItemRequest, Sections, fetch};
use octocode_github::{GitHubTransport, MAX_PER_PAGE, ProviderError, RequestContext};
use serde_json::Value;

/// One PR collection: what GraphQL served (the whole list, or a first page
/// it covers), otherwise the REST window covering the requested public page.
async fn load_collection(
    transport: &GitHubTransport,
    wanted: bool,
    graphql: Option<Loaded>,
    segments: &[&str],
    spec: WindowSpec,
    keep: impl Fn(&Value) -> bool,
    context: &RequestContext,
) -> Result<Option<Loaded>, ProviderError> {
    if !wanted {
        return Ok(None);
    }
    if let Some(served) = graphql {
        return Ok(Some(served));
    }
    load_window(transport, segments, spec, keep, context)
        .await
        .map(Some)
}

/// The provider data one pull-request read loaded.
pub(super) struct PrLoads {
    pub(super) raw: Value,
    pub(super) files: Option<Loaded>,
    pub(super) discussion: Option<Loaded>,
    pub(super) inline: Option<Loaded>,
    pub(super) reviews: Option<Loaded>,
    pub(super) commits: Option<Loaded>,
}

/// Requests that run before the collections: the PR itself, beside the
/// first batch of a file window that may need more than one batch (the PR's
/// `changed_files` then sizes the rest, which load at once), or alone when a
/// later comment page that `matchString` filters needs the PR's counts.
async fn first_requests(
    transport: &GitHubTransport,
    context: &RequestContext,
    pulls: &[&str],
    file_batch: Option<usize>,
    later_comments: bool,
) -> Result<(Option<Value>, Option<(usize, Batch)>), ProviderError> {
    if let Some(batch) = file_batch {
        let files_path = [pulls, &["files"]].concat();
        let page = [
            ("per_page", MAX_PER_PAGE.to_string()),
            ("page", batch.to_string()),
        ];
        let ((raw, _), files) = tokio::try_join!(
            fetch(transport, pulls, &[], context),
            fetch(transport, &files_path, &page, context),
        )?;
        return Ok((Some(raw), Some((batch, files))));
    }
    if later_comments {
        return Ok((Some(fetch(transport, pulls, &[], context).await?.0), None));
    }
    Ok((None, None))
}

/// The PR fetched before its collections, beside the first batch of its
/// file window, when they need its counts: a selected or match-filtered file
/// scan cannot jump to a batch by index, a page whose files cross a batch
/// boundary needs the count to read its batches at once, and a later
/// comment page that `matchString` filters cannot jump either. The held
/// batch is the window's own first batch, never read again; when it ends
/// the list (most PRs), the read costs one round trip.
async fn prefetch(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &Sections,
    plan: &PrPlan,
    files: &WindowSpec,
    graphql: Option<&GraphqlPr>,
) -> Result<(Option<Value>, Option<(usize, Batch)>), ProviderError> {
    if graphql.is_some() {
        return Ok((None, None));
    }
    let file_batch = (wants.lists_files() && files.spans_batches()).then(|| files.first_batch());
    // Only a `matchString` filters comments; an unfiltered later page jumps
    // straight to its provider batch.
    let later_comments = (wants.has(Section::Comments) || wants.has(Section::ReviewComments))
        && plan.needle.is_some()
        && gh_get_history_item_query::page(query).is_some_and(|page| page > 1);
    let pulls = [
        "repos",
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        "pulls",
        plan.number.as_str(),
    ];
    first_requests(transport, context, &pulls, file_batch, later_comments).await
}

/// The PR itself: from GraphQL, the prefetch, or one REST read.
async fn load_raw(
    transport: &GitHubTransport,
    context: &RequestContext,
    pulls: &[&str],
    graphql: Option<&GraphqlPr>,
    prefetched: Option<Value>,
) -> Result<Value, ProviderError> {
    match (graphql, prefetched) {
        (Some(graphql), _) => Ok(graphql.raw.clone()),
        (None, Some(raw)) => Ok(raw),
        (None, None) => fetch(transport, pulls, &[], context)
            .await
            .map(|(raw, _)| raw),
    }
}

/// A collection's item count on the prefetched PR (`changed_files`,
/// `comments`, `review_comments`).
fn provider_count(raw: Option<&Value>, key: &str) -> Option<usize> {
    raw.and_then(|raw| raw.get(key))
        .and_then(Value::as_u64)
        .map(usize_of)
}

/// The review-comment (inline) window of the requested page, sized by the
/// PR's `review_comments` count when it is known.
pub(super) async fn load_inline(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    plan: &PrPlan,
    provider_total: Option<usize>,
) -> Result<Option<Loaded>, ProviderError> {
    let needle = plan.needle.as_deref();
    let path = [
        "repos",
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        "pulls",
        plan.number.as_str(),
        "comments",
    ];
    load_collection(
        transport,
        true,
        None,
        &path,
        WindowSpec {
            batch_size: MAX_PER_PAGE,
            max_batches: MAX_COLLECTION_BATCHES,
            page: gh_get_history_item_query::page(query).unwrap_or(1),
            page_size: gh_get_history_item_query::collection_page_size(query),
            filtered: needle.is_some(),
            provider_total,
            prefetched: None,
        },
        |value| body_matches(value, needle),
        context,
    )
    .await
}

/// The review-comment (inline) window as `load_pr` receives it.
pub(super) enum InlineWindow {
    /// Already loaded beside the GraphQL read.
    Prefetched(Option<Loaded>),
    /// Not loaded: `load_pr` loads it when the sections ask for it.
    Pending,
}

/// `inline` is the review-comment window, prefetched when it loaded beside
/// the GraphQL read.
pub(super) async fn load_pr(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &Sections,
    plan: &PrPlan,
    graphql: Option<&GraphqlPr>,
    inline: InlineWindow,
) -> Result<PrLoads, ProviderError> {
    let (file_filter, needle) = (plan.file_filter(), plan.needle.as_deref());
    let page_size = gh_get_history_item_query::collection_page_size(query);
    let comment_filter = |value: &Value| body_matches(value, needle);
    let spec = |max_batches, page: Option<usize>, filtered| WindowSpec {
        batch_size: MAX_PER_PAGE,
        max_batches,
        page: page.unwrap_or(1),
        page_size,
        filtered,
        provider_total: None,
        prefetched: None,
    };
    let (owner, repo, number) = (
        gh_get_history_item_query::owner(query),
        gh_get_history_item_query::repo(query),
        plan.number.as_str(),
    );
    let under_pulls = |leaf: &'static str| ["repos", owner, repo, "pulls", number, leaf];
    let [files_path, reviews_path, commits_path] = ["files", "reviews", "commits"].map(under_pulls);
    let discussion_path = ["repos", owner, repo, "issues", number, "comments"];
    let complete = |kind: fn(&GraphqlPr) -> GraphqlCollection, map: fn(&Value) -> Vec<Value>| {
        graphql.and_then(|g| match kind(g) {
            GraphqlCollection::Complete => Some(Loaded::complete(map(&g.source))),
            GraphqlCollection::Partial => Some(Loaded::prefix(map(&g.source))),
            GraphqlCollection::Unused | GraphqlCollection::Incomplete => None,
        })
    };
    let mut files = WindowSpec {
        page_size: file_page_size(query, wants.has(Section::Patches)),
        ..spec(
            MAX_FILE_BATCHES,
            gh_get_history_item_query::page(query),
            !file_filter.is_trivial(),
        )
    };
    let (raw_first, file_batch) =
        prefetch(transport, query, context, wants, plan, &files, graphql).await?;
    // The PR's counts size the collections' batches: from the prefetch, or
    // from the GraphQL header (which counts files and comments).
    let counts = raw_first.as_ref().or(graphql.map(|g| &g.raw));
    let provider_count = |key: &str| provider_count(counts, key);
    files.provider_total = provider_count("changed_files");
    files.prefetched = file_batch;
    let pulls = ["repos", owner, repo, "pulls", number];
    // Collections load concurrently; each derives its provider batches from
    // the public page cursor (no provider cursor leaks into continuations).
    let (raw, files, discussion, inline, reviews, commits) = tokio::try_join!(
        load_raw(transport, context, &pulls, graphql, raw_first.clone()),
        load_collection(
            transport,
            wants.lists_files(),
            complete(|g| g.files, map_graphql_files),
            &files_path,
            files,
            |value| file_filter.matches(value),
            context,
        ),
        load_collection(
            transport,
            wants.has(Section::Comments),
            complete(|g| g.discussion, map_graphql_comments),
            &discussion_path,
            WindowSpec {
                provider_total: provider_count("comments"),
                ..spec(
                    MAX_COLLECTION_BATCHES,
                    gh_get_history_item_query::page(query),
                    needle.is_some()
                )
            },
            comment_filter,
            context,
        ),
        async {
            match inline {
                InlineWindow::Prefetched(loaded) => Ok(loaded),
                InlineWindow::Pending if wants.has(Section::ReviewComments) => {
                    let total = provider_count("review_comments");
                    load_inline(transport, query, context, plan, total).await
                }
                InlineWindow::Pending => Ok(None),
            }
        },
        load_collection(
            transport,
            wants.has(Section::Reviews),
            complete(|g| g.reviews, map_graphql_reviews),
            &reviews_path,
            spec(
                MAX_COLLECTION_BATCHES,
                gh_get_history_item_query::page(query),
                needle.is_some()
            ),
            comment_filter,
            context,
        ),
        load_collection(
            transport,
            wants.lists_commits(),
            complete(|g| g.commits, map_graphql_commits),
            &commits_path,
            spec(
                MAX_PR_COMMIT_BATCHES,
                gh_get_history_item_query::page(query),
                false
            ),
            |_| true,
            context,
        ),
    )?;
    Ok(PrLoads {
        raw,
        files,
        discussion,
        inline,
        reviews,
        commits,
    })
}
