//! Pull-request facts a search item lacks, read by GraphQL in one request
//! per page: a pull request's whole comment count (`totalCommentsCount`),
//! the count GitHub's search sorts (`sort:comments`) and filters
//! (`comments:`) by, and its target branch (`baseRef`). The REST item's
//! `comments` counts the conversation alone (the row's `commentsCount`), so
//! a page ranked by comments shows the whole count its order follows; a
//! listing's item names its target branch, a search item does not.
use super::{HistoryOperation, HistorySearch, gh_search_history_query};
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
use octocode_tools_shared::gh_shared::history::WHOLE_COUNT_UNREAD;
use serde_json::{Value, json};
use std::collections::HashMap;

/// The item key the whole count rides on to its row's `totalCommentsCount`
/// ([`super::rows::map_pr`]).
pub(super) const ITEM_KEY: &str = "total_comments";

/// The facts a page's rows state that its items may lack.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Wanted {
    whole_count: bool,
    base_ref: bool,
}

impl Wanted {
    /// What this page's rows state: the whole count when a `fields` pick
    /// names it or, with no pick, when the search sorts or filters by
    /// comments (GitHub ranks and selects by the whole count); the target
    /// branch when a pick names `baseRef` and some item does not carry it
    /// (a search item; a listing's item names it).
    pub(super) fn of(query: &HistorySearch, items: &[Value]) -> Self {
        if gh_search_history_query::operation(query) != HistoryOperation::PullRequest {
            return Self::default();
        }
        let fields = gh_search_history_query::fields(query);
        let picked = |key: &str| fields.iter().any(|field| field == key);
        Self {
            whole_count: if fields.is_empty() {
                by_comments(query)
            } else {
                picked("totalCommentsCount")
            },
            base_ref: picked("baseRef")
                && items.iter().any(|item| item.pointer("/base/ref").is_none()),
        }
    }

    fn any(self) -> bool {
        self.whole_count || self.base_ref
    }

    /// The GraphQL document reading what is wanted, by node id.
    fn document(self) -> String {
        let mut facts = vec!["id"];
        if self.whole_count {
            facts.push("totalCommentsCount");
        }
        if self.base_ref {
            facts.push("baseRefName");
        }
        format!(
            "query($ids: [ID!]!) {{ nodes(ids: $ids) {{ ... on PullRequest {{ {} }} }} }}",
            facts.join(" ")
        )
    }
}

/// Whether the search sorts or filters by comment count: its rows' counts
/// are what decided them.
pub(super) fn by_comments(query: &HistorySearch) -> bool {
    gh_search_history_query::sort(query).as_deref() == Some("comments")
        || query.filters().comments.is_some()
}

/// Puts each wanted fact on its item, read in one GraphQL request for the
/// page: the whole count on [`ITEM_KEY`], the target branch on `base.ref`
/// (where a listing's item carries it). Returns the note to state when an
/// item has no whole count: GraphQL is unavailable or failed, or did not
/// answer its node; its `comments` (the conversation count) is left as
/// GitHub sent it. A target branch left unread is stated by no row, which
/// the `fields` pick reports. A cancelled or timed-out request fails the
/// page.
pub(super) async fn read(
    transport: &GitHubTransport,
    wanted: Wanted,
    items: &mut [Value],
    context: &RequestContext,
) -> Result<Option<String>, ProviderError> {
    if items.is_empty() || !wanted.any() {
        return Ok(None);
    }
    let ids = items
        .iter()
        .filter_map(|item| item.get("node_id")?.as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let nodes = if ids.is_empty() || !transport.graphql_available(context) {
        HashMap::new()
    } else {
        match transport
            .execute_graphql(&wanted.document(), json!({ "ids": ids }), context)
            .await
        {
            Ok(page) => nodes_of(page.data.as_ref()),
            Err(error) if error.is_cancelled_or_timed_out() => return Err(error),
            Err(_) => HashMap::new(),
        }
    };
    let mut unread = false;
    for item in items.iter_mut() {
        let node = item
            .get("node_id")
            .and_then(Value::as_str)
            .and_then(|id| nodes.get(id));
        if wanted.whole_count {
            match node.and_then(|node| node["totalCommentsCount"].as_u64()) {
                Some(count) => item[ITEM_KEY] = json!(count),
                None => unread = true,
            }
        }
        if wanted.base_ref
            && item.pointer("/base/ref").is_none()
            && let Some(name) = node.and_then(|node| node["baseRefName"].as_str())
        {
            item["base"] = json!({ "ref": name });
        }
    }
    Ok((wanted.whole_count && unread).then(|| WHOLE_COUNT_UNREAD.to_owned()))
}

/// The answered nodes, by id.
fn nodes_of(data: Option<&Value>) -> HashMap<String, Value> {
    data.and_then(|data| data.get("nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| Some((node.get("id")?.as_str()?.to_owned(), node.clone())))
        .collect()
}
