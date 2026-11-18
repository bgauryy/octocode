//! A pull request's whole comment count: the count GitHub's search sorts
//! (`sort:comments`) and filters (`comments:`) by. GraphQL's
//! `totalCommentsCount` states it; the REST item's `comments` counts the
//! conversation alone, so a page ranked by comments would contradict its
//! own order.
use super::{HistoryOperation, HistorySearch, gh_search_history_query};
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Each pull request's whole comment count, by node id.
const DOCUMENT: &str =
    "query($ids: [ID!]!) { nodes(ids: $ids) { ... on PullRequest { id totalCommentsCount } } }";

/// The note of a page whose rows keep GitHub's conversation count.
const CONVERSATION_ONLY: &str = "commentsCount counts conversation comments only: GitHub's whole count, which its sort and filters use, could not be read (GraphQL).";

/// Whether this page's rows state `commentsCount`: a pull-request search
/// sorted by it, or a `fields` pick that names it.
pub(super) fn shown(query: &HistorySearch) -> bool {
    gh_search_history_query::operation(query) == HistoryOperation::PullRequest
        && (gh_search_history_query::sort(query).as_deref() == Some("comments")
            || gh_search_history_query::fields(query)
                .iter()
                .any(|field| field == "commentsCount"))
}

/// Sets each item's `comments` to its whole count, read in one GraphQL
/// request for the page. Returns the note to state when an item keeps its
/// REST count: GraphQL is unavailable or failed, or did not answer its node.
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
    let mut kept_rest = false;
    for item in items.iter_mut() {
        let count = item
            .get("node_id")
            .and_then(Value::as_str)
            .and_then(|id| counts.get(id).copied());
        match count {
            Some(count) => item["comments"] = json!(count),
            None => kept_rest = true,
        }
    }
    Ok(kept_rest.then(|| CONVERSATION_ONLY.to_owned()))
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
