//! Rows carry the text classification judges (a body, a commit's message
//! body) only when it reads them or the caller picked them, and the requests
//! to GitHub stay the same.
use super::*;
use wiremock::MockServer;

fn pull(number: u64, body: &str) -> Value {
    json!({"number": number, "title": "tidy", "state": "closed", "body": body,
        "user": {"login": "dev"}, "created_at": "2026-01-01T00:00:00Z",
        "pull_request": {"merged_at": "2026-01-02T00:00:00Z"}})
}

async fn search(server: &MockServer, query: Value, scanner: &impl ContentScan) -> Value {
    let provider = one_shot_provider(server);
    let query = serde_json::from_value(query).expect("query");
    execute(&provider, &query, &default_context(), scanner)
        .await
        .expect("history search")
}

async fn pulls(server_items: Vec<Value>) -> MockServer {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/issues",
        200,
        json!({"total_count": server_items.len(), "incomplete_results": false, "items": server_items}),
    )
    .await;
    server
}

fn base(extra: Value) -> Value {
    let mut query = json!({"operation":"pullRequest","owner":"o","repo":"r","keywords":["tidy"]});
    query
        .as_object_mut()
        .expect("object")
        .extend(extra.as_object().expect("object").clone());
    query
}

const JUDGE: &str = r#"{"classification":{"predicates":["Is this a bug fix?"]}}"#;

fn judged() -> Value {
    serde_json::from_str(JUDGE).expect("classification")
}

#[tokio::test]
async fn a_judged_page_carries_each_body_and_a_plain_page_none() {
    let server = pulls(vec![pull(1, "fixes a crash"), pull(2, "")]).await;
    let plain = search(&server, base(json!({})), &Passthrough).await;
    assert!(plain["pullRequests"][0].get("body").is_none(), "{plain}");
    let judged = search(&server, base(judged()), &Passthrough).await;
    assert_eq!(
        judged["pullRequests"][0]["body"], "fixes a crash",
        "{judged}"
    );
    // An empty body is no text: the row judges on its title alone.
    assert!(judged["pullRequests"][1].get("body").is_none(), "{judged}");
    // The judge's text costs no request.
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 2, "one search each: {requests:?}");
}

#[tokio::test]
async fn a_picked_body_is_returned_without_classification() {
    let server = pulls(vec![pull(1, "fixes a crash")]).await;
    let data = search(
        &server,
        base(json!({"fields":["title","body"]})),
        &Passthrough,
    )
    .await;
    assert_eq!(data["pullRequests"][0]["body"], "fixes a crash", "{data}");
    assert!(data.get("warnings").is_none(), "{data}");
}

#[tokio::test]
async fn a_pick_that_leaves_the_body_out_still_shows_the_judge_the_row_body() {
    let server = pulls(vec![pull(1, "fixes a crash")]).await;
    let mut query = base(json!({"fields":["title"]}));
    query["classification"] = judged()["classification"].clone();
    let data = search(&server, query, &Passthrough).await;
    assert_eq!(data["pullRequests"][0]["body"], "fixes a crash", "{data}");
}

#[tokio::test]
async fn a_commit_row_carries_its_message_body_for_the_judge() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/search/commits",
        200,
        json!({"total_count": 2, "incomplete_results": false, "items": [
            {"sha": "aaa", "commit": {"message": "tidy\n\nfixes a crash in the cache",
                "author": {"name": "Dev", "date": "2026-01-01T00:00:00Z"}}},
            {"sha": "bbb", "commit": {"message": "tidy",
                "author": {"name": "Dev", "date": "2026-01-01T00:00:00Z"}}},
        ]}),
    )
    .await;
    let query = json!({"operation":"commit","owner":"o","repo":"r","keywords":["tidy"],
        "classification":{"predicates":["Is this a bug fix?"]}});
    let data = search(&server, query, &Passthrough).await;
    assert_eq!(
        data["commits"][0]["messageBody"], "fixes a crash in the cache",
        "{data}"
    );
    assert!(data["commits"][1].get("messageBody").is_none(), "{data}");
}

#[tokio::test]
async fn a_secret_in_a_body_is_redacted_before_the_judge_reads_it() {
    let secret = "c7Tvo+hBKqFYY+kv5ZJr3J1TWDtk";
    let server = pulls(vec![pull(1, &format!("key = \"{secret}\""))]).await;
    let data = search(&server, base(judged()), &ContextOnly).await;
    let rendered = data.to_string();
    assert!(!rendered.contains(secret), "{rendered}");
    assert!(rendered.contains("[REDACTED-ENVVARSECRETS]"), "{rendered}");
}
