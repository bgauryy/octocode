mod support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn github_file_read_goes_through_execute_and_redacts() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(format!("one\nneedle ghp_{}\nthree\n", "a".repeat(37))),
            "size": 22,
            "sha": "f".repeat(40),
            "path": "src/lib.rs"
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({
            "owner": "a",
            "repo": "b",
            "path": "src/lib.rs",
            "branch": "main",
            "forceRefresh": true,
            "chunkType": "lines",
            "limit": 2
        }),
    )
    .await
    .expect("github read");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let file = &row_data(&outcome)["files"][0];
    let content = file["content"].as_str().unwrap_or("");
    assert!(content.contains("one\n"), "{content}");
    assert!(
        content.contains("[REDACTED"),
        "expected secret redaction, got {content}"
    );
    assert!(file["next"]["continue"]["query"].is_object());
    runtime.close().await;
}

#[tokio::test]
async fn github_missing_identity_is_a_contract_error() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b"}),
    )
    .await
    .expect_err("missing sha");
    assert_eq!(error.code, "invalidInput");
    runtime.close().await;
}
