//! A single PR connection also avoids a separate REST metadata read.
use super::*;

#[tokio::test]
async fn one_requested_connection_and_its_header_use_one_graphql_request() {
    for (section, connection) in [
        (
            "files",
            json!({"changedFiles":1, "files":{
                "pageInfo":{"hasNextPage":false}, "nodes":[
                    {"path":"src/a.rs", "additions":1, "deletions":0, "changeType":"ADDED"}
                ]
            }}),
        ),
        (
            "comments",
            json!({"commentsConn":{
                "pageInfo":{"hasNextPage":false}, "nodes":[
                    {"databaseId":11,"author":{"login":"amy","__typename":"User"},
                     "body":"Looks good.","createdAt":"2026-01-01T00:00:00Z","url":"https://x"}
                ]
            }}),
        ),
        (
            "reviews",
            json!({"reviews":{
                "pageInfo":{"hasNextPage":false}, "nodes":[
                    {"databaseId":12,"author":{"login":"amy"},"state":"APPROVED",
                     "body":"Reviewed.","submittedAt":"2026-01-01T00:00:00Z","commit":{"oid":HEAD}}
                ]
            }}),
        ),
        (
            "commits",
            json!({"commits":{
                "pageInfo":{"hasNextPage":false}, "nodes":[{"commit":{
                    "oid":HEAD,"message":"Fix parser","messageHeadline":"Fix parser",
                    "authoredDate":"2026-01-01T00:00:00Z","committedDate":"2026-01-01T00:00:00Z",
                    "author":{"name":"Amy","user":{"login":"amy"}}
                }}]
            }}),
        ),
    ] {
        let server = MockServer::builder().start().await;
        let mut answer = graphql_pr(connection);
        answer["data"]["repository"]["pullRequest"]
            .as_object_mut()
            .unwrap()
            .remove("labels");
        mount_graphql(&server, answer).await;
        let out = read(
            &server,
            json!({
                "operation":"pullRequest", "owner":"a", "repo":"b", "number":7,
                "sections":[section]
            }),
        )
        .await;
        let pr = &out.data["pullRequests"][0];
        assert_eq!(pr["headCommitSha"], HEAD, "{section}: {pr}");
        assert_eq!(
            pr[section].as_array().map(Vec::len),
            Some(1),
            "{section}: {pr}"
        );
        let requests = server.received_requests().await.expect("recorded requests");
        assert_eq!(requests.len(), 1, "{section}: {requests:?}");
        assert_eq!(requests[0].url.path(), "/api/graphql", "{section}");
        let sent: Value = serde_json::from_slice(&requests[0].body).expect("GraphQL request");
        assert!(
            !sent["query"].as_str().unwrap().contains("labels("),
            "{sent}"
        );
        assert!(sent["variables"].get("labels").is_none(), "{sent}");
    }
}
