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

/// What the GraphQL fast path produced for one pull-request read.
pub(super) enum GraphqlOutcome {
    /// GraphQL is disabled or unavailable for this endpoint: REST serves the
    /// read as the configured path, with nothing to report.
    Unavailable,
    /// GraphQL answered without a usable pull request; REST serves the read
    /// and the reason stays observable.
    Failed(String),
    Served(Box<GraphqlPr>),
}

/// The pull-request document for the wanted collections, and the page size
/// variable each collection binds.
pub(super) fn pull_request_document(wants: &ContentWants) -> (String, Vec<(&'static str, u32)>) {
    let mut selections: Vec<&str> = vec![
        "number title url state body isDraft author { login }",
        "labels(first:20){ pageInfo{ hasNextPage } nodes { name } }",
        "baseRefName headRefName headRefOid createdAt updatedAt closedAt mergedAt mergeCommit { oid }",
        "comments { totalCount } changedFiles additions deletions",
    ];
    let mut variables = Vec::new();
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
            "reviews(first:$reviews){ pageInfo{ hasNextPage } nodes{ databaseId author{ login } state body submittedAt commit{ oid } } }",
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
            variables.push((variable, first));
            header.push_str(&format!(",${variable}:Int!"));
        }
    }
    header.push(')');
    let document = format!(
        "{header}{{ repository(owner:$owner,name:$repo){{ pullRequest(number:$number){{ {} }} }} }}",
        selections.join(" ")
    );
    (document, variables)
}

/// Why a GraphQL answer carried no data: the first error's class and text.
fn graphql_failure(errors: &[crate::providers::github::GraphQlError]) -> String {
    errors.first().map_or_else(
        || "GraphQL returned no pull request".to_owned(),
        |error| {
            let class = error
                .error_type
                .as_deref()
                .or_else(|| error.extensions.get("code").and_then(Value::as_str));
            match class {
                Some(class) => format!("{class}: {}", error.message),
                None => error.message.clone(),
            }
        },
    )
}

/// Fetch the PR and its wanted collections in one GraphQL request.
pub(super) async fn graphql_pull_request<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistoryItemRequest,
    context: &RequestContext,
    wants: &ContentWants,
) -> Result<GraphqlOutcome, ProviderError> {
    if !transport.graphql_enabled || !transport.graphql_available(context).await {
        return Ok(GraphqlOutcome::Unavailable);
    }
    let Some(number) = query.number() else {
        return Ok(GraphqlOutcome::Unavailable);
    };
    let (document, bound) = pull_request_document(wants);
    let mut variables = json!({
        "owner": query.owner(),
        "repo": query.repo(),
        "number": number
    });
    for (variable, first) in bound {
        variables[variable] = json!(first);
    }
    let page = transport
        .execute_graphql(&document, variables, context)
        .await?;
    let pr = page
        .data
        .as_ref()
        .and_then(|value| value.pointer("/repository/pullRequest"))
        .cloned()
        .unwrap_or(Value::Null);
    if pr.is_null() {
        return Ok(GraphqlOutcome::Failed(graphql_failure(&page.errors)));
    }
    Ok(GraphqlOutcome::Served(Box::new(GraphqlPr {
        files: graphql_collection_state(&pr, "files", wants.files),
        discussion: graphql_collection_state(&pr, "commentsConn", wants.discussion),
        reviews: graphql_collection_state(&pr, "reviews", wants.reviews),
        commits: graphql_collection_state(&pr, "commits", wants.commits),
        raw: map_graphql_pr_metadata(&pr),
        source: pr,
    })))
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

/// Each node of the connection at `pointer`, mapped by `row`.
fn nodes(pr: &Value, pointer: &str, row: impl Fn(&Value) -> Value) -> Vec<Value> {
    pr.pointer(pointer)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(row)
        .collect()
}

pub(super) fn map_graphql_files(pr: &Value) -> Vec<Value> {
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

pub(super) fn map_graphql_comments(pr: &Value) -> Vec<Value> {
    nodes(pr, "/commentsConn/nodes", |node| {
        json!({
            "id": node.get("databaseId").cloned().unwrap_or_else(|| node["id"].clone()),
            "body": node.get("body"),
            "user": { "login": str_at(node, "/author/login").unwrap_or("unknown") },
            "created_at": node.get("createdAt"),
            "html_url": node.get("url"),
        })
    })
}

pub(super) fn map_graphql_reviews(pr: &Value) -> Vec<Value> {
    nodes(pr, "/reviews/nodes", |node| {
        // The REST review shape: numeric id and the reviewed commit.
        json!({
            "id": node.get("databaseId"),
            "user": { "login": str_at(node, "/author/login").unwrap_or("unknown") },
            "state": node.get("state"),
            "body": node.get("body"),
            "submitted_at": node.get("submittedAt"),
            "commit_id": node.pointer("/commit/oid"),
        })
    })
}

pub(super) fn map_graphql_commits(pr: &Value) -> Vec<Value> {
    nodes(pr, "/commits/nodes", |node| {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fields and arguments `document` selects that the GitHub schema
    /// fixture does not define, walked from the root `Query` type.
    fn undefined_selections(document: &str) -> Vec<String> {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/github/graphql-schema.json"
        ))
        .expect("schema fixture");
        let mut tokens = Vec::new();
        let mut chars = document.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_alphanumeric() || c == '_' {
                let mut word = String::new();
                while let Some(&c) = chars.peek().filter(|c| c.is_alphanumeric() || **c == '_') {
                    word.push(c);
                    chars.next();
                }
                tokens.push(word);
            } else {
                if !c.is_whitespace() {
                    tokens.push(c.to_string());
                }
                chars.next();
            }
        }
        fn selection(
            tokens: &[String],
            at: &mut usize,
            ty: &str,
            types: &Value,
            missing: &mut Vec<String>,
        ) {
            while *at < tokens.len() && tokens[*at] != "}" {
                let mut name = tokens[*at].as_str();
                *at += 1;
                if tokens.get(*at).is_some_and(|t| t == ":") {
                    name = tokens[*at + 1].as_str();
                    *at += 2;
                }
                let field = &types[ty][name];
                if field.is_null() {
                    missing.push(format!("{ty}.{name}"));
                }
                if tokens.get(*at).is_some_and(|t| t == "(") {
                    *at += 1;
                    while tokens[*at] != ")" {
                        let arg = tokens[*at].as_str();
                        let known = field["args"]
                            .as_array()
                            .is_some_and(|args| args.iter().any(|a| a == arg));
                        if !field.is_null() && !known {
                            missing.push(format!("{ty}.{name}({arg})"));
                        }
                        while !matches!(tokens[*at].as_str(), "," | ")") {
                            *at += 1;
                        }
                        if tokens[*at] == "," {
                            *at += 1;
                        }
                    }
                    *at += 1;
                }
                if tokens.get(*at).is_some_and(|t| t == "{") {
                    *at += 1;
                    let child = field["type"].as_str().unwrap_or("?");
                    selection(tokens, at, child, types, missing);
                    *at += 1;
                }
            }
        }
        assert_eq!(tokens.first().map(String::as_str), Some("query"));
        let mut at = 1;
        if tokens[at] == "(" {
            while tokens[at] != ")" {
                at += 1;
            }
            at += 1;
        }
        assert_eq!(tokens[at], "{");
        at += 1;
        let mut missing = Vec::new();
        selection(&tokens, &mut at, "Query", &schema["types"], &mut missing);
        assert_eq!(at, tokens.len() - 1, "document fully walked");
        missing
    }

    /// The outgoing documents select only fields and arguments GitHub's
    /// schema defines; an undefined field fails the whole request.
    #[test]
    fn outgoing_documents_validate_against_the_github_schema() {
        let every: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal":"test","reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["body","files","comments","reviews","commits"]
        }))
        .expect("GitHub history test data should be valid");
        let wants = content_wants(&every);
        assert!(wants.files && wants.discussion && wants.reviews && wants.commits);
        let (document, variables) = pull_request_document(&wants);
        assert_eq!(undefined_selections(&document), Vec::<String>::new());
        assert_eq!(variables.len(), 4, "{document}");
        assert_eq!(
            undefined_selections(super::super::issue::CLOSING_REFERENCES_DOCUMENT),
            Vec::<String>::new()
        );
        assert_eq!(
            undefined_selections(super::super::issue::CLOSING_REFERENCE_COUNT_DOCUMENT),
            Vec::<String>::new()
        );
        // The validator rejects what GitHub rejects.
        assert_eq!(
            undefined_selections(
                "query{ repository(owner:\"a\",name:\"b\"){ pullRequest(number:1){ isMerged reviews(bogus:1){ nodes{ id } } } } }"
            ),
            ["PullRequest.isMerged", "PullRequest.reviews(bogus)"]
        );
    }

    /// Two reviews keep two identities and their reviewed commits.
    #[test]
    fn graphql_reviews_carry_distinct_rest_ids() {
        let pr = json!({"reviews":{"pageInfo":{"hasNextPage":false},"nodes":[
            {"databaseId":11,"author":{"login":"a"},"state":"APPROVED","body":"",
             "submittedAt":"2026-01-01T00:00:00Z","commit":{"oid":"c1"}},
            {"databaseId":12,"author":{"login":"b"},"state":"COMMENTED","body":"x",
             "submittedAt":"2026-01-02T00:00:00Z","commit":{"oid":"c2"}}
        ]}});
        let reviews = map_graphql_reviews(&pr);
        assert_eq!(reviews[0]["id"], 11);
        assert_eq!(reviews[1]["id"], 12);
        assert_eq!(reviews[1]["commit_id"], "c2");
    }

    #[test]
    fn graphql_fast_path_requires_two_flags_first_pages_and_no_patches() {
        let bare: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1
        }))
        .expect("GitHub history test data should be valid");
        assert!(!super::graphql_complete_collection_eligible(&bare));
        let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["body","files"]
        }))
        .expect("GitHub history test data should be valid");
        assert!(super::graphql_complete_collection_eligible(&query));
        let file_page: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["body","files"],"filePage":2
        }))
        .expect("GitHub history test data should be valid");
        assert!(!super::graphql_complete_collection_eligible(&file_page));
        // Legacy provider cursors are not part of the wire contract.
        assert!(
            HistoryItemRequest::from_row(json!({
                "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
                "sections":["body","comments"],
                "collectionPages":{"discussion":2}
            }))
            .is_err()
        );
        let paged: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["body","comments"],
            "commentPage":2
        }))
        .expect("GitHub history test data should be valid");
        assert!(!super::graphql_complete_collection_eligible(&paged));
        let patches: HistoryItemRequest = HistoryItemRequest::from_row(json!({
            "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
            "sections":["body","files","patches"]
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

    /// GitHub nulls a connection the token cannot read and reports the cause
    /// in `errors[]` beside `data`: that is a missing collection, not an
    /// empty complete one.
    #[test]
    fn a_nulled_connection_is_incomplete_not_empty() {
        let pr = json!({
            "reviews": null,
            "commits": { "pageInfo": { "hasNextPage": false }, "nodes": [] },
        });
        assert_eq!(
            super::graphql_collection_state(&pr, "reviews", true),
            super::GraphqlCollection::Incomplete
        );
        assert_eq!(
            super::graphql_collection_state(&pr, "files", true),
            super::GraphqlCollection::Incomplete
        );
        assert_eq!(
            super::graphql_collection_state(&pr, "commits", true),
            super::GraphqlCollection::Complete
        );
        assert_eq!(
            super::graphql_collection_state(&pr, "reviews", false),
            super::GraphqlCollection::Unused
        );
    }
}
