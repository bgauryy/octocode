//! A pull request row's comment count is GitHub's whole count, the one its
//! search sorts and filters by, read in one GraphQL request per page.
use super::*;
use octocode_test_support::json_fixture;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Three merged pull requests as GitHub's issue search lists them: the
/// conversation counts (59, 1, 38) do not follow the comment order.
async fn mount_search(server: &MockServer) {
    let item = |number: u64, node: &str, comments: u64| {
        json!({"number": number, "node_id": node, "title": format!("pr {number}"),
            "state": "closed", "user": {"login": "dev"}, "comments": comments,
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-02-01T00:00:00Z",
            "pull_request": {"merged_at": "2024-03-01T00:00:00Z"}})
    };
    mount_json(
        server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": 3, "incomplete_results": false,
            "items": [item(9208, "PR_a", 59), item(5172, "PR_b", 1), item(7555, "PR_c", 38)]}),
    )
    .await;
}

async fn search(server: &MockServer, row: Value) -> Value {
    let query = serde_json::from_value(row).expect("query");
    execute(
        &one_shot_provider(server),
        &query,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("pull request search")
}

fn counts(data: &Value) -> Vec<Value> {
    data["pullRequests"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["commentsCount"].clone())
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

fn sorted_by_comments() -> Value {
    json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "cli",
            "repo": "cli",
            "keywords": ["config"],
            "sort": {"by": "commentsCount", "order": "desc"}
        }"#,
    )
}

/// Rows ranked by comments state GitHub's whole count, so the order the
/// page is in is the order its counts show; one GraphQL request reads them.
#[tokio::test]
async fn pull_requests_sorted_by_comments_state_githubs_whole_count() {
    let server = MockServer::builder().start().await;
    mount_search(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data": {"nodes": [
                {"id": "PR_a", "totalCommentsCount": 137},
                {"id": "PR_b", "totalCommentsCount": 82},
                {"id": "PR_c", "totalCommentsCount": 79}
            ]}})),
        )
        .mount(&server)
        .await;
    let data = search(&server, sorted_by_comments()).await;
    assert_eq!(counts(&data), [json!(137), json!(82), json!(79)], "{data}");
    assert!(data.get("warnings").is_none(), "{data}");
    assert_eq!(graphql_requests(&server).await, 1);
}

/// A page that states no comment count reads none: no GraphQL request.
#[tokio::test]
async fn a_page_without_comment_counts_reads_none() {
    let server = MockServer::builder().start().await;
    mount_search(&server).await;
    let data = search(
        &server,
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "cli",
                "repo": "cli",
                "keywords": ["config"]
            }"#,
        ),
    )
    .await;
    assert_eq!(
        counts(&data),
        [Value::Null, Value::Null, Value::Null],
        "{data}"
    );
    assert_eq!(graphql_requests(&server).await, 0);
}

/// Without GraphQL the rows keep GitHub's conversation count, and the page
/// says once that it counts conversation comments only.
#[tokio::test]
async fn without_graphql_a_comment_count_says_it_is_conversation_only() {
    let server = MockServer::builder().start().await;
    mount_search(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    let mut row = sorted_by_comments();
    row.as_object_mut().expect("row").remove("sort");
    row["fields"] = json!(["commentsCount"]);
    let data = search(&server, row).await;
    assert_eq!(counts(&data), [json!(59), json!(1), json!(38)], "{data}");
    let warnings = data["warnings"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        warnings
            .iter()
            .filter(|warning| warning
                .as_str()
                .is_some_and(|text| text.contains("conversation comments only")))
            .count(),
        1,
        "{data}"
    );
}
