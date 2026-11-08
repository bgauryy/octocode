//! A repeat search reads no file twice: a read is pinned to the commit the
//! index matched, so the transport keeps its answer for good
//! ([`octocode_github::SavedAnswers`]) and a new commit is read again.
use super::*;
use crate::tools::gh_shared::test_support::mock_endpoint;
use octocode_github::{GitHubBudget, GitHubProvider, GitHubTransport, RetryPolicy};
use std::sync::Arc;

const NEXT_SHA: &str = "89abcdef0123456789abcdef0123456789abcdef";

/// A provider whose transport keeps answers, as a persistent runtime does.
fn keeping_provider(server: &MockServer) -> GitHubProvider {
    let mut transport = GitHubTransport::new(
        mock_endpoint(server),
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    transport.set_saved_answers(Some(Arc::new(
        crate::providers::saved_answers::KeptAnswers::new(None),
    )));
    GitHubProvider::new(transport)
}

/// One hit of `keyword` in `app.rs`, indexed at `commit`.
async fn mount_hit(server: &MockServer, keyword: &str, commit: &str) {
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param_contains("q", keyword))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"blob1",
                 "html_url":format!("https://github.com/a/b/blob/{commit}/app.rs"),
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}}
            ]
        })))
        .mount(server)
        .await;
}

async fn search(provider: &GitHubProvider, keyword: &str) -> serde_json::Value {
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":[keyword]}))
            .expect("query");
    execute(
        provider,
        &query,
        &default_context(),
        &Passthrough,
        &crate::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search")
    .data
}

async fn content_requests(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .count()
}

#[tokio::test]
async fn a_repeat_reads_no_file_and_a_new_commit_reads_it_again() {
    let server = MockServer::builder().start().await;
    mount_hit(&server, "alpha", SHA).await;
    mount_hit(&server, "beta", NEXT_SHA).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/app.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64",
            "content":STANDARD.encode("fn alpha() {}\nfn beta() {}\n")
        })))
        .mount(&server)
        .await;
    let provider = keeping_provider(&server);

    let first = search(&provider, "alpha").await;
    assert_eq!(first["files"][0]["lines"], json!(["1\tfn alpha() {}"]));
    assert_eq!(content_requests(&server).await, 1, "{first}");

    let repeat = search(&provider, "alpha").await;
    assert_eq!(repeat["files"], first["files"]);
    assert_eq!(
        content_requests(&server).await,
        1,
        "the same hit at the same commit is not read again: {repeat}"
    );

    let moved = search(&provider, "beta").await;
    assert_eq!(moved["files"][0]["commitSha"], NEXT_SHA, "{moved}");
    assert_eq!(
        content_requests(&server).await,
        2,
        "a hit indexed at another commit is read again: {moved}"
    );
}
