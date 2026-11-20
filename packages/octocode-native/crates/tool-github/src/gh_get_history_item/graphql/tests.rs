//! GraphQL eligibility, documents against GitHub's schema, and projection.

/// A GraphQL `Bot` author reads as the REST `<login>[bot]`, so both
/// paths name review-app comments and reviews alike.
#[test]
fn graphql_bot_authors_get_the_rest_bot_suffix() {
    let pr = json_fixture(
        r#"{
            "commentsConn": {
                "nodes": [
                    {
                        "databaseId": 1,
                        "author": {"login": "cubic-dev-ai", "__typename": "Bot"},
                        "body": "b"
                    },
                    {
                        "databaseId": 2,
                        "author": {"login": "dev", "__typename": "User"},
                        "body": "b"
                    }
                ]
            },
            "reviews": {
                "nodes": [
                    {
                        "databaseId": 3,
                        "author": {"login": "cubic-dev-ai", "__typename": "Bot"},
                        "state": "COMMENTED"
                    }
                ]
            }
        }"#,
    );
    let comments = super::map_graphql_comments(&pr);
    assert_eq!(comments[0]["user"]["login"], "cubic-dev-ai[bot]");
    assert_eq!(comments[1]["user"]["login"], "dev");
    assert_eq!(
        super::map_graphql_reviews(&pr)[0]["user"]["login"],
        "cubic-dev-ai[bot]"
    );
}

/// One comment rule on both paths: a GraphQL discussion comment states
/// when it was edited as a REST one does (`updatedAt` only when it differs
/// from `createdAt`).
#[test]
fn graphql_comments_state_an_edit_as_rest_ones_do() {
    let pr = json_fixture(
        r#"{
            "commentsConn": {
                "nodes": [
                    {
                        "databaseId": 1,
                        "author": {"login": "dev"},
                        "body": "edited",
                        "createdAt": "2024-01-01T00:00:00Z",
                        "updatedAt": "2024-01-02T00:00:00Z"
                    },
                    {
                        "databaseId": 2,
                        "author": {"login": "dev"},
                        "body": "as written",
                        "createdAt": "2024-01-01T00:00:00Z",
                        "updatedAt": "2024-01-01T00:00:00Z"
                    }
                ]
            }
        }"#,
    );
    let rows = crate::gh_get_history_item::util::map_comments(super::map_graphql_comments(&pr));
    assert_eq!(rows[0]["updatedAt"], "2024-01-02T00:00:00Z", "{rows:?}");
    assert!(rows[1].get("updatedAt").is_none(), "{rows:?}");
}

use super::*;
use octocode_test_support::json_fixture;

mod single_connection;

/// Fields and arguments `document` selects that the GitHub schema
/// fixture does not define, walked from the root `Query` type.
fn undefined_selections(document: &str) -> Vec<String> {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../runtime/tests/fixtures/github/graphql-schema.json"
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
            // `... on T { }`: the fragment selects from `T`, which the
            // current type, when a union, must list as a member.
            if tokens[*at] == "." {
                assert!(
                    tokens[*at..*at + 4] == [".", ".", ".", "on"],
                    "inline fragments only: {:?}",
                    &tokens[*at..(*at + 4).min(tokens.len())]
                );
                let target = tokens[*at + 4].as_str();
                *at += 5;
                if types[target].is_null() {
                    missing.push(format!("{ty}...on {target}"));
                } else if let Some(members) = types[ty]["$union"].as_array()
                    && !members.iter().any(|member| member == target)
                {
                    missing.push(format!("{ty}...on {target}"));
                }
                assert_eq!(tokens[*at], "{", "fragment selection");
                *at += 1;
                selection(tokens, at, target, types, missing);
                *at += 1;
                continue;
            }
            let mut name = tokens[*at].as_str();
            *at += 1;
            if tokens.get(*at).is_some_and(|t| t == ":") {
                name = tokens[*at + 1].as_str();
                *at += 2;
            }
            let field = &types[ty][name];
            // `__typename` is a meta-field every object type has.
            if field.is_null() && name != "__typename" {
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
                    // A value may hold a list (`[A,B]`): its commas are its own.
                    let mut depth = 0usize;
                    while depth > 0 || !matches!(tokens[*at].as_str(), "," | ")") {
                        match tokens[*at].as_str() {
                            "[" | "{" => depth += 1,
                            "]" | "}" => depth -= 1,
                            _ => {}
                        }
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
    let every: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 1,
            "sections": ["body", "files", "comments", "reviews", "commits"]
        }"#,
    ))
    .expect("GitHub history test data should be valid");
    let wants = GraphqlWants::of(&every);
    assert_eq!(
        wants,
        GraphqlWants {
            body: true,
            files: true,
            discussion: true,
            reviews: true,
            commits: true
        }
    );
    let (document, variables) = pull_request_document(&wants);
    // The PR's own body is selected only when the read uses it.
    assert!(document.contains("totalCount } body "), "{document}");
    let (without_body, _) = pull_request_document(&GraphqlWants {
        body: false,
        ..wants
    });
    assert!(
        !without_body.contains("totalCount } body "),
        "{without_body}"
    );
    assert_eq!(undefined_selections(&document), Vec::<String>::new());
    assert_eq!(variables.len(), wants.count(), "{document}");
    assert!(
        !document.contains("labels("),
        "section reads do not show labels: {document}"
    );
    // Each connection's continuation reads on from its end cursor.
    for connection in [&FILES, &DISCUSSION, &REVIEWS, &COMMITS] {
        let next = connection.continuation_document();
        assert!(next.contains("after:$after"), "{next}");
        assert_eq!(undefined_selections(&next), Vec::<String>::new());
    }
    assert_eq!(
        undefined_selections(super::super::issue::CLOSING_REFERENCES_DOCUMENT),
        Vec::<String>::new()
    );
    assert_eq!(
        undefined_selections(super::super::issue::CLOSING_FIRST_DOCUMENT),
        Vec::<String>::new()
    );
    // An inline fragment is checked against its type and the union it sits in.
    assert_eq!(
        undefined_selections(
            "query{ repository(owner:\"a\",name:\"b\"){ issue(number:1){ timelineItems(last:1){ nodes{ __typename ... on ClosedEvent{ closer{ ... on Commit{ oid } ... on Label{ id } } bogus } ... on Nope{ id } } } } } }"
        ),
        [
            "Closer...on Label",
            "ClosedEvent.bogus",
            "IssueTimelineItems...on Nope",
            "Nope.id"
        ]
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
    let pr = json_fixture(
        r#"{
            "reviews": {
                "pageInfo": {"hasNextPage": false},
                "nodes": [
                    {
                        "databaseId": 11,
                        "author": {"login": "a"},
                        "state": "APPROVED",
                        "body": "",
                        "submittedAt": "2026-01-01T00:00:00Z",
                        "commit": {"oid": "c1"}
                    },
                    {
                        "databaseId": 12,
                        "author": {"login": "b"},
                        "state": "COMMENTED",
                        "body": "x",
                        "submittedAt": "2026-01-02T00:00:00Z",
                        "commit": {"oid": "c2"}
                    }
                ]
            }
        }"#,
    );
    let reviews = map_graphql_reviews(&pr);
    assert_eq!(reviews[0]["id"], 11);
    assert_eq!(reviews[1]["id"], 12);
    assert_eq!(reviews[1]["commit_id"], "c2");
}

#[test]
fn graphql_fast_path_requires_first_pages_and_no_patches() {
    let bare: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 1
        }"#,
    ))
    .expect("GitHub history test data should be valid");
    assert!(!super::graphql_complete_collection_eligible(&bare));
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 1,
            "sections": ["body", "files"]
        }"#,
    ))
    .expect("GitHub history test data should be valid");
    assert!(super::graphql_complete_collection_eligible(&query));
    let file_page: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 1,
            "sections": ["body", "files"],
            "page": 2
        }"#,
    ))
    .expect("GitHub history test data should be valid");
    assert!(!super::graphql_complete_collection_eligible(&file_page));
    // Legacy provider cursors are not part of the wire contract.
    assert!(
        HistoryItemRequest::from_row(json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 1,
                "sections": ["body", "comments"],
                "collectionPages": {"discussion": 2}
            }"#
        ))
        .is_err()
    );
    let paged: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 1,
            "sections": ["body", "comments"],
            "page": 2
        }"#,
    ))
    .expect("GitHub history test data should be valid");
    assert!(!super::graphql_complete_collection_eligible(&paged));
    let patches: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 1,
            "sections": ["body", "files", "patches"]
        }"#,
    ))
    .expect("GitHub history test data should be valid");
    assert!(!super::graphql_complete_collection_eligible(&patches));
}

#[test]
fn graphql_files_map_to_rest_filename_without_patch() {
    let pr = json_fixture(
        r#"{
            "files": {
                "pageInfo": {"hasNextPage": false},
                "nodes": [
                    {
                        "path": "src/lib.rs",
                        "additions": 1,
                        "deletions": 2,
                        "changeType": "ADDED"
                    }
                ]
            }
        }"#,
    );
    let files = super::map_graphql_files(&pr);
    assert_eq!(files[0]["filename"], "src/lib.rs");
    assert_eq!(files[0]["status"], "added");
    assert!(files[0].get("patch").is_none());
    assert_eq!(
        super::graphql_collection_state(&pr, "files"),
        super::GraphqlCollection::Complete
    );
    let incomplete = json_fixture(r#"{"files": {"pageInfo": {"hasNextPage": true}, "nodes": []}}"#);
    assert_eq!(
        super::graphql_collection_state(&incomplete, "files"),
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
    let pr = json_fixture(
        r#"{
            "labels": {
                "pageInfo": {"hasNextPage": true},
                "nodes": [{"name": "bug"}]
            },
            "commits": {
                "pageInfo": {"hasNextPage": false},
                "nodes": [
                    {
                        "commit": {
                            "oid": "abc",
                            "message": "Fix parser\n\nCo-Authored-By: A <a@example.com>",
                            "messageHeadline": "Fix parser",
                            "authoredDate": "2026-01-01T00:00:00Z",
                            "committedDate": "2026-01-02T00:00:00Z",
                            "author": {"user": {"login": "octo"}}
                        }
                    }
                ]
            }
        }"#,
    );
    let commits = map_graphql_commits(&pr);
    assert_eq!(
        commits[0]["commit"]["message"],
        "Fix parser\n\nCo-Authored-By: A <a@example.com>"
    );
    // A GraphQL commit dates its row like a REST one: by its committer.
    assert_eq!(
        octocode_tools_shared::gh_shared::commit::commit_date(&commits[0]),
        "2026-01-02T00:00:00Z"
    );
    let metadata = map_graphql_pr_metadata(&pr);
    assert_eq!(metadata["labels"], json_fixture(r#"["bug"]"#));
    assert_eq!(metadata["labels_truncated"], true);
    let complete = map_graphql_pr_metadata(&json_fixture(
        r#"{
            "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []}
        }"#,
    ));
    assert_eq!(complete["labels_truncated"], false);
}

/// The GraphQL PR carries the target-branch tip, the commit
/// and review-thread totals, and each commit's account login beside its
/// git name, in the REST shape.
#[test]
fn graphql_pr_maps_target_sha_totals_and_commit_logins() {
    let pr = json_fixture(
        r#"{
            "baseRefName": "main",
            "baseRefOid": "b1",
            "commitsCount": {"totalCount": 4},
            "reviewThreads": {"totalCount": 2},
            "commits": {
                "pageInfo": {"hasNextPage": false},
                "nodes": [
                    {
                        "commit": {
                            "oid": "c1",
                            "message": "m",
                            "authoredDate": "2026-01-01T00:00:00Z",
                            "author": {"name": "Ricky", "user": {"login": "rickhanlonii"}}
                        }
                    },
                    {
                        "commit": {"oid": "c2", "message": "m", "author": {"name": "Bot"}}
                    }
                ]
            }
        }"#,
    );
    let metadata = map_graphql_pr_metadata(&pr);
    assert_eq!(
        metadata["base"],
        json_fixture(r#"{"ref": "main", "sha": "b1"}"#)
    );
    assert_eq!(metadata["commits"], 4);
    assert_eq!(metadata["review_threads"], 2);
    let commits = map_graphql_commits(&pr);
    assert_eq!(commits[0]["author"]["login"], "rickhanlonii");
    assert_eq!(commits[0]["commit"]["author"]["name"], "Ricky");
    assert!(commits[1]["author"].is_null());
    assert_eq!(commits[1]["commit"]["author"]["name"], "Bot");
}

/// GitHub nulls a connection the token cannot read and reports the cause
/// in `errors[]` beside `data`: that is a missing collection, not an
/// empty complete one.
#[test]
fn a_nulled_connection_is_incomplete_not_empty() {
    let pr = json_fixture(
        r#"{
            "reviews": null,
            "commits": {"pageInfo": {"hasNextPage": false}, "nodes": []}
        }"#,
    );
    assert_eq!(
        super::graphql_collection_state(&pr, "reviews"),
        super::GraphqlCollection::Incomplete
    );
    assert_eq!(
        super::graphql_collection_state(&pr, "files"),
        super::GraphqlCollection::Incomplete
    );
    assert_eq!(
        super::graphql_collection_state(&pr, "commits"),
        super::GraphqlCollection::Complete
    );
}

/// GraphQL's `mergedBy` projects into the REST `merged_by` shape the row
/// reads.
#[test]
fn graphql_projects_who_merged() {
    let mapped = map_graphql_pr_metadata(&json!({
        "mergedAt": "2026-01-04T00:00:00Z", "mergedBy": {"login": "maintainer"}}));
    assert_eq!(mapped["merged_by"]["login"], "maintainer");
}
