//! GraphQL pull-request fast path.
//!
//! One GraphQL request can serve a first page of several PR collections at
//! once. This module decides eligibility, runs that request, reads on from a
//! connection's end cursor until the first page is covered, and projects the
//! GraphQL shape into the REST-shaped values the rest of the tool consumes,
//! with per-collection completeness state.

use super::util::str_at;
use super::window::{MAX_COLLECTION_BATCHES, MAX_FILE_BATCHES, MAX_PR_COMMIT_BATCHES};
use super::{HistoryItemRequest, ItemOperation, Section};
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use octocode_github::{GitHubTransport, ProviderError, RequestContext};
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

/// What GraphQL serves for one read: the wanted collections that no filter
/// narrows. A filtered collection needs every provider batch, which REST
/// reads at once from the PR's counts with full rows (a rename's previous
/// path, which an `include` scope matches); GraphQL would page it one
/// cursor at a time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct GraphqlWants {
    pub(super) body: bool,
    pub(super) files: bool,
    pub(super) discussion: bool,
    pub(super) reviews: bool,
    pub(super) commits: bool,
}

impl GraphqlWants {
    pub(super) fn of(query: &HistoryItemRequest) -> Self {
        let wants = query.sections();
        let unmatched = super::text::needle(query).is_none();
        Self {
            body: wants.has(Section::Body),
            files: wants.lists_files()
                && unmatched
                && !gh_get_history_item_query::has_file_filter(&query.query),
            discussion: wants.has(Section::Comments) && unmatched,
            reviews: wants.has(Section::Reviews) && unmatched,
            commits: wants.lists_commits(),
        }
    }

    fn count(self) -> usize {
        [
            self.body,
            self.files,
            self.discussion,
            self.reviews,
            self.commits,
        ]
        .into_iter()
        .filter(|wanted| *wanted)
        .count()
    }
}

/// GraphQL pays off only for a first page of at least two collections it
/// serves and no patches (GraphQL returns no patch text).
pub(super) fn graphql_complete_collection_eligible(query: &HistoryItemRequest) -> bool {
    if gh_get_history_item_query::operation(query) != ItemOperation::PullRequest {
        return false;
    }
    if gh_get_history_item_query::page(query).unwrap_or(1) > 1
        || query.sections().has(Section::Patches)
    {
        return false;
    }
    GraphqlWants::of(query).count() >= 2
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

/// One paged PR connection: its response key, the field (with its alias)
/// it selects, its page-size variable, the node fields, and where the PR
/// counts its items (`total`, a pointer into the PR), when it does.
struct Connection {
    key: &'static str,
    field: &'static str,
    variable: &'static str,
    nodes: &'static str,
    /// A connection with a count beside it is read by REST past its first
    /// GraphQL page: REST sizes every batch from the count and reads them
    /// in one concurrent wave, where the cursor walks one spaced GraphQL
    /// request at a time.
    total: Option<&'static str>,
}

const FILES: Connection = Connection {
    key: "files",
    field: "files",
    variable: "files",
    nodes: "path additions deletions changeType",
    total: Some("/changedFiles"),
};
const DISCUSSION: Connection = Connection {
    key: "commentsConn",
    field: "commentsConn: comments",
    variable: "discussion",
    nodes: "databaseId author{ login __typename } body createdAt url",
    total: Some("/comments/totalCount"),
};
const REVIEWS: Connection = Connection {
    key: "reviews",
    field: "reviews",
    variable: "reviews",
    nodes: "databaseId author{ login __typename } state body submittedAt commit{ oid }",
    total: None,
};
const COMMITS: Connection = Connection {
    key: "commits",
    field: "commits",
    variable: "commits",
    nodes: "commit{ oid message messageHeadline authoredDate author{ name user{ login } } }",
    total: None,
};

/// Nodes per connection page: GitHub's maximum, the size of one REST batch.
const CONNECTION_PAGE: u32 = 100;

impl Connection {
    /// The connection's selection; `after` binds `$after` (a continuation).
    fn selection(&self, after: bool) -> String {
        let after = if after { ",after:$after" } else { "" };
        format!(
            "{}(first:${}{after}){{ pageInfo{{ hasNextPage endCursor }} nodes{{ {} }} }}",
            self.field, self.variable, self.nodes
        )
    }

    /// The document that reads this connection's next page.
    fn continuation_document(&self) -> String {
        format!(
            "query($owner:String!,$repo:String!,$number:Int!,${}:Int!,$after:String!){{ repository(owner:$owner,name:$repo){{ pullRequest(number:$number){{ {} }} }} }}",
            self.variable,
            self.selection(true)
        )
    }
}

/// The pull-request document for the collections GraphQL serves, and the
/// page size variable each collection binds. The body is selected only when
/// the read shows it (`body`); a row without it leaves `body` unknown, not
/// empty.
pub(super) fn pull_request_document(wants: &GraphqlWants) -> (String, Vec<(&'static str, u32)>) {
    let mut selections: Vec<String> = [
        "number title url state isDraft author { login }",
        "labels(first:20){ pageInfo{ hasNextPage } nodes { name } }",
        "baseRefName baseRefOid headRefName headRefOid createdAt updatedAt closedAt mergedAt mergeCommit { oid }",
        "comments { totalCount } changedFiles additions deletions",
        // Totals only: an alias keeps the count apart from a paged
        // `commits(first:)` selection of the same connection.
        "commitsCount: commits { totalCount } reviewThreads { totalCount }",
    ]
    .map(str::to_owned)
    .into();
    if wants.body {
        selections.push("body".to_owned());
    }
    let mut variables = Vec::new();
    let mut header = String::from("query($owner:String!,$repo:String!,$number:Int!");
    for (wanted, connection) in [
        (wants.files, &FILES),
        (wants.discussion, &DISCUSSION),
        (wants.reviews, &REVIEWS),
        (wants.commits, &COMMITS),
    ] {
        if wanted {
            selections.push(connection.selection(false));
            variables.push((connection.variable, CONNECTION_PAGE));
            header.push_str(&format!(",${}:Int!", connection.variable));
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
fn graphql_failure(errors: &[octocode_github::GraphQlError]) -> String {
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

/// Fetch the PR and its wanted collections in one GraphQL request, then
/// read on from each connection's end cursor until its first page is held.
pub(super) async fn graphql_pull_request(
    transport: &GitHubTransport,
    query: &HistoryItemRequest,
    context: &RequestContext,
) -> Result<GraphqlOutcome, ProviderError> {
    if !transport.graphql_enabled || !transport.graphql_available(context) {
        return Ok(GraphqlOutcome::Unavailable);
    }
    let Some(number) = gh_get_history_item_query::number(query) else {
        return Ok(GraphqlOutcome::Unavailable);
    };
    let wants = GraphqlWants::of(query);
    let (document, bound) = pull_request_document(&wants);
    let mut variables = json!({
        "owner": gh_get_history_item_query::owner(query),
        "repo": gh_get_history_item_query::repo(query),
        "number": number
    });
    for (variable, first) in bound {
        variables[variable] = json!(first);
    }
    let mut page = transport
        .execute_graphql(&document, variables.clone(), context)
        .await?;
    let mut pr = page
        .data
        .as_mut()
        .and_then(|value| value.pointer_mut("/repository/pullRequest"))
        .map(Value::take)
        .unwrap_or(Value::Null);
    if pr.is_null() {
        return Ok(GraphqlOutcome::Failed(graphql_failure(&page.errors)));
    }
    let collection_page = gh_get_history_item_query::collection_page_size(query);
    let file_page = super::inventory::file_page_size(query, false);
    // Each wanted connection, the nodes its first page needs, and the
    // pages REST would read at most.
    let plans = [
        (&FILES, wants.files, file_page, MAX_FILE_BATCHES),
        (
            &DISCUSSION,
            wants.discussion,
            collection_page,
            MAX_COLLECTION_BATCHES,
        ),
        (
            &REVIEWS,
            wants.reviews,
            collection_page,
            MAX_COLLECTION_BATCHES,
        ),
        (
            &COMMITS,
            wants.commits,
            collection_page,
            MAX_PR_COMMIT_BATCHES,
        ),
    ];
    let mut states = [GraphqlCollection::Unused; 4];
    for (slot, (connection, wanted, need, max_pages)) in states.iter_mut().zip(plans) {
        if wanted {
            *slot = read_on(
                transport, context, &variables, &mut pr, connection, need, max_pages,
            )
            .await?;
        }
    }
    let [files, discussion, reviews, commits] = states;
    Ok(GraphqlOutcome::Served(Box::new(GraphqlPr {
        files,
        discussion,
        reviews,
        commits,
        raw: map_graphql_pr_metadata(&pr),
        source: pr,
    })))
}

/// Read `connection` on from its end cursor, appending every page's nodes
/// to `pr`, until it ends, `need` nodes are held, or `max_pages` pages were
/// read; then its state. A connection the PR counts is never read on: REST
/// reads the rest in one wave ([`Connection::total`]). A continuation that
/// fails (other than by cancellation or timeout) leaves the connection to
/// REST.
async fn read_on(
    transport: &GitHubTransport,
    context: &RequestContext,
    variables: &Value,
    pr: &mut Value,
    connection: &Connection,
    need: usize,
    max_pages: usize,
) -> Result<GraphqlCollection, ProviderError> {
    let mut pages = 1;
    loop {
        let state = graphql_collection_state(pr, connection.key);
        let held = &pr[connection.key];
        if state == GraphqlCollection::Complete || held.is_null() {
            return Ok(state);
        }
        if held["nodes"].as_array().map_or(0, Vec::len) >= need {
            return Ok(GraphqlCollection::Partial);
        }
        if connection
            .total
            .is_some_and(|total| pr.pointer(total).is_some_and(Value::is_u64))
        {
            return Ok(GraphqlCollection::Incomplete);
        }
        let Some(cursor) = held
            .pointer("/pageInfo/endCursor")
            .and_then(Value::as_str)
            .filter(|_| pages < max_pages)
            .map(str::to_owned)
        else {
            return Ok(GraphqlCollection::Incomplete);
        };
        let mut next = variables.clone();
        next[connection.variable] = json!(CONNECTION_PAGE);
        next["after"] = json!(cursor);
        let mut answer = match transport
            .execute_graphql(&connection.continuation_document(), next, context)
            .await
        {
            Ok(answer) => answer,
            Err(error) if error.is_cancelled_or_timed_out() => {
                return Err(error);
            }
            Err(_) => return Ok(GraphqlCollection::Incomplete),
        };
        let Some(page) = answer
            .data
            .as_mut()
            .and_then(|data| {
                data.pointer_mut(&format!("/repository/pullRequest/{}", connection.key))
            })
            .filter(|page| page.is_object())
        else {
            return Ok(GraphqlCollection::Incomplete);
        };
        let nodes = match page.get_mut("nodes").map(Value::take) {
            Some(Value::Array(nodes)) if !nodes.is_empty() => nodes,
            _ => return Ok(GraphqlCollection::Incomplete),
        };
        let held = &mut pr[connection.key];
        if let Some(list) = held.get_mut("nodes").and_then(Value::as_array_mut) {
            list.extend(nodes);
        }
        held["pageInfo"] = page["pageInfo"].take();
        pages += 1;
    }
}

/// Whether a paginated PR sub-collection was fully returned by the GraphQL
/// query, returned in part that covers its first page (more pages remain),
/// left for REST, or not requested.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GraphqlCollection {
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

pub(super) fn map_graphql_comments(pr: &Value) -> Vec<Value> {
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

pub(super) fn map_graphql_reviews(pr: &Value) -> Vec<Value> {
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

pub(super) fn map_graphql_commits(pr: &Value) -> Vec<Value> {
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
                }
            }
        })
    })
}

#[cfg(test)]
mod tests {
    /// A GraphQL `Bot` author reads as the REST `<login>[bot]`, so both
    /// paths name review-app comments and reviews alike.
    #[test]
    fn graphql_bot_authors_get_the_rest_bot_suffix() {
        let pr = serde_json::json!({
            "commentsConn": {"nodes": [
                {"databaseId": 1, "author": {"login": "cubic-dev-ai", "__typename": "Bot"}, "body": "b"},
                {"databaseId": 2, "author": {"login": "dev", "__typename": "User"}, "body": "b"}
            ]},
            "reviews": {"nodes": [
                {"databaseId": 3, "author": {"login": "cubic-dev-ai", "__typename": "Bot"}, "state": "COMMENTED"}
            ]}
        });
        let comments = super::map_graphql_comments(&pr);
        assert_eq!(comments[0]["user"]["login"], "cubic-dev-ai[bot]");
        assert_eq!(comments[1]["user"]["login"], "dev");
        assert_eq!(
            super::map_graphql_reviews(&pr)[0]["user"]["login"],
            "cubic-dev-ai[bot]"
        );
    }

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
        assert_eq!(variables.len(), 4, "{document}");
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
            "sections":["body","files"],"page":2
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
            "page":2
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
            super::graphql_collection_state(&pr, "files"),
            super::GraphqlCollection::Complete
        );
        let incomplete = json!({"files":{"pageInfo":{"hasNextPage":true},"nodes":[]}});
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

    /// The GraphQL PR carries the target-branch tip, the commit
    /// and review-thread totals, and each commit's account login beside its
    /// git name, in the REST shape.
    #[test]
    fn graphql_pr_maps_target_sha_totals_and_commit_logins() {
        let pr = json!({
            "baseRefName":"main","baseRefOid":"b1","commitsCount":{"totalCount":4},
            "reviewThreads":{"totalCount":2},
            "commits":{"pageInfo":{"hasNextPage":false},"nodes":[{"commit":{
                "oid":"c1","message":"m","authoredDate":"2026-01-01T00:00:00Z",
                "author":{"name":"Ricky","user":{"login":"rickhanlonii"}}}},
                {"commit":{"oid":"c2","message":"m","author":{"name":"Bot"}}}]}
        });
        let metadata = map_graphql_pr_metadata(&pr);
        assert_eq!(metadata["base"], json!({"ref":"main","sha":"b1"}));
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
        let pr = json!({
            "reviews": null,
            "commits": { "pageInfo": { "hasNextPage": false }, "nodes": [] },
        });
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
}
