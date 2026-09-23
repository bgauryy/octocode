//! GraphQL pull-request response mapping.
//!
//! Pure `Value -> Value` projection of the GitHub GraphQL pull-request shape
//! into the REST-shaped values the rest of the tool consumes, plus the
//! per-collection completeness state. No transport or request state lives here;
//! the fetch that produces these inputs stays in the parent module.

use super::util::str_at;
use serde_json::{Value, json};

/// Whether a paginated PR sub-collection was fully returned by the GraphQL
/// query, partially returned (more pages remain), or not requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GraphqlCollection {
    Unused,
    Complete,
    Incomplete,
}

pub(super) fn graphql_collection_state(pr: &Value, key: &str, wanted: bool) -> GraphqlCollection {
    if !wanted {
        return GraphqlCollection::Unused;
    }
    if pr
        .pointer(&format!("/{key}/pageInfo/hasNextPage"))
        .and_then(Value::as_bool)
        == Some(true)
        || pr.get(key).is_none()
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
    json!({
        "number": pr.get("number"),
        "title": pr.get("title"),
        "html_url": pr.get("url"),
        // GraphQL reports OPEN/CLOSED/MERGED; the public contract (and the REST
        // shape this projects into) uses lowercase open/closed/merged.
        "state": str_at(pr, "/state").map(str::to_ascii_lowercase),
        "body": pr.get("body"),
        "draft": pr.get("isDraft"),
        "merged_at": pr.get("mergedAt"),
        "user": { "login": str_at(pr, "/author/login").unwrap_or("") },
        "labels": labels,
        "base": { "ref": pr.get("baseRefName") },
        "head": { "ref": pr.get("headRefName"), "sha": pr.get("headRefOid") },
        "created_at": pr.get("createdAt"),
        "updated_at": pr.get("updatedAt"),
        "closed_at": pr.get("closedAt"),
        "comments": pr.pointer("/comments/totalCount"),
        "changed_files": pr.get("changedFiles"),
        "additions": pr.get("additions"),
        "deletions": pr.get("deletions"),
    })
}

pub(super) fn map_graphql_files(pr: &Value) -> Vec<Value> {
    pr.pointer("/files/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|node| {
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
        .collect()
}

pub(super) fn map_graphql_comments(pr: &Value) -> Vec<Value> {
    pr.pointer("/commentsConn/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|node| {
            json!({
                "id": node.get("databaseId").cloned().unwrap_or_else(|| node["id"].clone()),
                "body": node.get("body"),
                "user": { "login": str_at(node, "/author/login").unwrap_or("unknown") },
                "created_at": node.get("createdAt"),
                "html_url": node.get("url"),
            })
        })
        .collect()
}

pub(super) fn map_graphql_reviews(pr: &Value) -> Vec<Value> {
    pr.pointer("/reviews/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|node| {
            json!({
                "id": node.get("id"),
                "user": { "login": str_at(node, "/author/login").unwrap_or("unknown") },
                "state": node.get("state"),
                "body": node.get("body"),
                "submitted_at": node.get("submittedAt"),
            })
        })
        .collect()
}

pub(super) fn map_graphql_commits(pr: &Value) -> Vec<Value> {
    pr.pointer("/commits/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|node| {
            json!({
                "sha": str_at(node, "/commit/oid").unwrap_or(""),
                "commit": {
                    "message": str_at(node, "/commit/messageHeadline").unwrap_or(""),
                    "author": {
                        "name": str_at(node, "/commit/author/user/login").unwrap_or("unknown"),
                        "date": str_at(node, "/commit/authoredDate").unwrap_or("")
                    }
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphql_pr_state_is_lowercased_to_the_contract_enum() {
        for (raw, want) in [("OPEN", "open"), ("CLOSED", "closed"), ("MERGED", "merged")] {
            let mapped = map_graphql_pr_metadata(&json!({"state": raw}));
            assert_eq!(mapped["state"], want);
        }
    }
}
