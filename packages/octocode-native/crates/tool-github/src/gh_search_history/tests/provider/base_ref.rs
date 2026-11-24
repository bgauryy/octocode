//! A pull request's target branch (`baseRef`): a REST listing's item names
//! it, a search item does not, so a search page whose `fields` pick names
//! it reads each row's `baseRefName` in the page's one GraphQL request.
use super::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Two merged pull requests as GitHub's issue search lists them: no `base`.
async fn mount_search(server: &MockServer) {
    let item = |number: u64, node: &str| {
        json!({"number": number, "node_id": node, "title": format!("pr {number}"),
            "state": "closed", "user": {"login": "dev"}, "comments": 3,
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-02-01T00:00:00Z",
            "pull_request": {"merged_at": "2024-03-01T00:00:00Z"}})
    };
    mount_json(
        server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 2, "incomplete_results": false,
            "items": [item(14291, "PR_a"), item(14693, "PR_b")]}),
    )
    .await;
}

/// GraphQL's answer for both nodes: each one's target branch and whole count.
async fn mount_nodes(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data": {"nodes": [
                {"id": "PR_a", "baseRefName": "main", "totalCommentsCount": 12},
                {"id": "PR_b", "baseRefName": "v4", "totalCommentsCount": 7}
            ]}})),
        )
        .mount(server)
        .await;
}

async fn search(server: &MockServer, fields: Value) -> Value {
    let query = serde_json::from_value(json!({
        "operation": "pullRequest", "owner": "vitejs", "repo": "vite",
        "keywords": ["warmup"], "filters": {"isMerged": true}, "fields": fields
    }))
    .expect("query");
    execute(
        &one_shot_provider(server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("pull request search")
}

fn column(data: &Value, key: &str) -> Vec<Value> {
    data["pullRequests"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row[key].clone())
        .collect()
}

async fn graphql_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("requests")
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .count()
}

fn warns_of(data: &Value, key: &str) -> bool {
    data["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|warning| warning.as_str().is_some_and(|text| text.contains(key)))
}

/// A search page picked for `baseRef` states each row's target branch, as
/// a listing does, read in one GraphQL request; no "no row has" warning.
#[tokio::test]
async fn a_search_picked_for_base_ref_states_each_rows_target_branch() {
    let server = MockServer::builder().start().await;
    mount_search(&server).await;
    mount_nodes(&server).await;
    let data = search(&server, json!(["number", "baseRef"])).await;
    assert_eq!(
        column(&data, "baseRef"),
        [json!("main"), json!("v4")],
        "{data}"
    );
    assert!(!warns_of(&data, "baseRef"), "{data}");
    assert_eq!(graphql_requests(&server).await, 1);
}

/// A pick of both GraphQL facts still reads the page once.
#[tokio::test]
async fn a_pick_of_base_ref_and_whole_count_reads_graphql_once() {
    let server = MockServer::builder().start().await;
    mount_search(&server).await;
    mount_nodes(&server).await;
    let data = search(&server, json!(["baseRef", "totalCommentsCount"])).await;
    assert_eq!(
        column(&data, "baseRef"),
        [json!("main"), json!("v4")],
        "{data}"
    );
    assert_eq!(
        column(&data, "totalCommentsCount"),
        [json!(12), json!(7)],
        "{data}"
    );
    assert_eq!(graphql_requests(&server).await, 1);
}

/// A pick without `baseRef` (or the whole count) reads no GraphQL.
#[tokio::test]
async fn a_pick_without_base_ref_reads_no_graphql() {
    let server = MockServer::builder().start().await;
    mount_search(&server).await;
    mount_nodes(&server).await;
    let data = search(&server, json!(["title", "mergedAt"])).await;
    assert_eq!(
        column(&data, "baseRef"),
        [Value::Null, Value::Null],
        "{data}"
    );
    assert_eq!(graphql_requests(&server).await, 0);
}

/// Without GraphQL the rows state no target branch they cannot read, and
/// the page says no row has it.
#[tokio::test]
async fn without_graphql_a_base_ref_pick_states_none() {
    let server = MockServer::builder().start().await;
    mount_search(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    let data = search(&server, json!(["number", "baseRef"])).await;
    assert_eq!(
        column(&data, "baseRef"),
        [Value::Null, Value::Null],
        "{data}"
    );
    assert!(warns_of(&data, "baseRef"), "{data}");
}
