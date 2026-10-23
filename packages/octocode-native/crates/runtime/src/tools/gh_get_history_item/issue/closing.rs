//! The pull requests that close an issue (`closedBy`) and the fix read
//! they lead to.
use crate::id::ToolId;
use crate::response::result::{Confidence, Continuation, remove_nulls};
use crate::tools::gh_get_history_item::HistoryItemRequest;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_get_history_item::util::str_at;
use crate::tools::gh_shared::ensure_next;
use octocode_github::{GitHubTransport, RequestContext};
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

/// Closing references one issue read lists. Past it the set is bounded and
/// the response says so.
const MAX_CLOSING_REFERENCES: usize = 25;

/// The closing-reference lookup; `first` is [`MAX_CLOSING_REFERENCES`].
pub(in crate::tools::gh_get_history_item) const CLOSING_REFERENCES_DOCUMENT: &str = "query($owner:String!,$repo:String!,$number:Int!,$first:Int!){ repository(owner:$owner,name:$repo){ issue(number:$number){ closedByPullRequestsReferences(first:$first,includeClosedPrs:true){ totalCount pageInfo{ hasNextPage } nodes{ number state mergedAt additions deletions changedFiles } } } } }";

/// One linked pull request: its public `closedBy` row and whether its whole
/// diff fits one patch read.
pub(super) struct ClosingPr {
    pub(super) row: Value,
    small: bool,
}

/// The linked pull requests one read returned, merged first, how many the
/// first window lists, and GitHub's total when more exist than that.
pub(super) struct ClosingReferences {
    pub(super) prs: Vec<ClosingPr>,
    pub(super) listed: usize,
    pub(super) bounded_total: Option<u64>,
}

/// The pull requests whose merge closes (or closed) the issue, merged first:
/// `{number, state, mergedAt?}`. `None` when GraphQL is unavailable or
/// failed (the caller falls back to search); empty when none are linked.
pub(super) async fn closing_pull_requests(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<ClosingReferences> {
    let references = closed_by_references(transport, query, context).await?;
    let more = references
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        == Some(true);
    let prs = map_closing_pull_requests(references.get("nodes")?.as_array()?);
    let listed = u64::try_from(prs.len()).unwrap_or(u64::MAX);
    Some(ClosingReferences {
        bounded_total: more.then(|| {
            references
                .get("totalCount")
                .and_then(Value::as_u64)
                .map_or(listed, |total| total.max(listed))
        }),
        listed: prs.len(),
        prs,
    })
}

/// The issue's first [`MAX_CLOSING_REFERENCES`]
/// `closedByPullRequestsReferences`. `None` when GraphQL is unavailable or
/// failed.
async fn closed_by_references(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Option<Value> {
    if !transport.graphql_enabled || !transport.graphql_available(context) {
        return None;
    }
    let variables = json!({
        "owner": gh_get_history_item_query::owner(query),
        "repo": gh_get_history_item_query::repo(query),
        "number": gh_get_history_item_query::number(query)?,
        "first": MAX_CLOSING_REFERENCES,
    });
    let mut page = transport
        .execute_graphql(CLOSING_REFERENCES_DOCUMENT, variables, context)
        .await
        .ok()?;
    page.data
        .as_mut()?
        .pointer_mut("/repository/issue/closedByPullRequestsReferences")
        .map(Value::take)
}

fn map_closing_pull_requests(nodes: &[Value]) -> Vec<ClosingPr> {
    let mut prs = nodes
        .iter()
        .filter_map(|node| {
            let number = node.get("number").and_then(Value::as_u64)?;
            let mut row = json!({
                "number": number,
                "state": str_at(node, "/state").unwrap_or("closed").to_ascii_lowercase(),
                "mergedAt": node.get("mergedAt").filter(|v| !v.is_null()),
            });
            remove_nulls(&mut row);
            let count = |key: &str| node.get(key).and_then(Value::as_u64);
            let small = crate::tools::gh_get_history_item::pr_menu::is_small_pr(
                count("additions"),
                count("deletions"),
                count("changedFiles"),
            );
            Some(ClosingPr { row, small })
        })
        .collect::<Vec<_>>();
    // Stable: merged first, otherwise GitHub's order.
    prs.sort_by_key(|pr| pr.row.get("mergedAt").is_none());
    prs
}

/// `next.readPullRequest` reads the first linked (merged-first) pull request;
/// with no link information a closed issue offers the keyword search hop.
pub(super) fn attach_fix_pr(
    out: &mut Value,
    query: &HistoryItemRequest,
    closed_by: Option<&[ClosingPr]>,
    closed: bool,
    bounded: bool,
) {
    let confidence = if bounded {
        Confidence::Medium
    } else {
        Confidence::High
    };
    let next = match closed_by.and_then(<[ClosingPr]>::first) {
        // A small fix reads whole in one call: its patches are the evidence
        // (`closedBy` already links it to this issue). A larger one names
        // its files, beside its description, so the review can pick patches.
        // Files the caller's goal names: their patches are the evidence, and
        // the filter rides the copied query, so the narrowing is visible.
        Some(pr) => {
            let named = goal_file_globs(query);
            let mut read = json!({
                "operation":"pullRequest","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),
                "number":pr.row["number"],
                "sections": if pr.small || !named.is_empty() { json!(["patches"]) } else { json!(["body", "files"]) }});
            if !named.is_empty() {
                read["include"] = json!(named);
            }
            (
                "readPullRequest",
                Continuation::new(ToolId::GhGetHistoryItem, read)
                    .confidence(confidence)
                    .build(),
            )
        }
        None if closed && closed_by.is_none() => (
            "findPullRequest",
            Continuation::new(
                ToolId::GhSearchHistory,
                json!({"operation":"pullRequest","owner":gh_get_history_item_query::owner(query),"repo":gh_get_history_item_query::repo(query),
                    "keywords":[gh_get_history_item_query::number(query).unwrap_or_default().to_string()]}),
            )
            .confidence(Confidence::Medium)
            .build(),
        ),
        None => return,
    };
    ensure_next(out);
    // The fix is new evidence: it leads the issue's own reads, so a
    // one-lead answer offers it.
    if let Some(leads) = out["next"].as_object_mut() {
        leads.shift_insert(0, next.0.to_owned(), next.1);
    }
}

/// Most goal-named files one fix-PR read narrows to.
const MAX_GOAL_FILES: usize = 5;

/// `include` globs for file names the query's goal mentions
/// (`merge.go`, `pkg/cmd/merge.go`): a word counts only when its name has a
/// known code, config, doc, or lock file type, so dotted identifiers
/// (`res.redirect`) and versions never narrow a read.
fn goal_file_globs(query: &HistoryItemRequest) -> Vec<String> {
    use crate::content::classify_file_type;
    let goal = query
        .public
        .get("mainGoal")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut globs = Vec::new();
    for word in goal.split(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))) {
        let path = word.trim_matches(|c| c == '.' || c == '/');
        let name = path.rsplit('/').next().unwrap_or(path);
        let has_stem = name
            .rsplit_once('.')
            .is_some_and(|(stem, _)| stem.chars().any(char::is_alphanumeric));
        if !has_stem || classify_file_type(name).is_none() {
            continue;
        }
        let glob = format!("**/{path}");
        if !globs.contains(&glob) {
            globs.push(glob);
        }
        if globs.len() == MAX_GOAL_FILES {
            break;
        }
    }
    globs
}
