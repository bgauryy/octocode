//! A pull request's whole comment count (`totalCommentsCount`): the count
//! GitHub's search sorts (`sort:comments`) and filters (`comments:`) by.
//! GraphQL states it; the REST item's `comments` counts the conversation
//! alone (the row's `commentsCount`), so a page ranked by comments shows
//! the whole count its order follows.
use super::{HistoryOperation, HistorySearch, gh_search_history_query};
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use octocode_tools_shared::gh_shared::history::WHOLE_COUNT_UNREAD;
use serde_json::{Value, json};
use std::collections::HashMap;

/// Each pull request's whole comment count, by node id.
const DOCUMENT: &str =
    "query($ids: [ID!]!) { nodes(ids: $ids) { ... on PullRequest { id totalCommentsCount } } }";

/// The item key the whole count rides on to its row's `totalCommentsCount`
/// ([`super::rows::map_pr`]).
pub(super) const ITEM_KEY: &str = "total_comments";

/// Whether this page's rows state `totalCommentsCount`: a `fields` pick
/// that names it, or, with no pick, a pull-request search sorted or
/// filtered by comments (GitHub ranks and selects by the whole count). A
/// pick without it shows no whole count, so none is read.
pub(super) fn shown(query: &HistorySearch) -> bool {
    if gh_search_history_query::operation(query) != HistoryOperation::PullRequest {
        return false;
    }
    let fields = gh_search_history_query::fields(query);
    if fields.is_empty() {
        by_comments(query)
    } else {
        fields.iter().any(|field| field == "totalCommentsCount")
    }
}

/// Whether the search sorts or filters by comment count: its rows' counts
/// are what decided them.
pub(super) fn by_comments(query: &HistorySearch) -> bool {
    gh_search_history_query::sort(query).as_deref() == Some("comments")
        || query.filters().comments.is_some()
}

/// Puts each item's whole count on it ([`ITEM_KEY`]), read in one GraphQL
/// request for the page. Returns the note to state when an item has none:
/// GraphQL is unavailable or failed, or did not answer its node; its
/// `comments` (the conversation count) is left as GitHub sent it.
/// A cancelled or timed-out request fails the page.
pub(super) async fn whole_counts(
    transport: &GitHubTransport,
    items: &mut [Value],
    context: &RequestContext,
) -> Result<Option<String>, ProviderError> {
    if items.is_empty() {
        return Ok(None);
    }
    let ids = items
        .iter()
        .filter_map(|item| item.get("node_id")?.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let counts = if ids.is_empty() || !transport.graphql_available(context) {
        HashMap::new()
    } else {
        match transport
            .execute_graphql(DOCUMENT, json!({ "ids": ids }), context)
            .await
        {
            Ok(page) => counts_of(page.data.as_ref()),
            Err(error) if error.is_cancelled_or_timed_out() => return Err(error),
            Err(_) => HashMap::new(),
        }
    };
    let mut unread = false;
    for item in items.iter_mut() {
        let count = item
            .get("node_id")
            .and_then(Value::as_str)
            .and_then(|id| counts.get(id).copied());
        match count {
            Some(count) => item[ITEM_KEY] = json!(count),
            None => unread = true,
        }
    }
    Ok(unread.then(|| WHOLE_COUNT_UNREAD.to_owned()))
}

/// The answered nodes' counts, by id.
fn counts_of(data: Option<&Value>) -> HashMap<String, u64> {
    data.and_then(|data| data.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| {
            Some((
                node.get("id")?.as_str()?.to_owned(),
                node.get("totalCommentsCount")?.as_u64()?,
            ))
        })
        .collect()
}
