//! A path read of a renamed repository (GitHub answers 301 to `/repositories/<id>`) reads
//! the canonical repository's content, not an empty or missing one.

use crate::support::{Workspace, call, row_data, row_status};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use octocode_test_support::json_fixture;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn redirect(server: &MockServer, from: &str, to: &str) {
    Mock::given(method("GET"))
        .and(path(from))
        .respond_with(
            ResponseTemplate::new(301)
                .insert_header("location", format!("{}{to}?ref={SHA}", server.uri())),
        )
        .mount(server)
        .await;
}

async fn canonical_metadata(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"default_branch": "main", "full_name": "c/d"}"#,
        )))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_file_read_at_a_commit_follows_the_rename_to_the_content() {
    let server = MockServer::builder().start().await;
    redirect(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib.rs",
        "/api/v3/repositories/1/contents/src%2Flib.rs",
    )
    .await;
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "src/lib.rs").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/contents/src%2Flib.rs"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("fn renamed() {}\n"),
            "size":16,"sha":"f".repeat(40),"path":"src/lib.rs"
        })))
        .mount(&server)
        .await;
    canonical_metadata(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/c/d/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA}),
    )
    .await
    .expect("read");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    assert!(data["content"].to_string().contains("fn renamed"), "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn a_directory_listing_at_a_commit_follows_the_rename_to_the_entries() {
    let server = MockServer::builder().start().await;
    redirect(
        &server,
        "/api/v3/repos/a/b/contents/src",
        "/api/v3/repositories/1/contents/src",
    )
    .await;
    octocode_test_support::mock_http::list_in_trees(&server, "a", "b", "src/lib.rs").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/contents/src"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"[
                {
                    "name": "lib.rs",
                    "path": "src/lib.rs",
                    "type": "file",
                    "size": 10
                }
            ]"#,
        )))
        .mount(&server)
        .await;
    canonical_metadata(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner":"a","repo":"b","path":"src","ref":SHA}),
    )
    .await
    .expect("listing");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    assert!(data.to_string().contains("lib.rs"), "{data}");
    runtime.close().await;
}
