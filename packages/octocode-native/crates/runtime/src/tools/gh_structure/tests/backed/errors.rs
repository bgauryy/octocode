//! Listing failures: a missing path, a missing branch, a file path.
use super::*;
use octocode_github::ProviderErrorKind;
use octocode_github::mock_http::mount_json;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn tree_missing_path_on_existing_branch_does_not_fall_back() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/missing"))
        .and(query_param("ref", TREE_SHA))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    // The path exists on the default branch only: the read must report
    // it missing on `dev`, not show the default-branch copy.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/missing"))
        .and(query_param("ref", "main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"x.rs","path":"missing/x.rs","type":"file","size":1}
        ])))
        .mount(&server)
        .await;
    mount_ref(&server, "dev").await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main"}),
    )
    .await;
    let error = run(
        &server,
        json!({"operation":"tree", "owner":"a","repo":"b","ref":"dev","path":"missing"}),
    )
    .await
    .expect_err("path missing on an existing branch");
    assert_eq!(error.kind, ProviderErrorKind::NotFound);
}

/// An explicit ref that does not exist is an error naming it,
/// never a silent listing of the default branch.
#[tokio::test]
async fn tree_missing_branch_is_an_error_not_a_default_branch_listing() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/gone",
        422,
        json!({"message":"No commit found for SHA: gone"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"x.rs","path":"x.rs","type":"file","size":1}
        ]),
    )
    .await;
    let error = run(
        &server,
        json!({"operation":"tree", "owner":"a","repo":"b","ref":"gone"}),
    )
    .await
    .expect_err("a missing ref is an error");
    assert_eq!(error.kind, ProviderErrorKind::NotFound);
    assert_eq!(
        error.reason,
        Some(octocode_github::ProviderErrorReason::RefNotFound)
    );
    assert!(error.message.contains("\"gone\""), "{}", error.message);
}

#[tokio::test]
async fn tree_on_a_file_path_is_a_clear_error() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib.rs",
        200,
        json!({
            "name":"lib.rs","path":"src/lib.rs","type":"file","size":3,"sha":"1"
        }),
    )
    .await;
    let error = run(
        &server,
        json!({"operation":"tree", "owner":"a","repo":"b","ref":"main","path":"src/lib.rs"}),
    )
    .await
    .expect_err("file path");
    assert_eq!(error.kind, ProviderErrorKind::Validation);
    assert!(error.message.contains("is a file"), "{}", error.message);
}
