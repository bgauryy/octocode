//! GraphQL pull-request fast path.
//!
//! One GraphQL request can serve a first page of several PR collections at
//! once. This module decides eligibility, runs that request, and projects the
//! GraphQL shape into the REST-shaped values the rest of the tool consumes,
//! with per-collection completeness state.

use super::pull_request::{ContentWants, content_wants};
use super::util::str_at;
use super::{HistoryItemRequest, ItemOperation};
use crate::providers::github::{
    CredentialResolver, GitHubTransport, ProviderError, RequestContext,
};
use serde_json::{Value, json};

/// The first-page PR served by GraphQL: REST-shaped metadata, the raw
/// GraphQL node, and per-collection completeness.
pub(super) struct GraphqlPr {
    pub(super) raw: Value,
    pub(super) source: Value,
    pub(super) files: GraphqlCollection,
    pub(super) discussion: GraphqlCollection,
    pub(super) reviews: GraphqlCollection,
    pub(super) commits: GraphqlCollection,
}

/// GraphQL pays off only for a first page of at least two collections and no
/// patches (GraphQL returns no patch text).
pub(super) fn graphql_complete_collection_eligible(query: &HistoryItemRequest) -> bool {
    if !matches!(
        query.operation(),
        ItemOperation::PullRequest | ItemOperation::Issue
    ) {
        return false;
    }
    if query.page().unwrap_or(1) > 1
        || query.file_page().unwrap_or(1) > 1
        || query.comment_page().unwrap_or(1) > 1
        || query.commit_page().unwrap_or(1) > 1
        || query.review_page().unwrap_or(1) > 1
        || query.include_diff()
    {
        return false;
    }
    let wants = content_wants(query);
    if wants.patch_mode != "none" {
        return false;
    }
    let eligible = [
        wants.body,
        wants.files,
        wants.discussion,
        wants.commits,
        wants.reviews,
    ]
    .into_iter()
    .filter(|value| *value)
    .count();
    eligible >= 2
}

/// Fetch the PR and its wanted collections in one GraphQL request. `None`
/// means GraphQL is unavailable or answered nothing usable (fall back to REST).
pub(super) async fn graphql_pull_request<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &ContentWants,
) -> Result<Option<GraphqlPr>, ProviderError> {
    if !transport.graphql_enabled || !transport.graphql_available(context).await {
        return Ok(None);
    }
    let Some(number) = query.number() else {
        return Ok(None);
    };
    let mut selections: Vec<&str> = vec![
        "number title url state body isDraft isMerged author { login }",
        "labels(first:20){ pageInfo{ hasNextPage } nodes { name } }",
        "baseRefName headRefName headRefOid createdAt updatedAt closedAt mergedAt mergeCommit { oid }",
        "comments { totalCount } changedFiles additions deletions",
    ];
    let mut variables = json!({
        "owner": query.owner(),
        "repo": query.repo(),
        "number": number
    });
    let mut header = String::from("query($owner:String!,$repo:String!,$number:Int!");
    for (wanted, variable, first, selection) in [
        (
            wants.files,
            "files",
            100,
            "files(first:$files){ pageInfo{ hasNextPage } nodes{ path additions deletions changeType } }",
        ),
        (
            wants.discussion,
            "discussion",
            100,
            "commentsConn: comments(first:$discussion){ pageInfo{ hasNextPage } nodes{ databaseId author{ login } body createdAt url } }",
        ),
        (
            wants.reviews,
            "reviews",
            100,
            "reviews(first:$reviews){ pageInfo{ hasNextPage } nodes{ author{ login } state body submittedAt } }",
        ),
        (
            wants.commits,
            "commits",
            50,
            "commits(first:$commits){ pageInfo{ hasNextPage } nodes{ commit{ oid message messageHeadline authoredDate author{ user{ login } } } } }",
        ),
    ] {
        if wanted {
            selections.push(selection);
            variables[variable] = json!(first);
            header.push_str(&format!(",${variable}:Int!"));
        }
    }
    header.push(')');
    let document = format!(
        "{header}{{ repository(owner:$owner,name:$repo){{ pullRequest(number:$number){{ {} }} }} }}",
        selections.join(" ")
    );
    let page = transport
        .execute_graphql(&document, variables, context)
        .await?;
    if page.data.is_none() && !page.errors.is_empty() {
        return Ok(None);
    }
    let pr = page
        .data
        .as_ref()
        .and_then(|value| value.pointer("/repository/pullRequest"))
        .cloned()
        .unwrap_or(Value::Null);
    if pr.is_null() {
        return Ok(None);
    }
    Ok(Some(GraphqlPr {
        files: graphql_collection_state(&pr, "files", wants.files),
        discussion: graphql_collection_state(&pr, "commentsConn", wants.discussion),
        reviews: graphql_collection_state(&pr, "reviews", wants.reviews),
        commits: graphql_collection_state(&pr, "commits", wants.commits),
        raw: map_graphql_pr_metadata(&pr),
        source: pr,
    }))
}

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
    let labels_truncated = pr
        .pointer("/labels/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        == Some(true);
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
        // REST 2026-03-10 drops merge_commit_sha; GraphQL keeps mergeCommit.
        "merge_commit_sha": pr.pointer("/mergeCommit/oid"),
        "user": { "login": str_at(pr, "/author/login").unwrap_or("") },
        "labels": labels,
        "labels_truncated": labels_truncated,
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
                    // Full message (headline + body), like the REST shape;
                    // the headline is only a fallback for older servers.
                    "message": str_at(node, "/commit/message")
                        .or_else(|| str_at(node, "/commit/messageHeadline"))
                        .unwrap_or(""),
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
    fn graphql_fast_path_requires_two_flags_first_pages_and_no_patches() {
        let bare: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1
        }))
        .expect("GitHub history test data should be valid");
        assert!(!super::graphql_complete_collection_eligible(&bare));
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"body":true,"changedFiles":true}
        }))
        .expect("GitHub history test data should be valid");
        assert!(super::graphql_complete_collection_eligible(&query));
        let file_page: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"body":true,"changedFiles":true},"filePage":2
        }))
        .expect("GitHub history test data should be valid");
        assert!(!super::graphql_complete_collection_eligible(&file_page));
        // Legacy provider cursors are not part of the wire contract.
        assert!(
            HistoryItemRequest::from_row(json!({
                "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
                "content":{"body":true,"comments":{"discussion":true}},
                "collectionPages":{"discussion":2}
            }))
            .is_err()
        );
        let paged: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"body":true,"comments":{"discussion":true}},
            "commentPage":2
        }))
        .expect("GitHub history test data should be valid");
        assert!(!super::graphql_complete_collection_eligible(&paged));
        let patches: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","goal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "content":{"body":true,"changedFiles":true,"patches":{"mode":"all"}}
        }))
        .expect("GitHub history test data should be valid");
        assert!(!super::graphql_complete_collection_eligible(&patches));
    }

    #[test]
    fn graphql_files_map_to_rest_filename_without_patch() {
        let pr = json!({
            "files":{"pageInfo":{"hasNextPage":false},"nodes":[
                {"path":"src/lib.rs","additions":1,"deletions":2,"changeType":"ADDED"}
            ]}
        });
        let files = super::map_graphql_files(&pr);
        assert_eq!(files[0]["filename"], "src/lib.rs");
        assert_eq!(files[0]["status"], "added");
        assert!(files[0].get("patch").is_none());
        assert_eq!(
            super::graphql_collection_state(&pr, "files", true),
            super::GraphqlCollection::Complete
        );
        let incomplete = json!({"files":{"pageInfo":{"hasNextPage":true},"nodes":[]}});
        assert_eq!(
            super::graphql_collection_state(&incomplete, "files", true),
            super::GraphqlCollection::Incomplete
        );
    }

    #[test]
    fn graphql_pr_state_is_lowercased_to_the_contract_enum() {
        for (raw, want) in [("OPEN", "open"), ("CLOSED", "closed"), ("MERGED", "merged")] {
            let mapped = map_graphql_pr_metadata(&json!({"state": raw}));
            assert_eq!(mapped["state"], want);
        }
    }

    #[test]
    fn graphql_commits_keep_the_full_message_and_labels_report_truncation() {
        let pr = json!({
            "labels": {"pageInfo": {"hasNextPage": true}, "nodes": [{"name": "bug"}]},
            "commits": {"pageInfo": {"hasNextPage": false}, "nodes": [{"commit": {
                "oid": "abc",
                "message": "Fix parser\n\nCo-Authored-By: A <a@example.com>",
                "messageHeadline": "Fix parser",
                "authoredDate": "2026-01-01T00:00:00Z",
                "author": {"user": {"login": "octo"}}
            }}]}
        });
        let commits = map_graphql_commits(&pr);
        assert_eq!(
            commits[0]["commit"]["message"],
            "Fix parser\n\nCo-Authored-By: A <a@example.com>"
        );
        let metadata = map_graphql_pr_metadata(&pr);
        assert_eq!(metadata["labels"], json!(["bug"]));
        assert_eq!(metadata["labels_truncated"], true);
        let complete = map_graphql_pr_metadata(&json!({
            "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []}
        }));
        assert_eq!(complete["labels_truncated"], false);
    }
}
