//! The GraphQL pull request projected into the REST-shaped values the
//! tool consumes, with per-collection completeness.
use crate::tools::gh_get_history_item::util::str_at;
use serde_json::{Value, json};

/// Whether a paginated PR sub-collection was fully returned by the GraphQL
/// query, returned in part that covers its first page (more pages remain),
/// left for REST, or not requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::tools::gh_get_history_item) enum GraphqlCollection {
    Unused,
    Complete,
    /// The held nodes cover the first page; later pages remain.
    Partial,
    Incomplete,
}

pub(super) fn graphql_collection_state(pr: &Value, key: &str) -> GraphqlCollection {
    if pr
        .pointer(&format!("/{key}/pageInfo/hasNextPage"))
        .and_then(Value::as_bool)
        == Some(true)
        // A connection the token cannot read comes back `null` with the
        // cause in `errors[]`: missing, not empty.
        || pr.get(key).is_none_or(Value::is_null)
    {
        GraphqlCollection::Incomplete
    } else {
        GraphqlCollection::Complete
    }
}

pub(super) fn map_graphql_pr_metadata(pr: &Value) -> Value {
    let labels = pr
        .pointer("/labels/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| str_at(value, "/name").map(str::to_owned))
        .map(Value::String)
        .collect::<Vec<_>>();
    let labels_truncated = pr
        .pointer("/labels/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        == Some(true);
    let mut raw = json!({
        "number": pr.get("number"),
        "title": pr.get("title"),
        "html_url": pr.get("url"),
        // GraphQL reports OPEN/CLOSED/MERGED; the public contract (and the REST
        // shape this projects into) uses lowercase open/closed/merged.
        "state": str_at(pr, "/state").map(str::to_ascii_lowercase),
        "draft": pr.get("isDraft"),
        "merged_at": pr.get("mergedAt"),
        // REST 2026-03-10 drops merge_commit_sha; GraphQL keeps mergeCommit.
        "merge_commit_sha": pr.pointer("/mergeCommit/oid"),
        "user": { "login": str_at(pr, "/author/login").unwrap_or("") },
        "labels": labels,
        "labels_truncated": labels_truncated,
        // The target-branch tip GitHub recorded for the PR (`baseSha`).
        "base": { "ref": pr.get("baseRefName"), "sha": pr.get("baseRefOid") },
        "head": { "ref": pr.get("headRefName"), "sha": pr.get("headRefOid") },
        "created_at": pr.get("createdAt"),
        "updated_at": pr.get("updatedAt"),
        "closed_at": pr.get("closedAt"),
        "comments": pr.pointer("/comments/totalCount"),
        "commits": pr.pointer("/commitsCount/totalCount"),
        // GraphQL only: REST has no review-thread total.
        "review_threads": pr.pointer("/reviewThreads/totalCount"),
        "changed_files": pr.get("changedFiles"),
        "additions": pr.get("additions"),
        "deletions": pr.get("deletions"),
    });
    // A selected body is GitHub's (null when empty); an unselected one is
    // left out, so readers tell "no body" from "not read".
    if let Some(body) = pr.get("body") {
        raw["body"] = body.clone();
    }
    raw
}

/// Each node of the connection at `pointer`, mapped by `row`.
fn nodes(pr: &Value, pointer: &str, row: impl Fn(&Value) -> Value) -> Vec<Value> {
    pr.pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(row)
        .collect()
}

pub(in crate::tools::gh_get_history_item) fn map_graphql_files(pr: &Value) -> Vec<Value> {
    nodes(pr, "/files/nodes", |node| {
        json!({
            "filename": str_at(node, "/path").unwrap_or(""),
            "additions": node.get("additions"),
            "deletions": node.get("deletions"),
            "status": match str_at(node, "/changeType").unwrap_or("MODIFIED") {
                "ADDED" => "added",
                "DELETED" => "removed",
                "RENAMED" => "renamed",
                _ => "modified",
            }
        })
    })
}

/// The REST login of a GraphQL author: GraphQL drops the `[bot]` suffix
/// REST gives app accounts, so a `Bot` author gets it back and both paths
/// name the same author.
fn author_login(node: &Value) -> String {
    let login = str_at(node, "/author/login").unwrap_or("unknown");
    if str_at(node, "/author/__typename") == Some("Bot") && !login.ends_with("[bot]") {
        format!("{login}[bot]")
    } else {
        login.to_owned()
    }
}

pub(in crate::tools::gh_get_history_item) fn map_graphql_comments(pr: &Value) -> Vec<Value> {
    nodes(pr, "/commentsConn/nodes", |node| {
        json!({
            "id": node.get("databaseId").cloned().unwrap_or_else(|| node["id"].clone()),
            "body": node.get("body"),
            "user": { "login": author_login(node) },
            "created_at": node.get("createdAt"),
            "html_url": node.get("url"),
        })
    })
}

pub(in crate::tools::gh_get_history_item) fn map_graphql_reviews(pr: &Value) -> Vec<Value> {
    nodes(pr, "/reviews/nodes", |node| {
        // The REST review shape: numeric id and the reviewed commit.
        json!({
            "id": node.get("databaseId"),
            "user": { "login": author_login(node) },
            "state": node.get("state"),
            "body": node.get("body"),
            "submitted_at": node.get("submittedAt"),
            "commit_id": node.pointer("/commit/oid"),
        })
    })
}

pub(in crate::tools::gh_get_history_item) fn map_graphql_commits(pr: &Value) -> Vec<Value> {
    nodes(pr, "/commits/nodes", |node| {
        // The REST shape: the GitHub account beside the git identity.
        json!({
            "sha": str_at(node, "/commit/oid").unwrap_or(""),
            "author": str_at(node, "/commit/author/user/login").map(|login| json!({"login": login})),
            "commit": {
                // Full message (headline + body), like the REST shape;
                // the headline is only a fallback for older servers.
                "message": str_at(node, "/commit/message")
                    .or_else(|| str_at(node, "/commit/messageHeadline"))
                    .unwrap_or(""),
                "author": {
                    "name": str_at(node, "/commit/author/name").unwrap_or("unknown"),
                    "date": str_at(node, "/commit/authoredDate").unwrap_or("")
                },
                // The date a commit row shows (`commit::commit_date`).
                "committer": {"date": str_at(node, "/commit/committedDate")}
            }
        })
    })
}
