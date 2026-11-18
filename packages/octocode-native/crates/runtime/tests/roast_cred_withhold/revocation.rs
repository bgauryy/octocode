//! A credential the origin refuses (401) keeps no saved answer: a
//! commit-pinned answer that would serve unasked is read again.
use crate::support::{Workspace, call, row_status};
use octocode_test_support::json_fixture;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn contents_reads(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path().ends_with("/contents/notes.txt"))
        .count()
}

#[tokio::test]
async fn a_refused_credential_stops_serving_its_pinned_answers() {
    let server = MockServer::builder().start().await;
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "notes.txt").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/notes.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            b"plain notes\n".to_vec(),
            "application/vnd.github.raw; charset=utf-8",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json_fixture(r#"{"message": "Bad credentials"}"#)),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let read = json!({"owner":"a","repo":"b","path":"notes.txt","ref":SHA});
    let first = call(&runtime, "ghGetFileContent", read.clone())
        .await
        .expect("read");
    assert_eq!(row_status(&first), "success");
    call(&runtime, "ghGetFileContent", read.clone())
        .await
        .expect("repeat");
    assert_eq!(
        contents_reads(&server).await,
        1,
        "the pinned answer serves unasked"
    );
    let refused = call(
        &runtime,
        "ghSearchRepo",
        json_fixture(r#"{"operation": "search", "keywords": ["x"]}"#),
    )
    .await
    .expect("search");
    assert_eq!(
        row_status(&refused),
        "error",
        "{}",
        refused.structured_content
    );
    let refusal = refused.structured_content.to_string();
    assert!(
        refusal.contains("authentication"),
        "the origin answered 401: {refusal}"
    );
    // The refused credential serves nothing more from its saved answers: the
    // read either goes back to GitHub or fails with the refusal.
    let after = call(&runtime, "ghGetFileContent", read)
        .await
        .expect("after the refusal");
    let served = after.structured_content.to_string();
    let reread = contents_reads(&server).await == 2;
    assert!(
        reread || !served.contains("plain notes"),
        "a refused credential still serves its pinned answer: {served}"
    );
    runtime.close().await;
}
