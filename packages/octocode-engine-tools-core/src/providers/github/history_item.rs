use super::{
    CredentialResolver, GitHubTransport, GraphQlPage, ProviderError, ProviderErrorKind,
    RequestContext, RequestSpec,
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

pub const GRAPHQL_FILES_FIRST: usize = 100;
pub const GRAPHQL_DISCUSSION_FIRST: usize = 100;
pub const GRAPHQL_REVIEWS_FIRST: usize = 100;
pub const GRAPHQL_COMMITS_FIRST: usize = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphqlItemKind {
    PullRequest,
    Issue,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GraphqlHistoryWanted {
    pub body: bool,
    pub files: bool,
    pub discussion: bool,
    pub reviews: bool,
    pub commits: bool,
}

#[derive(Clone, Debug)]
pub struct GraphqlCollection {
    pub nodes: Vec<Value>,
    pub complete: bool,
}

#[derive(Clone, Debug)]
pub struct GraphqlHistoryItem {
    pub raw: Value,
    pub files: Option<GraphqlCollection>,
    pub discussion: Option<GraphqlCollection>,
    pub reviews: Option<GraphqlCollection>,
    pub commits: Option<GraphqlCollection>,
}

#[derive(Clone, Debug)]
pub struct HistoryItemResponse {
    pub value: Value,
    pub has_more: bool,
}

fn graphql_skip_hosts() -> &'static Mutex<HashSet<String>> {
    static HOSTS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    HOSTS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn graphql_skip_key(transport: &GitHubTransport<impl CredentialResolver>) -> String {
    let url = transport.endpoint().graphql();
    format!(
        "{}://{}:{}",
        url.scheme(),
        url.host_str().unwrap_or_default(),
        url.port_or_known_default().unwrap_or(0)
    )
}

fn mark_graphql_skipped(transport: &GitHubTransport<impl CredentialResolver>) {
    if let Ok(mut hosts) = graphql_skip_hosts().lock() {
        hosts.insert(graphql_skip_key(transport));
    }
}

fn graphql_rate_limited(page: &GraphQlPage) -> bool {
    page.errors.iter().any(|error| {
        error.type_name.as_deref() == Some("RATE_LIMITED")
            || error.extensions.get("code").and_then(Value::as_str) == Some("RATE_LIMITED")
            || error.extensions.get("type").and_then(Value::as_str) == Some("RATE_LIMITED")
    })
}

pub(crate) fn build_history_graphql(
    kind: GraphqlItemKind,
    wanted: GraphqlHistoryWanted,
    owner: &str,
    name: &str,
    number: u64,
) -> (String, Value) {
    let mut var_defs: Vec<String> = vec![
        "$owner: String!".into(),
        "$name: String!".into(),
        "$number: Int!".into(),
    ];
    let mut variables = json!({
        "owner": owner,
        "name": name,
        "number": number,
    });
    let mut fields: Vec<String> = match kind {
        GraphqlItemKind::PullRequest => vec![
            "number title state isDraft isMerged".into(),
            "author { login }".into(),
            "labels(first: 20) { nodes { name } }".into(),
            "baseRefName headRefName headRefOid".into(),
            "createdAt updatedAt closedAt mergedAt".into(),
            "comments { totalCount }".into(),
            "changedFiles additions deletions".into(),
            "body".into(),
        ],
        GraphqlItemKind::Issue => vec![
            "number title state".into(),
            "author { login }".into(),
            "labels(first: 20) { nodes { name } }".into(),
            "createdAt updatedAt closedAt".into(),
            "body".into(),
        ],
    };
    if wanted.files {
        var_defs.push("$files: Int!".into());
        variables["files"] = json!(GRAPHQL_FILES_FIRST);
        fields.push(
            "files(first: $files) { pageInfo { hasNextPage } nodes { path additions deletions changeType } }"
                .into(),
        );
    }
    if wanted.reviews {
        var_defs.push("$reviews: Int!".into());
        variables["reviews"] = json!(GRAPHQL_REVIEWS_FIRST);
        fields.push(
            "reviews(first: $reviews) { pageInfo { hasNextPage } nodes { databaseId author { login } state body submittedAt } }"
                .into(),
        );
    }
    if wanted.commits {
        var_defs.push("$commits: Int!".into());
        variables["commits"] = json!(GRAPHQL_COMMITS_FIRST);
        fields.push(
            "commits(first: $commits) { pageInfo { hasNextPage } nodes { commit { oid messageHeadline authoredDate author { user { login } } } } }"
                .into(),
        );
    }
    if wanted.discussion {
        var_defs.push("$discussion: Int!".into());
        variables["discussion"] = json!(GRAPHQL_DISCUSSION_FIRST);
        let comments_field = match kind {
            GraphqlItemKind::PullRequest => {
                "commentsConn: comments(first: $discussion) { pageInfo { hasNextPage } nodes { databaseId author { login } body createdAt url } }"
            }
            GraphqlItemKind::Issue => {
                "comments(first: $discussion) { pageInfo { hasNextPage } nodes { databaseId author { login } body createdAt url } }"
            }
        };
        fields.push(comments_field.into());
    }
    let field_block = fields.join(" ");
    let defs = var_defs.join(", ");
    let root = match kind {
        GraphqlItemKind::PullRequest => "pullRequest",
        GraphqlItemKind::Issue => "issue",
    };
    let sibling = match kind {
        GraphqlItemKind::Issue => " pullRequest(number: $number) { number }",
        GraphqlItemKind::PullRequest => "",
    };
    let query = format!(
        "query HistoryItem({defs}) {{ repository(owner: $owner, name: $name) {{ {root}(number: $number) {{ {field_block} }}{sibling} }} }}"
    );
    (query, variables)
}

fn rest_file_status(change_type: &str) -> Option<&'static str> {
    Some(match change_type {
        "ADDED" => "added",
        "DELETED" => "removed",
        "MODIFIED" => "modified",
        "RENAMED" => "renamed",
        "COPIED" => "copied",
        "CHANGED" => "changed",
        _ => return None,
    })
}

fn connection_complete(
    conn: &Value,
    map_node: impl Fn(&Value) -> Option<Value>,
) -> GraphqlCollection {
    let has_next = conn
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool);
    let Some(nodes) = conn.get("nodes").and_then(Value::as_array) else {
        return GraphqlCollection {
            nodes: Vec::new(),
            complete: false,
        };
    };
    let Some(has_next) = has_next else {
        return GraphqlCollection {
            nodes: Vec::new(),
            complete: false,
        };
    };
    let mut mapped = Vec::with_capacity(nodes.len());
    for node in nodes {
        let Some(value) = map_node(node) else {
            return GraphqlCollection {
                nodes: Vec::new(),
                complete: false,
            };
        };
        mapped.push(value);
    }
    if has_next {
        GraphqlCollection {
            nodes: Vec::new(),
            complete: false,
        }
    } else {
        GraphqlCollection {
            nodes: mapped,
            complete: true,
        }
    }
}

pub(crate) fn map_graphql_files(conn: &Value) -> GraphqlCollection {
    connection_complete(conn, |node| {
        Some(json!({
            "filename": node.get("path")?.as_str()?,
            "additions": node.get("additions")?.as_u64()?,
            "deletions": node.get("deletions")?.as_u64()?,
            "status": rest_file_status(node.get("changeType")?.as_str()?)?,
        }))
    })
}

fn map_graphql_discussion(conn: &Value) -> GraphqlCollection {
    connection_complete(conn, |node| {
        let id = node.get("databaseId").filter(|v| !v.is_null())?;
        let login = node
            .pointer("/author/login")
            .and_then(Value::as_str)
            .unwrap_or("");
        Some(json!({
            "id": id,
            "user": { "login": login },
            "body": node.get("body").and_then(Value::as_str).unwrap_or(""),
            "created_at": node.get("createdAt"),
            "updated_at": node.get("updatedAt").cloned().or_else(|| node.get("createdAt").cloned()),
            "html_url": node.get("url"),
        }))
    })
}

fn map_graphql_reviews(conn: &Value) -> GraphqlCollection {
    connection_complete(conn, |node| {
        Some(json!({
            "id": node.get("databaseId").cloned().unwrap_or(Value::Null),
            "user": { "login": node.pointer("/author/login").and_then(Value::as_str).unwrap_or("unknown") },
            "state": node.get("state").and_then(Value::as_str).unwrap_or(""),
            "body": node.get("body").and_then(Value::as_str).unwrap_or(""),
            "submitted_at": node.get("submittedAt"),
        }))
    })
}

fn map_graphql_commits(conn: &Value) -> GraphqlCollection {
    connection_complete(conn, |node| {
        let commit = node.get("commit")?;
        Some(json!({
            "sha": commit.get("oid")?.as_str()?,
            "commit": {
                "message": commit.get("messageHeadline").and_then(Value::as_str).unwrap_or(""),
                "author": {
                    "name": commit.pointer("/author/user/login").and_then(Value::as_str).unwrap_or("unknown"),
                    "date": commit.get("authoredDate").and_then(Value::as_str).unwrap_or(""),
                }
            }
        }))
    })
}

fn map_labels(item: &Value) -> Vec<Value> {
    item.pointer("/labels/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|label| {
            label
                .get("name")
                .and_then(Value::as_str)
                .map(|name| json!({ "name": name }))
        })
        .collect()
}

fn map_pr_raw(pr: &Value) -> Option<Value> {
    pr.get("number")?;
    let is_merged = pr.get("isMerged").and_then(Value::as_bool).unwrap_or(false);
    let merged_at = pr.get("mergedAt").cloned().filter(|v| !v.is_null());
    Some(json!({
        "number": pr.get("number"),
        "title": pr.get("title"),
        "body": pr.get("body"),
        "state": pr.get("state").and_then(Value::as_str).unwrap_or("OPEN").to_ascii_lowercase(),
        "draft": pr.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
        "user": { "login": pr.pointer("/author/login").and_then(Value::as_str).unwrap_or("") },
        "labels": map_labels(pr),
        "base": { "ref": pr.get("baseRefName") },
        "head": { "ref": pr.get("headRefName"), "sha": pr.get("headRefOid") },
        "created_at": pr.get("createdAt"),
        "updated_at": pr.get("updatedAt"),
        "closed_at": pr.get("closedAt"),
        "merged_at": if is_merged { merged_at } else { None },
        "comments": pr.pointer("/comments/totalCount"),
        "changed_files": pr.get("changedFiles"),
        "additions": pr.get("additions"),
        "deletions": pr.get("deletions"),
    }))
}

fn map_issue_raw(issue: &Value, pull_request: Option<&Value>) -> Option<Value> {
    issue.get("number")?;
    Some(json!({
        "number": issue.get("number"),
        "title": issue.get("title"),
        "body": issue.get("body"),
        "state": issue.get("state").and_then(Value::as_str).unwrap_or("OPEN").to_ascii_lowercase(),
        "user": { "login": issue.pointer("/author/login").and_then(Value::as_str).unwrap_or("unknown") },
        "labels": map_labels(issue),
        "created_at": issue.get("createdAt"),
        "updated_at": issue.get("updatedAt"),
        "closed_at": issue.get("closedAt"),
        "pull_request": pull_request.filter(|v| v.is_object()).cloned(),
    }))
}

fn wanted_connection(
    wanted: bool,
    item: &Value,
    keys: &[&str],
    map: impl Fn(&Value) -> GraphqlCollection,
) -> Option<GraphqlCollection> {
    if !wanted {
        return None;
    }
    for key in keys {
        if let Some(conn) = item.pointer(key) {
            return Some(map(conn));
        }
    }
    Some(GraphqlCollection {
        nodes: Vec::new(),
        complete: false,
    })
}

fn decode_graphql_item(
    kind: GraphqlItemKind,
    wanted: GraphqlHistoryWanted,
    data: &Value,
) -> Option<GraphqlHistoryItem> {
    let (raw, item) = match kind {
        GraphqlItemKind::PullRequest => {
            let pr = data.pointer("/repository/pullRequest")?;
            if !pr.is_object() {
                return None;
            }
            (map_pr_raw(pr)?, pr)
        }
        GraphqlItemKind::Issue => {
            let issue = data.pointer("/repository/issue")?;
            if !issue.is_object() {
                return None;
            }
            (
                map_issue_raw(issue, data.pointer("/repository/pullRequest"))?,
                issue,
            )
        }
    };
    Some(GraphqlHistoryItem {
        raw,
        files: wanted_connection(wanted.files, item, &["/files"], map_graphql_files),
        discussion: wanted_connection(
            wanted.discussion,
            item,
            &["/commentsConn", "/comments"],
            map_graphql_discussion,
        ),
        reviews: wanted_connection(wanted.reviews, item, &["/reviews"], map_graphql_reviews),
        commits: wanted_connection(wanted.commits, item, &["/commits"], map_graphql_commits),
    })
}

impl<R: CredentialResolver> GitHubTransport<R> {
    pub fn graphql_host_skipped(&self) -> bool {
        graphql_skip_hosts()
            .lock()
            .map(|hosts| hosts.contains(&graphql_skip_key(self)))
            .unwrap_or(false)
    }

    pub async fn history_item(
        &self,
        segments: &[&str],
        query: &[(&str, String)],
        context: &RequestContext,
    ) -> Result<HistoryItemResponse, ProviderError> {
        let mut url = self.endpoint().rest(segments)?;
        {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in query {
                pairs.append_pair(k, v);
            }
        }
        let response = self.execute(RequestSpec::get(url), context).await?;
        let value = serde_json::from_slice(&response.body).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "invalid GitHub history item response",
            )
        })?;
        Ok(HistoryItemResponse {
            value,
            has_more: response.next.is_some(),
        })
    }

    pub async fn history_item_graphql(
        &self,
        kind: GraphqlItemKind,
        owner: &str,
        name: &str,
        number: u64,
        wanted: GraphqlHistoryWanted,
        context: &RequestContext,
    ) -> Result<GraphqlHistoryItem, ProviderError> {
        let (query, variables) = build_history_graphql(kind, wanted, owner, name, number);
        let page = match self.execute_graphql(&query, variables, context).await {
            Ok(page) => page,
            Err(error) => {
                if error.kind == ProviderErrorKind::RateLimited {
                    mark_graphql_skipped(self);
                }
                return Err(error);
            }
        };
        if graphql_rate_limited(&page) {
            mark_graphql_skipped(self);
        }
        let Some(data) = page.data.as_ref().filter(|value| !value.is_null()) else {
            return Err(ProviderError::new(
                if graphql_rate_limited(&page) {
                    ProviderErrorKind::RateLimited
                } else {
                    ProviderErrorKind::Decode
                },
                "GitHub GraphQL history item returned no data",
            ));
        };
        decode_graphql_item(kind, wanted, data).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Decode,
                "GitHub GraphQL history item was missing mapped fields",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_graphql_files_to_rest_listing_without_patch() {
        let conn = json!({
            "pageInfo": { "hasNextPage": false },
            "nodes": [{
                "path": "src/lib.rs",
                "additions": 4,
                "deletions": 1,
                "changeType": "ADDED"
            }]
        });
        let mapped = map_graphql_files(&conn);
        assert!(mapped.complete);
        assert_eq!(mapped.nodes[0]["filename"], "src/lib.rs");
        assert_eq!(mapped.nodes[0]["status"], "added");
        assert_eq!(mapped.nodes[0]["additions"], 4);
        assert_eq!(mapped.nodes[0]["deletions"], 1);
        assert!(mapped.nodes[0].get("patch").is_none());
    }

    #[test]
    fn discards_graphql_files_when_has_next_page() {
        let conn = json!({
            "pageInfo": { "hasNextPage": true },
            "nodes": [{
                "path": "src/lib.rs",
                "additions": 1,
                "deletions": 0,
                "changeType": "MODIFIED"
            }]
        });
        let mapped = map_graphql_files(&conn);
        assert!(!mapped.complete);
        assert!(mapped.nodes.is_empty());
    }

    #[test]
    fn omitted_connections_never_use_first_zero_or_review_threads() {
        let (query, variables) = build_history_graphql(
            GraphqlItemKind::PullRequest,
            GraphqlHistoryWanted {
                body: true,
                files: true,
                discussion: false,
                reviews: false,
                commits: false,
            },
            "o",
            "r",
            1,
        );
        assert!(!query.contains("first: 0"));
        assert!(!query.contains("first:0"));
        assert!(!query.contains("reviewThreads"));
        assert!(!query.contains("commentsConn"));
        assert!(!query.contains("reviews("));
        assert!(!query.contains("commits("));
        assert!(query.contains("files(first: $files)"));
        assert_eq!(variables["files"], GRAPHQL_FILES_FIRST);
        assert!(variables.get("discussion").is_none());
        assert!(variables.get("reviews").is_none());
        assert!(variables.get("commits").is_none());
    }
}
